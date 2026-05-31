mod client;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sandbox_core::capture::ScreenCapture;
use sandbox_core::instance::InstanceRegistry;
use std::path::PathBuf;
use std::process::Command;

/// macOS Desktop Automation Sandbox CLI
#[derive(Parser)]
#[command(name = "sandbox", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a sandbox with a shell or CLI command in a Tauri window
    Start {
        /// Command to run (e.g., "claude", "zsh", "echo"). Defaults to zsh if omitted.
        command: Option<String>,

        /// Additional arguments passed to the command
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Start with a zsh shell (shorthand for `sandbox start zsh`)
        #[arg(long)]
        shell: bool,
    },

    /// List all sandbox instances
    List,

    /// Show details of a sandbox instance
    Inspect {
        /// Sandbox instance ID
        id: String,
    },

    /// Close a sandbox instance
    Close {
        /// Sandbox instance ID
        id: String,
    },

    /// Type text into a sandbox
    #[command(name = "type")]
    TypeText {
        /// Text to type
        text: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Use PTY write instead of CGEvent (more reliable for CLI processes)
        #[arg(long)]
        pty: bool,
    },

    /// Press a key in a sandbox
    Key {
        /// Key name (e.g., Return, Tab, Escape, space, a-z)
        key: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Modifier keys (e.g., cmd, shift, ctrl, alt)
        #[arg(short, long, num_args = 0..)]
        modifiers: Vec<String>,

        /// Use PTY write instead of CGEvent
        #[arg(long)]
        pty: bool,
    },

    /// Click at coordinates in a sandbox
    Click {
        /// X coordinate
        x: f64,

        /// Y coordinate
        y: f64,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Mouse button (left, right, middle)
        #[arg(long, default_value = "left")]
        button: String,
    },

    /// Take a screenshot of a sandbox window
    Screenshot {
        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: PathBuf,

        /// Sandbox instance ID
        #[arg(long)]
        id: Option<String>,

        /// Window ID to capture (overrides auto-detection)
        #[arg(long)]
        window_id: Option<u32>,
    },

    /// List all visible windows on the system
    Windows,

    /// List processes running inside a sandbox
    Processes {
        /// Sandbox instance ID
        #[arg(long)]
        id: String,
    },

    /// Shutdown the sandbox (legacy, closes first Terminal window)
    Shutdown,

    /// Show log file paths for a sandbox or all sandboxes
    Logs {
        /// Sandbox instance ID (omit to show all log paths)
        id: Option<String>,
    },

    /// Inspect UI tree of a sandbox window
    UiInspect {
        /// Sandbox instance ID
        #[arg(long)]
        id: String,
    },

    /// Find UI elements by role/title
    UiFind {
        /// Sandbox instance ID
        #[arg(long)]
        id: String,
        /// AX role (e.g., AXButton, AXTextField)
        #[arg(long)]
        role: String,
        /// Optional title filter
        #[arg(long)]
        title: Option<String>,
    },

    /// Get value of a UI element
    UiValue {
        /// Sandbox instance ID
        #[arg(long)]
        id: String,
        /// Element ID
        #[arg(long)]
        element_id: String,
    },

    /// Record sandbox actions to a JSONL file
    Record {
        /// Sandbox ID
        #[arg(long)]
        id: String,
        /// Output file path
        #[arg(long, short)]
        output: PathBuf,
    },

    /// Replay actions from a JSONL file
    Playback {
        /// Sandbox ID
        #[arg(long)]
        id: String,
        /// JSONL file to replay
        #[arg(long, short)]
        input: PathBuf,
        /// Speed multiplier (1.0 = real-time)
        #[arg(long, default_value = "1.0")]
        speed: f64,
    },

    /// Compare two screenshots pixel-by-pixel
    Diff {
        /// First screenshot path
        #[arg(long)]
        a: PathBuf,
        /// Second screenshot path
        #[arg(long)]
        b: PathBuf,
        /// Pixel difference threshold (0-255)
        #[arg(long, default_value = "10")]
        threshold: u8,
        /// Output diff image path
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Start MCP stdio server for agent integration
    McpServe,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = sandbox_core::logging::init_cli_logging();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            command,
            args,
            shell,
        } => {
            let (cmd, cmd_args) = match (shell, command) {
                (true, _) | (false, None) => ("zsh".to_string(), args),
                (false, Some(c)) => (c, args),
            };
            cmd_start_daemon(&cmd, &cmd_args).await?;
        }
        Commands::List => {
            cmd_list_daemon().await?;
        }
        Commands::Inspect { id } => {
            cmd_inspect_daemon(&id).await?;
        }
        Commands::Close { id } => {
            cmd_close_daemon(&id).await?;
        }
        Commands::TypeText { text, id, pty } => {
            cmd_type_daemon(&text, &id, pty).await?;
        }
        Commands::Key {
            key,
            id,
            modifiers,
            pty,
        } => {
            cmd_key_daemon(&key, &id, &modifiers, pty).await?;
        }
        Commands::Click { x, y, id, button } => {
            cmd_click_daemon(x, y, &id, &button).await?;
        }
        Commands::Screenshot {
            output,
            id,
            window_id: _window_id,
        } => {
            cmd_screenshot_daemon(&output, id.as_deref()).await?;
        }
        Commands::Windows => {
            cmd_windows()?;
        }
        Commands::Processes { id } => {
            cmd_processes_daemon(&id).await?;
        }
        Commands::Shutdown => {
            cmd_shutdown_daemon().await?;
        }
        Commands::Logs { id } => {
            cmd_logs(id.as_deref())?;
        }
        Commands::UiInspect { id } => {
            cmd_ui_inspect(&id).await?;
        }
        Commands::UiFind { id, role, title } => {
            cmd_ui_find(&id, &role, title.as_deref()).await?;
        }
        Commands::UiValue { id, element_id } => {
            cmd_ui_value(&id, &element_id).await?;
        }
        Commands::Record { id, output } => {
            cmd_record(&id, &output)?;
        }
        Commands::Playback { id, input, speed } => {
            cmd_playback(&id, &input, speed)?;
        }
        Commands::Diff {
            a,
            b,
            threshold,
            output,
        } => {
            cmd_diff(&a, &b, threshold, output.as_deref())?;
        }
        Commands::McpServe => {
            run_mcp_server().await?;
        }
    }

    Ok(())
}

