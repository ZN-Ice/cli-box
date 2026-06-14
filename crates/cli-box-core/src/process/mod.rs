use crate::error::{AppError, Result};
use crate::pty_store::PtyStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::{debug, info, trace, warn};

#[cfg(target_os = "macos")]
use {
    nix::sys::signal::{kill, Signal},
    nix::unistd::Pid,
    portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize},
};

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    /// Actual OS process ID (for PTY sessions, this is the child process PID).
    /// For app spawns, this equals `pid`. For CLI PTY spawns, `pid` is the
    /// internal tracked_id while `os_pid` is the real OS PID.
    #[serde(default)]
    pub os_pid: Option<u32>,
    pub name: String,
    pub path: Option<String>,
    pub is_running: bool,
}

/// PTY session holding the writer handle and a background reader thread.
///
/// A dedicated reader thread continuously reads PTY output into a shared
/// SQLite-backed PtyStore. Output persists across WebSocket reconnections
/// and supports late-subscriber replay.
#[cfg(target_os = "macos")]
struct PtySession {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn MasterPty>,
    #[allow(dead_code)]
    child_pid: u32,
    command: String,
    /// SQLite-backed persistent output store (replaces VecDeque buffer)
    store: Arc<PtyStore>,
    /// Flag to signal the reader thread to stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the reader thread (for join on cleanup)
    reader_thread: Option<std::thread::JoinHandle<()>>,
    /// Broadcast sender for streaming PTY output to WebSocket subscribers
    output_tx: broadcast::Sender<String>,
}

/// Track active PTY sessions by sandbox-tracked PID
static SESSIONS: std::sync::LazyLock<Mutex<HashMap<u32, PtySession>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Counter for generating unique tracked PIDs
static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1000);

/// Known Chromium-based app bundle identifiers.
/// These apps enforce single-instance and need `open -g -n --args --user-data-dir`
/// to create isolated processes that don't interfere with the user's existing browser.
const CHROMIUM_BUNDLE_IDS: &[&str] = &[
    "com.google.Chrome",
    "com.google.Chrome.beta",
    "com.google.Chrome.dev",
    "com.google.Chrome.canary",
    "com.microsoft.edgemac",
    "com.microsoft.edgemac.Dev",
    "com.microsoft.edgemac.Beta",
    "com.brave.Browser",
    "com.brave.Browser.beta",
    "com.brave.Browser.nightly",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "com.operasoftware.OperaNext",
    "company.thebrowser.Browser", // Arc
];

/// Read `CFBundleIdentifier` from an app bundle's Info.plist.
fn read_bundle_id(app_path: &str) -> Option<String> {
    let plist_path = std::path::Path::new(app_path).join("Contents/Info.plist");
    let data = std::fs::read_to_string(plist_path).ok()?;
    let marker = "<key>CFBundleIdentifier</key>";
    let idx = data.find(marker)?;
    let after = &data[idx + marker.len()..];
    let start = after.find("<string>")? + 8;
    let end = after.find("</string>")?;
    Some(after[start..end].to_string())
}

/// Check if an app is Chromium-based by its bundle identifier.
fn is_chromium_app(app_path: &str) -> bool {
    match read_bundle_id(app_path) {
        Some(id) => CHROMIUM_BUNDLE_IDS
            .iter()
            .any(|known| id == *known || id.starts_with(&format!("{known}."))),
        None => false,
    }
}

/// Unique user-data-dir path for a Chromium sandbox instance.
fn chromium_user_data_dir(sandbox_id: &str) -> String {
    format!("/tmp/cli-box-chromium/{sandbox_id}")
}

/// Clean up temporary Chromium user-data-dir for a sandbox.
pub fn cleanup_chromium_data(sandbox_id: &str) {
    let dir = chromium_user_data_dir(sandbox_id);
    if std::path::Path::new(&dir).exists() {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            warn!("Failed to cleanup Chromium data dir {dir}: {e}");
        } else {
            debug!("Cleaned up Chromium data dir: {dir}");
        }
    }
}

/// Process manager for launching and managing apps/CLIs in the sandbox
pub struct ProcessManager;

