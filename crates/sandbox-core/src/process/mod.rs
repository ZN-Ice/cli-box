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

/// Process manager for launching and managing apps/CLIs in the sandbox
pub struct ProcessManager;

impl ProcessManager {
    /// Launch a macOS .app by path using the `open` command.
    /// This avoids ObjC NSExceptions that crash the Rust process.
    /// Returns (ProcessInfo, Option<SCWindow ID>) — the window is discovered by
    /// searching for a title containing the app's stem name after a short delay.
    #[cfg(target_os = "macos")]
    pub fn spawn_app(app_path: &str) -> Result<ProcessInfo> {
        let (info, _window_id) = Self::spawn_app_with_window(app_path)?;
        Ok(info)
    }

    /// Launch a macOS .app and discover its SCWindow ID.
    /// Returns both the process info and the discovered window ID (if found).
    #[cfg(target_os = "macos")]
    pub fn spawn_app_with_window(app_path: &str) -> Result<(ProcessInfo, Option<u32>)> {
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

        let output = std::process::Command::new("open")
            .arg(app_path)
            .output()
            .map_err(|e| AppError::Process(format!("Failed to run `open` command: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Process(format!(
                "Failed to launch app: {app_path} ({stderr})"
            )));
        }

        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let info = ProcessInfo {
            pid: id,
            name: app_name.clone(),
            path: Some(app_path.to_string()),
            is_running: true,
        };

        // Wait for the app window to appear, then discover its SCWindow ID
        std::thread::sleep(std::time::Duration::from_millis(800));
        let window_id = crate::capture::ScreenCapture::find_window_by_title(&app_name).ok();

        info!(
            "Launched app: {} (tracked_id={}, window_id={:?})",
            app_path, id, window_id
        );

        Ok((info, window_id))
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
                            // Persist to SQLite (survives reconnections)
                            if let Err(e) = thread_store.append(&text) {
                                warn!("PTY reader {tracked_id}: store append failed: {e}");
                            }
                            // Real-time broadcast to current subscribers
                            let _ = thread_tx.send(text);
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
                name: s.command.clone(),
                path: None,
                is_running: true,
            })
            .collect();
        Ok(processes)
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