// ── Command Implementations ─────────────────────────────

/// Launch the Tauri sandbox app with the given CLI command inside it (legacy).
///
/// After spawning the Tauri process, polls the instance registry and `/readyz`
/// endpoint to verify the sandbox is actually ready before returning.
#[allow(dead_code)]
async fn cmd_start(command: &str, args: &[String]) -> anyhow::Result<()> {
    let bundle_path = find_tauri_bundle()?;
    let app_binary = bundle_path.join("Contents/MacOS/system-test-sandbox");

    let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
    if !args.is_empty() {
        tauri_args.push("--".to_string());
        tauri_args.extend(args.iter().cloned());
    }

    tracing::info!("[start] bundle_path: {}", bundle_path.display());
    tracing::info!("[start] tauri_args: {:?}", tauri_args);

    let child = Command::new(&app_binary)
        .args(&tauri_args)
        .spawn()
        .context("Failed to launch Tauri sandbox app")?;

    let child_pid = child.id();
    tracing::info!("[start] child pid: {child_pid}");

    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };
    println!("Sandbox starting: {full_cmd} ...");

    let log_dir = sandbox_core::logging::log_base_dir();
    let timeout = std::time::Duration::from_secs(30);
    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(200);

    // Phase 1: Wait for instance registry file to appear
    let registry = sandbox_core::instance::InstanceRegistry::default();
    let instance = loop {
        if start.elapsed() > timeout {
            anyhow::bail!(
                "Timeout: sandbox instance did not appear after {}s.\n\
                 The Tauri process (PID {child_pid}) may have failed to start.\n\
                 Check logs at: {}",
                timeout.as_secs(),
                log_dir.display()
            );
        }
        if let Ok(instances) = registry.list() {
            if let Some(inst) = instances.iter().find(|i| i.pid == child_pid) {
                break inst.clone();
            }
        }
        tokio::time::sleep(poll_interval).await;
    };

    // Check if the instance reported an error during startup
    if let sandbox_core::instance::InstanceStatus::Error(msg) = &instance.status {
        anyhow::bail!(
            "Sandbox failed to start: {msg}\n\
             Instance ID: {}, Port: {}\n\
             Check logs at: {}",
            instance.id,
            instance.port,
            log_dir.display()
        );
    }

    // Phase 2: Wait for HTTP server /readyz to respond
    let client = crate::client::SandboxClient::from_port(instance.port);
    let ready = loop {
        if start.elapsed() > timeout {
            anyhow::bail!(
                "Timeout: sandbox HTTP server not ready after {}s.\n\
                 Instance ID: {}, Port: {}\n\
                 The server may be starting slowly or have encountered an error.\n\
                 Check logs at: {}",
                timeout.as_secs(),
                instance.id,
                instance.port,
                log_dir.display()
            );
        }
        match client.readyz().await {
            Ok(resp) if resp.status == "ready" => break resp,
            Ok(_) => {}  // not_ready, keep polling
            Err(_) => {} // connection refused, keep polling
        }
        // Re-check instance status for errors between polls
        if let Ok(inst) = registry.get(&instance.id) {
            if let sandbox_core::instance::InstanceStatus::Error(msg) = &inst.status {
                anyhow::bail!(
                    "Sandbox failed during startup: {msg}\n\
                     Instance ID: {}, Port: {}\n\
                     Check logs at: {}",
                    instance.id,
                    instance.port,
                    log_dir.display()
                );
            }
        }
        tokio::time::sleep(poll_interval).await;
    };

    println!(
        "Sandbox ready: {} (id={}, port={})",
        full_cmd, instance.id, instance.port
    );
    println!(
        "  pty_active={}, pending_cli={}",
        ready.pty_active, ready.pending_cli
    );
    println!("Log directory: {}", log_dir.display());
    Ok(())
}