impl ProcessManager {
    /// Launch a macOS .app by path using the `open` command.
    /// This avoids ObjC NSExceptions that crash the Rust process.
    /// Returns (ProcessInfo, Option<SCWindow ID>) — the window is discovered by
    /// searching for a title containing the app's stem name after a short delay.
    #[cfg(target_os = "macos")]
    pub fn spawn_app(app_path: &str) -> Result<ProcessInfo> {
        let (info, _window_id) = Self::spawn_app_with_window(app_path, None)?;
        Ok(info)
    }

    /// Launch a macOS .app and discover its SCWindow ID.
    /// Returns both the process info and the discovered window ID (if found).
    ///
    /// `sandbox_id` is used to create isolated user-data-dirs for Chromium apps,
    /// preventing them from connecting to the user's existing browser process.
    #[cfg(target_os = "macos")]
    pub fn spawn_app_with_window(
        app_path: &str,
        sandbox_id: Option<&str>,
    ) -> Result<(ProcessInfo, Option<u32>)> {
        let path = std::path::Path::new(app_path);
        if !path.exists() {
            return Err(AppError::Process(format!(
                "App path does not exist: {app_path}"
            )));
        }

        let app_name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Check if app is already running before launching
        let pre_existing_pids = Self::find_pids_by_app_name(&app_name);

        // Launch strategy:
        // - Chromium apps: `open -g -n --args --user-data-dir` for process isolation
        // - Other apps: `open -g -n` to force a new instance without focus steal
        let chromium_launch = is_chromium_app(app_path) && sandbox_id.is_some();

        if chromium_launch {
            let sid = sandbox_id.unwrap();
            let user_data_dir = chromium_user_data_dir(sid);
            info!(
                "Launching Chromium app in isolated mode: app_path={app_path}, user_data_dir={user_data_dir}"
            );
            // -n: force new instance (even if app is already running)
            // --args: pass --user-data-dir to the binary for process isolation
            // NOTE: We do NOT use -g because CGEvents require the target
            // app to be the key window. Chrome launched with -g won't receive input.
            let output = std::process::Command::new("open")
                .arg("-n")
                .arg(app_path)
                .arg("--args")
                .arg(format!("--user-data-dir={user_data_dir}"))
                .arg("--no-first-run")
                .arg("--disable-default-apps")
                .arg("--disable-extensions")
                .arg("--new-window")
                .output()
                .map_err(|e| AppError::Process(format!("Failed to run `open` command: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AppError::Process(format!(
                    "Failed to launch Chromium app: {app_path} ({stderr})"
                )));
            }
        } else {
            let output = std::process::Command::new("open")
                .arg("-g")
                .arg("-n")
                .arg(app_path)
                .output()
                .map_err(|e| AppError::Process(format!("Failed to run `open` command: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AppError::Process(format!(
                    "Failed to launch app: {app_path} ({stderr})"
                )));
            }
        }

        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Wait for the app to start and window to appear.
        // Chromium apps need longer due to multi-process startup.
        let initial_wait = if chromium_launch { 2000 } else { 800 };
        std::thread::sleep(std::time::Duration::from_millis(initial_wait));

        // Retry window discovery — Chromium apps may need up to 5s for window creation
        let mut window_id: Option<u32> = None;
        let max_retries = if chromium_launch { 5 } else { 1 };

        for attempt in 0..max_retries {
            // Try to find window by title first
            window_id = crate::capture::ScreenCapture::find_window_by_title(&app_name).ok();

            // If window not found by title, try finding by PID
            if window_id.is_none() {
                let current_pids = Self::find_pids_by_app_name(&app_name);
                let new_pids: Vec<u32> = current_pids
                    .iter()
                    .filter(|pid| !pre_existing_pids.contains(pid))
                    .copied()
                    .collect();
                let target_pid = new_pids.first().or(current_pids.first()).copied();

                if let Some(pid) = target_pid {
                    window_id = crate::capture::ScreenCapture::find_window_by_pid(pid).ok();
                }
            }

            if window_id.is_some() {
                info!(
                    "[window_discovery] Found window on attempt {attempt}: wid={:?}",
                    window_id
                );
                break;
            }

            if attempt < max_retries - 1 {
                debug!("[window_discovery] Attempt {attempt}: no window found, retrying...");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        if window_id.is_none() {
            warn!(
                "[window_discovery] Failed to find window after {max_retries} attempts for {app_name}"
            );
        }

        // OS PID is not directly available from `open -g`; the window_id
        // is the primary handle for app sandboxes.
        let os_pid: Option<u32> = None;

        let info = ProcessInfo {
            pid: id,
            os_pid,
            name: app_name.clone(),
            path: Some(app_path.to_string()),
            is_running: true,
        };

        info!(
            "Launched app: {} (tracked_id={}, os_pid={:?}, window_id={:?}, chromium_isolated={})",
            app_path, id, os_pid, window_id, chromium_launch
        );

        Ok((info, window_id))
    }

    /// Find PIDs for an app by its display name using `pgrep`.
    #[cfg(target_os = "macos")]
    fn find_pids_by_app_name(app_name: &str) -> Vec<u32> {
        let binary_name = app_name.to_lowercase().replace(' ', "-");
        let mut pids = Vec::new();

        for name in [&binary_name, app_name] {
            if let Ok(output) = std::process::Command::new("pgrep")
                .arg("-x")
                .arg(name)
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if !pids.contains(&pid) {
                            pids.push(pid);
                        }
                    }
                }
            }
        }
        pids
    }

    #[cfg(not(target_os = "macos"))]
    pub fn spawn_app(app_path: &str) -> Result<ProcessInfo> {
        let _ = app_path;
        Err(AppError::Process(
            "spawn_app only supported on macOS".into(),
        ))
    }

    /// Launch a CLI process with PTY support (default 80x24)
    #[cfg(target_os = "macos")]
    pub fn spawn_cli(command: &str, args: &[String]) -> Result<ProcessInfo> {
        Self::spawn_cli_with_size(command, args, 80, 24)
    }

    /// Launch a CLI process with PTY support and custom terminal dimensions.
    #[cfg(target_os = "macos")]
    pub fn spawn_cli_with_size(
        command: &str,
        args: &[String],
        cols: u16,
        rows: u16,
    ) -> Result<ProcessInfo> {
        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Process(format!("Failed to open PTY: {e}")))?;

        let mut cmd = CommandBuilder::new(command);
        cmd.args(args);
        // Ensure PTY child processes have proper terminal environment.
        // TUI apps (opencode, vim, htop) check TERM to decide whether to render.
        // When launched from a GUI app (Tauri), TERM may be missing.
        cmd.env("TERM", "xterm-256color");
        if std::env::var("COLORTERM").is_err() {
            cmd.env("COLORTERM", "truecolor");
        }
        if std::env::var("LANG").is_err() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AppError::Process(format!("Failed to spawn command: {e}")))?;

        let child_pid = child.process_id();
        // Drop slave - the child process owns it now
        drop(pty_pair.slave);

        let reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| AppError::Process(format!("Failed to clone PTY reader: {e}")))?;
        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| AppError::Process(format!("Failed to take PTY writer: {e}")))?;

        let tracked_id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Create SQLite-backed persistent store and stop flag for the reader thread
        let store = PtyStore::new_in_memory(&tracked_id.to_string())?;
        let stop_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

        // Create broadcast channel for streaming output to WebSocket subscribers
        let (output_tx, _) = broadcast::channel::<String>(256);
        let thread_tx = output_tx.clone();

        let thread_store = Arc::clone(&store);
        let thread_stop = Arc::clone(&stop_flag);

        // Spawn a dedicated reader thread that continuously reads PTY output
        let reader_thread = std::thread::Builder::new()
            .name(format!("pty-reader-{tracked_id}"))
            .spawn(move || {
                let mut reader = reader;
                let mut read_buf = [0u8; 4096];
                loop {
                    if thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        debug!("PTY reader thread {tracked_id}: stop flag set, exiting");
                        break;
                    }
                    match std::io::Read::read(&mut reader, &mut read_buf) {
                        Ok(0) => {
                            debug!("PTY reader thread {tracked_id}: EOF (process exited)");
                            break;
                        }
                        Ok(n) => {
                            let text = String::from_utf8_lossy(&read_buf[..n]).to_string();
                            let preview: String = text.chars().take(40).collect();
                            info!(
                                "[PTY-READ] pid={tracked_id}: read {} bytes, preview={:?}",
                                n, preview
                            );
                            // Persist to SQLite (survives reconnections)
                            if let Err(e) = thread_store.append(&text) {
                                warn!("PTY reader {tracked_id}: store append failed: {e}");
                            }
                            // Real-time broadcast to current subscribers
                            let receiver_count = thread_tx.receiver_count();
                            let _ = thread_tx.send(text);
                            info!(
                                "[PTY-READ] pid={tracked_id}: broadcast sent, receivers={}",
                                receiver_count
                            );
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            trace!("PTY reader thread {tracked_id}: interrupted, retrying");
                            continue;
                        }
                        Err(e) => {
                            warn!("PTY reader thread {tracked_id}: read error: {e}");
                            break;
                        }
                    }
                }
                debug!("PTY reader thread {tracked_id}: thread exiting");
            })
            .map_err(|e| AppError::Process(format!("Failed to spawn reader thread: {e}")))?;

        let mut sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        sessions.insert(
            tracked_id,
            PtySession {
                writer,
                master: pty_pair.master,
                child_pid: child_pid.unwrap_or(0),
                command: command.to_string(),
                store,
                stop_flag,
                reader_thread: Some(reader_thread),
                output_tx,
            },
        );

        info!(
            "Spawned CLI: {} (tracked_id={}, os_pid={:?}, {}x{})",
            command, tracked_id, child_pid, cols, rows
        );

        Ok(ProcessInfo {
            pid: tracked_id,
            os_pid: child_pid,
            name: command.to_string(),
            path: None,
            is_running: true,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn spawn_cli(command: &str, args: &[String]) -> Result<ProcessInfo> {
        Self::spawn_cli_with_size(command, args, 80, 24)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn spawn_cli_with_size(
        _command: &str,
        _args: &[String],
        _cols: u16,
        _rows: u16,
    ) -> Result<ProcessInfo> {
        Err(AppError::Process(
            "spawn_cli_with_size only supported on macOS".into(),
        ))
    }

    /// List all running processes in the sandbox
    pub fn list_processes() -> Result<Vec<ProcessInfo>> {
        let sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        let processes: Vec<ProcessInfo> = sessions
            .iter()
            .map(|(id, s)| ProcessInfo {
                pid: *id,
                os_pid: Some(s.child_pid).filter(|&p| p != 0),
                name: s.command.clone(),
                path: None,
                is_running: true,
            })
            .collect();
        Ok(processes)
    }

    /// Check if a PTY session with the given tracked PID is still alive.
    pub fn is_session_alive(tracked_pid: u32) -> bool {
        SESSIONS
            .lock()
            .map(|sessions| sessions.contains_key(&tracked_pid))
            .unwrap_or(false)
    }

    /// Kill a process by tracked PID
    #[cfg(target_os = "macos")]
    pub fn kill_process(pid: u32) -> Result<()> {
        // Step 1: Remove session from SESSIONS (brief lock)
        let mut session = {
            let mut sessions = SESSIONS
                .lock()
                .map_err(|e| AppError::Process(e.to_string()))?;
            sessions
                .remove(&pid)
                .ok_or_else(|| AppError::Process(format!("Process {pid} not found in sandbox")))?
        };

        let os_pid = session.child_pid;

        // Step 2: Signal the reader thread to stop
        session
            .stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // Step 3: Kill the actual OS child process
        if os_pid > 0 {
            kill(Pid::from_raw(os_pid as i32), Signal::SIGTERM)
                .map_err(|e| AppError::Process(format!("Failed to kill process {os_pid}: {e}")))?;
        }

        // Step 4: Join the reader thread.
        // drop(session) closes the PTY master fd, which causes the reader
        // thread's blocking read() to return an error → thread exits.
        // This ordering is critical — if join() ran before drop(), the
        // reader thread could block forever on read().
        let reader_thread = session.reader_thread.take();
        drop(session);

        if let Some(handle) = reader_thread {
            match handle.join() {
                Ok(()) => debug!("PTY reader thread for pid={pid} joined successfully"),
                Err(_) => warn!("PTY reader thread for pid={pid} panicked"),
            }
        }

        // Step 5: Session already dropped above (closes PTY master, writer, etc.)
        info!("Killed process: tracked_id={}, os_pid={}", pid, os_pid);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn kill_process(pid: u32) -> Result<()> {
        let _ = pid;
        Err(AppError::Process(
            "kill_process only supported on macOS".into(),
        ))
    }

    /// Send input to a PTY process
    #[cfg(target_os = "macos")]
    pub fn send_input(pid: u32, data: &[u8]) -> Result<()> {
        info!(
            "[pty] send_input: pid={}, len={}, preview={:?}",
            pid,
            data.len(),
            if data.len() > 40 {
                String::from_utf8_lossy(&data[..40]).to_string()
            } else {
                String::from_utf8_lossy(data).to_string()
            }
        );
        let mut sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        if let Some(session) = sessions.get_mut(&pid) {
            session
                .writer
                .write_all(data)
                .map_err(|e| AppError::Process(format!("Failed to write to PTY: {e}")))?;
            session
                .writer
                .flush()
                .map_err(|e| AppError::Process(format!("Failed to flush PTY: {e}")))?;
            info!("[pty] send_input: written and flushed to pid={}", pid);
            Ok(())
        } else {
            let available: Vec<u32> = sessions.keys().copied().collect();
            warn!(
                "[pty] send_input: pid={} not found. Available PIDs: {:?}",
                pid, available
            );
            Err(AppError::Process(format!("Process {pid} not found")))
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn send_input(_pid: u32, _data: &[u8]) -> Result<()> {
        Err(AppError::Process(
            "send_input only supported on macOS".into(),
        ))
    }

    /// Resize a PTY session's terminal dimensions
    #[cfg(target_os = "macos")]
    pub fn resize_pty(pid: u32, cols: u16, rows: u16) -> Result<()> {
        let sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        let session = sessions
            .get(&pid)
            .ok_or_else(|| AppError::Process(format!("Session not found: {pid}")))?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Process(format!("Failed to resize PTY: {e}")))?;
        info!("[pty] resize: pid={}, cols={}, rows={}", pid, cols, rows);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn resize_pty(_pid: u32, _cols: u16, _rows: u16) -> Result<()> {
        Err(AppError::Process(
            "resize_pty only supported on macOS".into(),
        ))
    }

    /// Read output from a PTY process.
    ///
    /// Reads all available data from the SQLite-backed PtyStore.
    /// Non-blocking: returns `Ok(None)` when the store is empty.
    #[cfg(target_os = "macos")]
    pub fn read_output(pid: u32) -> Result<Option<String>> {
        let store = {
            let sessions = SESSIONS
                .lock()
                .map_err(|e| AppError::Process(e.to_string()))?;
            let session = sessions
                .get(&pid)
                .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
            Arc::clone(&session.store)
        }; // SESSIONS lock released here

        let chunks = store.read_all()?;
        if chunks.is_empty() {
            trace!("PTY pid={pid}: no output available");
            return Ok(None);
        }

        let text: String = chunks.into_iter().map(|c| c.data).collect();
        // Clear after reading (HTTP poll mode)
        store.clear()?;
        debug!("PTY pid={pid}: drained {} chars from store", text.len());
        Ok(Some(text))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn read_output(_pid: u32) -> Result<Option<String>> {
        Err(AppError::Process(
            "read_output only supported on macOS".into(),
        ))
    }

    /// Subscribe to PTY output stream for WebSocket streaming.
    /// Returns a broadcast::Receiver that receives output chunks in real-time.
    #[cfg(target_os = "macos")]
    pub fn subscribe_output(pid: u32) -> Result<broadcast::Receiver<String>> {
        let sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        let session = sessions
            .get(&pid)
            .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
        Ok(session.output_tx.subscribe())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn subscribe_output(_pid: u32) -> Result<broadcast::Receiver<String>> {
        Err(AppError::Process(
            "subscribe_output only supported on macOS".into(),
        ))
    }

    /// Get the PtyStore for a session (for WebSocket replay).
    #[cfg(target_os = "macos")]
    pub fn get_store(pid: u32) -> Result<Arc<PtyStore>> {
        let sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        let session = sessions
            .get(&pid)
            .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
        Ok(Arc::clone(&session.store))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn get_store(_pid: u32) -> Result<Arc<PtyStore>> {
        Err(AppError::Process(
            "get_store only supported on macOS".into(),
        ))
    }
}