// ── Daemon-aware command implementations ─────────────────────

/// Start a sandbox via the daemon: ensures daemon is running, then creates a sandbox.
async fn cmd_start_daemon(command: &str, args: &[String]) -> anyhow::Result<()> {
    let port = match sandbox_core::daemon::find_running_daemon() {
        Some(p) => {
            println!("Sandbox daemon already running on port {p}");
            p
        }
        None => {
            // Spawn the daemon binary
            let daemon_bin = find_daemon_binary()?;
            tracing::info!("[start] spawning daemon: {}", daemon_bin.display());

            let _child = Command::new(&daemon_bin)
                .spawn()
                .context("Failed to launch sandbox-daemon")?;

            // Wait for daemon.json to appear (up to 5s)
            let timeout = std::time::Duration::from_secs(5);
            let start = std::time::Instant::now();
            let port = loop {
                if start.elapsed() > timeout {
                    anyhow::bail!(
                        "Timeout: sandbox daemon did not start within {}s.",
                        timeout.as_secs()
                    );
                }
                if let Some(p) = sandbox_core::daemon::find_running_daemon() {
                    break p;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            };
            println!("Sandbox daemon started on port {port}");
            port
        }
    };

    // Determine mode: "app" if command ends with .app, else "cli"
    let mode = if command.to_lowercase().ends_with(".app") {
        "app"
    } else {
        "cli"
    };

    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    println!("Creating sandbox: mode={mode}, command={full_cmd}");

    let result = client::daemon_create_sandbox(mode, Some(command), args, None, None).await?;

    println!(
        "Sandbox created: id={}, pty_pid={:?}, window_id={:?}",
        result.sandbox_id, result.pty_pid, result.window_id
    );
    println!("Daemon port: {port}");

    // Spawn Electron — if already running, requestSingleInstanceLock triggers
    // second-instance event which syncs sandboxes and creates tabs.
    if let Ok(electron_bin) = find_electron_binary() {
        tracing::info!("[start] spawning Electron: {}", electron_bin.display());
        let _child = Command::new(&electron_bin)
            .spawn()
            .context("Failed to launch Electron app")?;
        tracing::info!("[start] Electron launched");
    } else {
        tracing::warn!("[start] Electron app not found, running in headless daemon mode");
    }

    Ok(())
}

/// List all sandboxes via the daemon API.
async fn cmd_list_daemon() -> anyhow::Result<()> {
    let sandboxes = client::daemon_list_sandboxes().await?;

    if sandboxes.is_empty() {
        println!("No sandbox instances found.");
        println!("Start one with: sandbox start  (opens zsh by default)");
        println!("Or: sandbox start <command>  (e.g., sandbox start claude)");
        return Ok(());
    }

    println!(
        "{:<12}  {:<20}  {:<10}  {:<10}  {:<8}  {:<8}  CREATED",
        "ID", "KIND", "STATUS", "PID", "WINDOW", "PORT"
    );
    println!("{}", "-".repeat(100));

    for sb in &sandboxes {
        let kind = match &sb.kind {
            sandbox_core::instance::InstanceKind::Cli { command, .. } => {
                format!("CLI({})", command)
            }
            sandbox_core::instance::InstanceKind::App { path } => {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("APP({})", name)
            }
        };
        let kind_display = if kind.len() > 20 {
            format!("{}...", &kind[..17])
        } else {
            kind
        };
        let status = match &sb.status {
            sandbox_core::instance::InstanceStatus::Starting => "Starting",
            sandbox_core::instance::InstanceStatus::Running => "Running",
            sandbox_core::instance::InstanceStatus::Stopped => "Stopped",
            sandbox_core::instance::InstanceStatus::Error(e) => e,
        };
        let pid_str = sb
            .pty_pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".into());
        let wid_str = sb
            .window_id
            .map(|w| w.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<12}  {:<20}  {:<10}  {:<10}  {:<8}  {:<8}",
            sb.id, kind_display, status, pid_str, wid_str, sb.port
        );
    }
    println!("\nTotal: {} instance(s)", sandboxes.len());
    Ok(())
}

/// Close a sandbox via the daemon API.
async fn cmd_close_daemon(id: &str) -> anyhow::Result<()> {
    println!("Closing sandbox {id}...");
    client::daemon_close(id).await?;
    println!("Sandbox {id} closed.");
    Ok(())
}

/// Type text in a sandbox via the daemon API.
async fn cmd_type_daemon(text: &str, id: &str, pty: bool) -> anyhow::Result<()> {
    tracing::info!(
        "[cli] type: text_len={}, id={}, pty={}",
        text.len(),
        id,
        pty
    );
    if pty {
        client::daemon_pty_write(id, text).await?;
        println!("Typed (PTY): {:?} -> sandbox {}", text, id);
    } else {
        client::daemon_type(id, text).await?;
        println!("Typed: {:?} -> sandbox {}", text, id);
    }
    Ok(())
}

/// Press a key in a sandbox via the daemon API.
async fn cmd_key_daemon(
    key: &str,
    id: &str,
    modifiers: &[String],
    pty: bool,
) -> anyhow::Result<()> {
    tracing::info!(
        "[cli] key: key={}, modifiers={:?}, id={}, pty={}",
        key,
        modifiers,
        id,
        pty
    );
    if pty {
        let data = client::key_to_pty_bytes_with_modifiers(key, modifiers);
        if data.is_empty() {
            let plain = client::key_to_pty_bytes(key);
            if plain.is_empty() {
                anyhow::bail!(
                    "Key '{}' with modifiers {:?} cannot be mapped to PTY bytes. Use CGEvent mode (remove --pty).",
                    key, modifiers
                );
            }
            client::daemon_pty_write(id, &plain).await?;
        } else {
            client::daemon_pty_write(id, &data).await?;
        }
        println!(
            "Pressed (PTY): {}{} -> sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    } else {
        client::daemon_key(id, key, modifiers).await?;
        println!(
            "Pressed: {}{} -> sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    }
    Ok(())
}

/// Click in a sandbox via the daemon API.
async fn cmd_click_daemon(x: f64, y: f64, id: &str, button: &str) -> anyhow::Result<()> {
    client::daemon_click(id, x, y, button).await?;
    println!("Clicked ({}, {}) [{}] -> sandbox {}", x, y, button, id);
    Ok(())
}

/// Take a screenshot via the daemon API.
async fn cmd_screenshot_daemon(output: &std::path::Path, id: Option<&str>) -> anyhow::Result<()> {
    let sandbox_id = id.ok_or_else(|| {
        anyhow::anyhow!(
            "--id is required for screenshots. Use: sandbox screenshot --id <sandbox-id>"
        )
    })?;

    let png = client::daemon_screenshot(sandbox_id).await?;
    std::fs::write(output, &png)
        .with_context(|| format!("Failed to write screenshot to {:?}", output))?;
    println!("Screenshot saved to {:?} ({} bytes)", output, png.len());
    Ok(())
}

/// Shutdown the daemon via HTTP.
async fn cmd_shutdown_daemon() -> anyhow::Result<()> {
    client::daemon_shutdown().await?;
    println!("Sandbox daemon shut down.");
    Ok(())
}

/// Inspect a sandbox via the daemon API.
async fn cmd_inspect_daemon(id: &str) -> anyhow::Result<()> {
    let sb = client::daemon_inspect(id).await?;

    println!("Instance:");
    println!("  ID:       {}", sb.id);
    println!("  Port:     {}", sb.port);
    println!("  PTY PID:  {:?}", sb.pty_pid);
    println!("  Window:   {:?}", sb.window_id);
    println!("  Status:   {:?}", sb.status);

    let kind = match &sb.kind {
        sandbox_core::instance::InstanceKind::Cli { command, args } => {
            if args.is_empty() {
                format!("CLI({})", command)
            } else {
                format!("CLI({} {})", command, args.join(" "))
            }
        }
        sandbox_core::instance::InstanceKind::App { path } => {
            let name = std::path::Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            format!("APP({})", name)
        }
    };
    println!("  Kind:     {}", kind);

    Ok(())
}

/// List processes in a sandbox via the daemon API.
async fn cmd_processes_daemon(id: &str) -> anyhow::Result<()> {
    let processes = client::daemon_processes(id).await?;

    if processes.is_empty() {
        println!("No processes found in sandbox {}.", id);
        return Ok(());
    }

    println!("{:<10}  {:<20}  {:<10}  PATH", "PID", "NAME", "RUNNING");
    println!("{}", "-".repeat(70));
    for p in &processes {
        let running = if p.is_running { "yes" } else { "no" };
        let path = p.path.as_deref().unwrap_or("-");
        println!("{:<10}  {:<20}  {:<10}  {}", p.pid, p.name, running, path);
    }
    println!("\nTotal: {} process(es)", processes.len());
    Ok(())
}

// ── Legacy command implementations (kept for reference) ──────

/// List all registered sandbox instances (legacy, reads from filesystem registry).
#[allow(dead_code)]
fn cmd_list() -> anyhow::Result<()> {
    let registry = InstanceRegistry::default();
    let instances = registry.list()?;

    if instances.is_empty() {
        println!("No sandbox instances found.");
        println!("Start one with: sandbox start  (opens zsh by default)");
        println!("Or: sandbox start <command>  (e.g., sandbox start claude)");
        return Ok(());
    }

    println!(
        "{:<12}  {:<20}  {:<6}  {:<10}  {:<6}  CREATED",
        "ID", "TITLE", "KIND", "STATUS", "PORT"
    );
    println!("{}", "-".repeat(90));

    for inst in &instances {
        let kind = match &inst.kind {
            sandbox_core::instance::InstanceKind::Cli { command, .. } => {
                format!("CLI({})", command)
            }
            sandbox_core::instance::InstanceKind::App { path } => {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("APP({})", name)
            }
        };
        let status = match &inst.status {
            sandbox_core::instance::InstanceStatus::Starting => "Starting",
            sandbox_core::instance::InstanceStatus::Running => "Running",
            sandbox_core::instance::InstanceStatus::Stopped => "Stopped",
            sandbox_core::instance::InstanceStatus::Error(e) => return_ref(e),
        };
        let title_display = if inst.title.len() > 20 {
            format!("{}...", &inst.title[..17])
        } else {
            inst.title.clone()
        };
        println!(
            "{:<12}  {:<20}  {:<6}  {:<10}  {:<6}  {}",
            inst.id, title_display, kind, status, inst.port, inst.created_at
        );
    }
    println!("\nTotal: {} instance(s)", instances.len());
    Ok(())
}

fn return_ref(s: &str) -> &str {
    s
}

/// Show details of a specific sandbox instance (legacy, reads per-instance HTTP API).
#[allow(dead_code)]
async fn cmd_inspect(id: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;

    // Show registry info
    let registry = InstanceRegistry::default();
    let instance = registry.get(id)?;
    println!("Instance:");
    println!("  ID:       {}", instance.id);
    println!("  Port:     {}", instance.port);
    println!("  PID:      {}", instance.pid);
    println!("  Title:    {}", instance.title);
    println!("  Status:   {:?}", instance.status);
    println!("  Window:   {:?}", instance.window_id);
    println!("  Created:  {}", instance.created_at);

    // Try to get live info from the HTTP API
    match client.health().await {
        Ok(health) => {
            println!("\nHTTP API:");
            println!("  Status:   {}", health.status);
            println!("  Version:  {}", health.version);
            println!("  Uptime:   {}s", health.uptime_secs);
        }
        Err(e) => {
            println!("\nHTTP API: unreachable ({})", e);
        }
    }

    if let Ok(info) = client.sandbox_info().await {
        println!("  Window:   {:?}", info.window_id);
    }

    Ok(())
}

/// Close a sandbox instance via HTTP API (legacy).
#[allow(dead_code)]
async fn cmd_close(id: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;

    println!("Closing sandbox {}...", id);
    client.shutdown().await?;

    // Clean up registry
    let registry = InstanceRegistry::default();
    let _ = registry.unregister(id);

    println!("Sandbox {} closed.", id);
    Ok(())
}

/// Type text into a sandbox (legacy).
#[allow(dead_code)]
async fn cmd_type(text: &str, id: &str, pty: bool) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    tracing::info!(
        "[cli] type: text_len={}, id={}, pty={}",
        text.len(),
        id,
        pty
    );

    if pty {
        client.pty_write_auto(text).await?;
        println!("Typed (PTY): {:?} → sandbox {}", text, id);
    } else {
        tracing::warn!("[cli] type: using CGEvent path (not PTY). This targets the Tauri process, not the CLI child process. Consider using --pty for CLI sandboxes.");
        client.type_text(text).await?;
        println!("Typed (CGEvent): {:?} → sandbox {}", text, id);
    }
    Ok(())
}

/// Press a key in a sandbox (legacy).
#[allow(dead_code)]
async fn cmd_key(key: &str, id: &str, modifiers: &[String], pty: bool) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    tracing::info!(
        "[cli] key: key={}, modifiers={:?}, id={}, pty={}",
        key,
        modifiers,
        id,
        pty
    );

    if pty {
        let data = client::key_to_pty_bytes_with_modifiers(key, modifiers);
        if data.is_empty() {
            // Try plain key mapping as fallback
            let plain = client::key_to_pty_bytes(key);
            if plain.is_empty() {
                anyhow::bail!(
                    "Key '{}' with modifiers {:?} cannot be mapped to PTY bytes. Use CGEvent mode (remove --pty).",
                    key, modifiers
                );
            }
            client.pty_write_auto(&plain).await?;
        } else {
            client.pty_write_auto(&data).await?;
        }
        println!(
            "Pressed (PTY): {} {} → sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    } else {
        tracing::warn!("[cli] key: using CGEvent path (not PTY). This targets the Tauri process, not the CLI child process. Consider using --pty for CLI sandboxes.");
        client.press_key(key, modifiers).await?;
        println!(
            "Pressed (CGEvent): {} {} → sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    }
    Ok(())
}

/// Click at coordinates in a sandbox (legacy).
#[allow(dead_code)]
async fn cmd_click(x: f64, y: f64, id: &str, button: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    client.click(x, y, button).await?;
    println!("Clicked ({}, {}) [{}] → sandbox {}", x, y, button, id);
    Ok(())
}

/// Take a screenshot (legacy).
#[allow(dead_code)]
async fn cmd_screenshot(
    output: &std::path::Path,
    id: Option<&str>,
    window_id: Option<u32>,
) -> anyhow::Result<()> {
    if let Some(sandbox_id) = id {
        // Instance-scoped screenshot via HTTP API
        let client = client::SandboxClient::from_instance_id(sandbox_id)?;
        let png = client.screenshot().await?;
        std::fs::write(output, &png)
            .with_context(|| format!("Failed to write screenshot to {:?}", output))?;
        println!("Screenshot saved to {:?} ({} bytes)", output, png.len());
    } else {
        // Legacy: auto-discover window
        let wid = if let Some(id) = window_id {
            id
        } else {
            discover_sandbox_window()?
        };
        let png = ScreenCapture::capture_window(wid).with_context(|| {
            format!("Failed to capture window {wid}. Is Screen Recording permission granted?")
        })?;
        std::fs::write(output, &png)
            .with_context(|| format!("Failed to write screenshot to {:?}", output))?;
        println!("Screenshot saved to {:?} ({} bytes)", output, png.len());
    }
    Ok(())
}

/// List all visible windows via ScreenCaptureKit.
fn cmd_windows() -> anyhow::Result<()> {
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    if windows.is_empty() {
        println!("No windows found.");
        return Ok(());
    }

    println!("{:<12}  Title", "Window ID");
    println!("{}", "-".repeat(80));
    for (id, title) in &windows {
        let title_display = if title.len() > 64 {
            let mut end = 61;
            while end > 0 && !title.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &title[..end])
        } else {
            title.clone()
        };
        println!("{:<12}  {}", id, title_display);
    }
    println!("Total: {} windows", windows.len());
    Ok(())
}

/// List processes running inside a sandbox instance (legacy).
#[allow(dead_code)]
async fn cmd_processes(id: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    let processes = client.list_processes().await?;

    if processes.is_empty() {
        println!("No processes found in sandbox {}.", id);
        return Ok(());
    }

    println!("{:<10}  {:<20}  {:<10}  PATH", "PID", "NAME", "RUNNING");
    println!("{}", "-".repeat(70));
    for p in &processes {
        let running = if p.is_running { "yes" } else { "no" };
        let path = p.path.as_deref().unwrap_or("-");
        println!("{:<10}  {:<20}  {:<10}  {}", p.pid, p.name, running, path);
    }
    println!("\nTotal: {} process(es)", processes.len());
    Ok(())
}

/// Legacy shutdown command (osascript-based).
#[allow(dead_code)]
fn cmd_shutdown() -> anyhow::Result<()> {
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    let tauri_window = windows
        .iter()
        .find(|(_, title)| title.starts_with("System Test Sandbox"));

    if let Some((id, title)) = tauri_window {
        println!("Closing sandbox window: {} (ID: {})", title, id);
        let script = r#"tell application "System Events"
    set procList to every process whose name is "system-test-sandbox"
    repeat with proc in procList
        set winList to every window of proc
        repeat with win in winList
            close win
        end repeat
    end repeat
end tell"#
            .to_string();
        let _ = Command::new("osascript").arg("-e").arg(&script).output();
    } else {
        let script = r#"tell application "Terminal"
    close first window
end tell"#;
        let _ = Command::new("osascript").arg("-e").arg(script).output();
    }

    println!("Sandbox shutdown complete.");
    Ok(())
}

/// Show log file paths.
fn cmd_logs(id: Option<&str>) -> anyhow::Result<()> {
    let base = sandbox_core::logging::log_base_dir();
    println!("Log base: {}\n", base.display());

    if let Some(sandbox_id) = id {
        // Show logs for a specific sandbox
        let path = sandbox_core::logging::sandbox_log_path(sandbox_id);
        let server = sandbox_core::logging::server_log_path();
        println!("  Sandbox [{sandbox_id}]:");
        println!("    {}", path.display());
        println!("  Server (shared):");
        println!("    {}", server.display());
    } else {
        // Show all known sandboxes and their log paths
        let registry = InstanceRegistry::default();
        let instances = registry.list()?;

        if instances.is_empty() {
            println!("No sandbox instances found.");
        } else {
            for inst in &instances {
                let path = sandbox_core::logging::sandbox_log_path(&inst.id);
                println!("  [{}] {} → {}", inst.id, inst.title, path.display());
            }
        }

        // Show shared logs
        let server = sandbox_core::logging::server_log_path();
        let cli = sandbox_core::logging::cli_log_path();
        println!("\n  Shared logs:");
        println!("    Server: {}", server.display());
        println!("    CLI:    {}", cli.display());
    }

    // List existing log entries
    if base.exists() {
        println!("\n  Existing logs:");
        let mut entries: Vec<String> = std::fs::read_dir(&base)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if e.path().is_dir() {
                    format!("{}/", name)
                } else {
                    name
                }
            })
            .collect();
        entries.sort();
        for d in &entries {
            println!("    {}", d);
        }
    }

    Ok(())
}

// ── UI Inspection Commands ──────────────────────────────

async fn cmd_ui_inspect(id: &str) -> anyhow::Result<()> {
    let tree = client::daemon_ui_inspect(id).await?;
    println!("{}", serde_json::to_string_pretty(&tree)?);
    Ok(())
}

async fn cmd_ui_find(id: &str, role: &str, title: Option<&str>) -> anyhow::Result<()> {
    let elements = client::daemon_ui_find(id, role, title).await?;
    println!("{}", serde_json::to_string_pretty(&elements)?);
    Ok(())
}

async fn cmd_ui_value(id: &str, element_id: &str) -> anyhow::Result<()> {
    let value = client::daemon_ui_value(id, element_id).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

// ── Record / Playback / Diff Commands ───────────────────

fn cmd_record(id: &str, output: &std::path::Path) -> anyhow::Result<()> {
    println!("Recording sandbox {id} to {}...", output.display());
    println!("Use 'sandbox type', 'sandbox key', 'sandbox click' commands while recording.");
    println!("Recording is integrated into the daemon — use HTTP API for now.");
    Ok(())
}

fn cmd_playback(id: &str, input: &std::path::Path, speed: f64) -> anyhow::Result<()> {
    println!(
        "Playing back {} on sandbox {id} at {speed}x speed...",
        input.display()
    );
    let actions = sandbox_core::player::Player::load_actions(input)?;
    println!("Loaded {} actions.", actions.len());
    for action in &actions {
        println!("  {}ms: {:?}", action.offset_ms, action.action);
    }
    Ok(())
}

fn cmd_diff(
    a: &std::path::Path,
    b: &std::path::Path,
    threshold: u8,
    output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let img_a = std::fs::read(a).with_context(|| format!("Failed to read {}", a.display()))?;
    let img_b = std::fs::read(b).with_context(|| format!("Failed to read {}", b.display()))?;
    let result = sandbox_core::diff::diff_images(&img_a, &img_b, threshold)?;
    println!("Total pixels: {}", result.total_pixels);
    println!(
        "Different: {} ({:.2}%)",
        result.different_pixels, result.diff_percentage
    );
    if let (Some(out_path), Some(img)) = (output, &result.diff_image) {
        std::fs::write(out_path, img)?;
        println!("Diff image saved to: {}", out_path.display());
    }
    Ok(())
}

// ── MCP stdio server ────────────────────────────────────

fn mcp_tools() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "list_sandboxes",
                "description": "List all active sandbox instances",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "start_sandbox",
                "description": "Start a new sandbox with a CLI command",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command to run (e.g., 'zsh', 'claude')" },
                        "args": { "type": "array", "items": { "type": "string" }, "description": "Command arguments" }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "close_sandbox",
                "description": "Close a sandbox by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            },
            {
                "name": "screenshot_sandbox",
                "description": "Take a screenshot of a sandbox (returns base64 PNG)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            },
            {
                "name": "type_text",
                "description": "Type text into a sandbox PTY",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" },
                        "text": { "type": "string" }
                    },
                    "required": ["sandbox_id", "text"]
                }
            },
            {
                "name": "press_key",
                "description": "Press a key in a sandbox",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" },
                        "key": { "type": "string", "description": "Key name (Return, Tab, Escape, etc.)" },
                        "modifiers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["sandbox_id", "key"]
                }
            },
            {
                "name": "inspect_ui",
                "description": "Inspect the UI tree of a sandbox window",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            }
        ]
    })
}

async fn run_mcp_server() -> anyhow::Result<()> {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let msg: serde_json::Value = serde_json::from_str(&line)?;
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(serde_json::json!({}));

        let result = match method {
            "initialize" => serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "sandbox-mcp", "version": "0.1.0" }
            }),
            "tools/list" => mcp_tools(),
            "tools/call" => {
                let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                handle_mcp_tool(tool_name, &args).await
            }
            _ => {
                serde_json::json!({ "error": { "code": -32601, "message": "Method not found" } })
            }
        };

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }
    Ok(())
}

async fn handle_mcp_tool(name: &str, args: &serde_json::Value) -> serde_json::Value {
    let result: anyhow::Result<serde_json::Value> = async {
        match name {
            "list_sandboxes" => {
                let list = client::daemon_list_sandboxes().await?;
                Ok(serde_json::to_value(list)?)
            }
            "start_sandbox" => {
                let cmd = args["command"].as_str().unwrap_or("zsh");
                let cmd_args: Vec<String> = args["args"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let result =
                    client::daemon_create_sandbox("cli", Some(cmd), &cmd_args, None, None).await?;
                Ok(serde_json::to_value(result)?)
            }
            "close_sandbox" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                client::daemon_close(id).await?;
                Ok(serde_json::json!({ "closed": id }))
            }
            "screenshot_sandbox" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let png = client::daemon_screenshot(id).await?;
                let b64 = base64_encode(&png);
                Ok(serde_json::json!({ "sandbox_id": id, "image_base64": b64 }))
            }
            "type_text" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let text = args["text"].as_str().unwrap_or("");
                client::daemon_pty_write(id, text).await?;
                Ok(serde_json::json!({ "typed": text }))
            }
            "press_key" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let key = args["key"].as_str().unwrap_or("Return");
                let mods: Vec<String> = args["modifiers"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                client::daemon_key(id, key, &mods).await?;
                Ok(serde_json::json!({ "pressed": key }))
            }
            "inspect_ui" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let tree = client::daemon_ui_inspect(id).await?;
                Ok(serde_json::to_value(tree)?)
            }
            _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {name}") })),
        }
    }
    .await;

    match result {
        Ok(value) => serde_json::json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }]
        }),
        Err(e) => serde_json::json!({
            "content": [{ "type": "text", "text": format!("Error: {e}") }],
            "isError": true
        }),
    }
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

// ── Helpers ─────────────────────────────────────────────

fn find_tauri_bundle() -> anyhow::Result<PathBuf> {
    let app_name = "System Test Sandbox.app";
    let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
    let exe_dir = exe_path.parent().context("No parent dir for exe")?;

    let path1 = exe_dir.join(app_name);
    if path1.exists() {
        return Ok(path1);
    }

    let path2 = exe_dir.join("bundle/macos").join(app_name);
    if path2.exists() {
        return Ok(path2);
    }

    if let Some(project_root) = exe_dir.parent() {
        let path3 = project_root
            .join("target/release/bundle/macos")
            .join(app_name);
        if path3.exists() {
            return Ok(path3);
        }
    }

    anyhow::bail!(
        "Tauri sandbox app not found.\n\
         Searched:\n  {}\n  {}\n  {}\n\
         Build it first with: cargo tauri build",
        path1.display(),
        path2.display(),
        exe_dir
            .join("../target/release/bundle/macos")
            .join(app_name)
            .display()
    )
}

/// Locate the `sandbox-daemon` binary next to the current executable.
fn find_daemon_binary() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
    let exe_dir = exe_path.parent().context("No parent dir for exe")?;

    let daemon_name = "sandbox-daemon";

    // Same directory as current exe
    let path1 = exe_dir.join(daemon_name);
    if path1.exists() {
        return Ok(path1);
    }

    // target/release/ or target/debug/ (relative to project root)
    for dir_name in &["release", "debug"] {
        let path = exe_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("target").join(dir_name).join(daemon_name));
        if let Some(p) = path {
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // Check target/debug/ directly from the current exe if we're in the project
    let cwd = std::env::current_dir().unwrap_or_default();
    for dir_name in &["release", "debug"] {
        let path = cwd.join("target").join(dir_name).join(daemon_name);
        if path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!(
        "sandbox-daemon binary not found.\n\
         Searched:\n  {}\n\
         Build it first with: cargo build -p sandbox-daemon",
        path1.display()
    )
}

/// Locate the Electron app binary next to the current executable.
fn find_electron_binary() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
    let exe_dir = exe_path.parent().context("No parent dir for exe")?;

    // Check for Electron binary in release directory
    let electron_name = "System Test Sandbox";
    let app_bundle = exe_dir.join(format!("{electron_name}.app"));
    if app_bundle.exists() {
        return Ok(app_bundle.join("Contents/MacOS/System Test Sandbox"));
    }

    // Dev mode: check dist/electron
    let cwd = std::env::current_dir().unwrap_or_default();
    let dev_bundle = cwd.join("dist/electron/mac-arm64/System Test Sandbox.app");
    if dev_bundle.exists() {
        return Ok(dev_bundle.join("Contents/MacOS/system-test-sandbox"));
    }

    // Also check x64
    let dev_bundle_x64 = cwd.join("dist/electron/mac/System Test Sandbox.app");
    if dev_bundle_x64.exists() {
        return Ok(dev_bundle_x64.join("Contents/MacOS/system-test-sandbox"));
    }

    anyhow::bail!(
        "Electron app not found. Build it first: cd electron-app && pnpm build && pnpm pack"
    )
}

/// Check if Electron is already running by reading ~/.sandbox/electron.json
#[allow(dead_code)]
fn find_running_electron() -> bool {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(home)
        .join(".sandbox")
        .join("electron.json");
    if !path.exists() {
        return false;
    }
    let json = match std::fs::read_to_string(&path) {
        Ok(j) => j,
        Err(_) => return false,
    };
    let info: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let pid = match info["pid"].as_u64() {
        Some(p) => p as i32,
        None => return false,
    };
    let alive = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if alive {
        return true;
    }
    let _ = std::fs::remove_file(&path);
    false
}

fn discover_sandbox_window() -> anyhow::Result<u32> {
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    for (id, title) in &windows {
        if title.starts_with("System Test Sandbox") {
            return Ok(*id);
        }
    }

    for (id, title) in &windows {
        if is_terminal_title(title) {
            return Ok(*id);
        }
    }

    for (id, title) in &windows {
        if title.to_lowercase().contains("claude") {
            return Ok(*id);
        }
    }

    anyhow::bail!(
        "No sandbox window found automatically.\n\
         Use 'sandbox windows' to list all windows, then 'sandbox screenshot --window-id <ID>'."
    )
}

fn is_terminal_title(title: &str) -> bool {
    let sep = " — ";
    let last_sep = match title.rfind(sep) {
        Some(pos) => pos + sep.len(),
        None => return false,
    };

    let suffix = &title[last_sep..];
    let parts: Vec<&str> = suffix.split('×').collect();
    if parts.len() != 2 {
        return false;
    }
    parts[0].trim().parse::<u32>().is_ok() && parts[1].trim().parse::<u32>().is_ok()
}
