use clap::{Parser, Subcommand};
use sandbox_core::automation::cg_event::{InputSimulator, MouseButton};
use sandbox_core::capture::ScreenCapture;
use sandbox_core::instance::{
    generate_instance_id, InstanceKind, InstanceRegistry, SandboxInstance,
};
use sandbox_core::process::ProcessManager;
use sandbox_core::recorder::ActionRecorder;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

mod client;
mod mcp_server;

#[derive(Parser)]
#[command(name = "sandbox-cli", about = "macOS Desktop Automation Sandbox CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a new sandbox instance
    Start {
        /// Run a CLI command inside the sandbox
        #[arg(long, group = "mode")]
        cli: Option<String>,

        /// Launch a macOS .app inside the sandbox
        #[arg(long, group = "mode")]
        app: Option<String>,

        /// Additional arguments for the CLI command
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Sandbox window width
        #[arg(long, default_value = "1280")]
        width: u32,

        /// Sandbox window height
        #[arg(long, default_value = "800")]
        height: u32,
    },

    /// List all active sandbox instances
    List,

    /// Show details of a specific sandbox
    Inspect {
        /// Sandbox instance ID
        id: String,
    },

    /// Close a sandbox instance
    Close {
        /// Sandbox instance ID
        id: String,
    },

    /// Take a screenshot
    Screenshot {
        /// Sandbox instance ID
        #[arg(short, long)]
        id: Option<String>,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Window ID to capture
        #[arg(long)]
        window_id: Option<u32>,
    },

    /// List windows
    Windows {
        #[arg(short, long)]
        id: Option<String>,
    },

    /// List processes
    Processes {
        #[arg(short, long)]
        id: Option<String>,
    },

    /// Spawn an app
    SpawnApp {
        #[arg(short, long)]
        id: Option<String>,

        /// Path to the .app bundle
        path: String,
    },

    /// Spawn a CLI
    SpawnCli {
        #[arg(short, long)]
        id: Option<String>,

        /// Command to run
        command: String,

        /// Additional arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Simulate mouse click
    Click {
        #[arg(short, long)]
        id: Option<String>,

        x: f64,
        y: f64,

        #[arg(short, long, default_value = "left")]
        button: String,
    },

    /// Simulate typing text
    Type {
        #[arg(short, long)]
        id: Option<String>,

        /// Text to type
        text: String,
    },

    /// Simulate key press
    Key {
        #[arg(short, long)]
        id: Option<String>,

        /// Key name (e.g., Return, Tab, Space)
        key: String,

        /// Modifier keys (e.g., cmd, shift, alt)
        #[arg(short, long)]
        modifiers: Vec<String>,
    },

    /// Kill a process by PID
    Kill {
        #[arg(short, long)]
        id: Option<String>,

        /// Process ID
        pid: u32,
    },

    /// Start standalone HTTP + MCP server (legacy mode)
    Serve {
        #[arg(long, default_value = "5801")]
        port: u16,
    },

    /// Start MCP server via stdio
    McpServe,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        // ── Multi-instance commands ──────────────────────
        Commands::Start {
            cli,
            app,
            args,
            width,
            height,
        } => {
            let kind = match (cli, app) {
                (Some(cmd), None) => InstanceKind::Cli { command: cmd, args },
                (None, Some(app_path)) => InstanceKind::App { path: app_path },
                (Some(_), Some(_)) => anyhow::bail!("Cannot specify both --cli and --app"),
                (None, None) => anyhow::bail!("Must specify --cli or --app"),
            };

            let sandbox_id = generate_instance_id();

            // Find a free port
            let port = find_free_port()?;

            let title = match &kind {
                InstanceKind::Cli { command, .. } => command.clone(),
                InstanceKind::App { path } => std::path::Path::new(path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            };

            println!("Starting sandbox: {sandbox_id} ({title}) on port {port}");

            // Try to launch the Tauri app
            let launched = launch_tauri_app(&sandbox_id, port, &kind, width, height);

            if !launched {
                // Fallback: start standalone HTTP server
                tracing::info!("Tauri app not found, starting standalone HTTP server");
                let state = Arc::new(Mutex::new(sandbox_core::server::AppState {
                    sandbox_id: Some(sandbox_id.clone()),
                    start_time: Instant::now(),
                    window_id: None,
                    target_pid: None,
                    recorder: ActionRecorder::new(),
                }));

                // Auto-spawn the target CLI in background
                if let InstanceKind::Cli { command, args } = &kind {
                    let cmd = command.clone();
                    let cmd_args = args.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        match ProcessManager::spawn_cli(&cmd, &cmd_args) {
                            Ok(info) => {
                                tracing::info!("Auto-spawned CLI: {} (pid={})", cmd, info.pid)
                            }
                            Err(e) => tracing::error!("Failed to auto-spawn CLI: {e}"),
                        }
                    });
                }

                let registry = InstanceRegistry::default();
                let instance = SandboxInstance::new(
                    sandbox_id.clone(),
                    port,
                    std::process::id(),
                    kind.clone(),
                );
                registry.register(&instance)?;

                let app = sandbox_core::server::build_router(state);
                let addr = format!("127.0.0.1:{port}");
                let listener = tokio::net::TcpListener::bind(&addr).await?;

                println!("Sandbox started: {sandbox_id}");
                println!("  HTTP API: http://{addr}");
                println!("  Use 'sandbox-cli list' to see all instances");
                println!("  Use 'sandbox-cli close {sandbox_id}' to stop");

                axum::serve(listener, app).await?;
            } else {
                // Wait for Tauri health check
                let client = client::SandboxClient::new(port);
                let mut ready = false;
                for attempt in 0..30 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if client.health().await.unwrap_or(false) {
                        ready = true;
                        break;
                    }
                    tracing::debug!("Waiting for sandbox to start... attempt {attempt}");
                }

                if ready {
                    let registry = InstanceRegistry::default();
                    let instance =
                        SandboxInstance::new(sandbox_id.clone(), port, std::process::id(), kind);
                    registry.register(&instance)?;
                    println!("Sandbox started: {sandbox_id}");
                } else {
                    anyhow::bail!(
                        "Sandbox failed to start within 6 seconds. Check the Tauri app logs."
                    );
                }
            }
        }

        Commands::List => {
            let registry = InstanceRegistry::default();
            let instances = registry.list()?;

            if instances.is_empty() {
                println!("No active sandboxes.");
            } else {
                println!(
                    "{:<10} {:<25} {:<6} {:<10} {:<8} {:<20}",
                    "ID", "TITLE", "KIND", "STATUS", "PORT", "CREATED"
                );
                for inst in &instances {
                    let kind_str = match &inst.kind {
                        InstanceKind::Cli { .. } => "CLI",
                        InstanceKind::App { .. } => "APP",
                    };
                    let status_str = match &inst.status {
                        sandbox_core::instance::InstanceStatus::Starting => "Starting",
                        sandbox_core::instance::InstanceStatus::Running => "Running",
                        sandbox_core::instance::InstanceStatus::Stopped => "Stopped",
                        sandbox_core::instance::InstanceStatus::Error(_) => "Error",
                    };
                    println!(
                        "{:<10} {:<25} {:<6} {:<10} {:<8} {:<20}",
                        inst.id, inst.title, kind_str, status_str, inst.port, inst.created_at
                    );
                }
                println!("\nTotal: {} instance(s)", instances.len());
            }
        }

        Commands::Inspect { id } => {
            let registry = InstanceRegistry::default();
            let instance = registry.get(&id)?;
            let json = serde_json::to_string_pretty(&instance)?;
            println!("{json}");
        }

        Commands::Close { id } => {
            let registry = InstanceRegistry::default();
            let instance = registry.get(&id)?;

            // Try to send shutdown to the sandbox
            let cli = client::SandboxClient::new(instance.port);
            match cli.shutdown().await {
                Ok(()) => println!("Shutdown signal sent to sandbox {id}"),
                Err(e) => {
                    tracing::warn!("Failed to send shutdown (sandbox may be gone): {e}");
                }
            }

            registry.unregister(&id)?;
            println!("Sandbox {id} closed and unregistered.");
        }

        // ── Instance-scoped or local operations ──────────
        Commands::Screenshot {
            id,
            output,
            window_id,
        } => {
            let path = output.unwrap_or_else(|| PathBuf::from("sandbox_screenshot.png"));

            let png_data = if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                cli.screenshot().await?
            } else {
                let data = ScreenCapture::capture_sandbox_by_id(window_id)?;
                if data.is_empty() {
                    anyhow::bail!("Screenshot returned empty data — no sandbox window available");
                }
                if !data.starts_with(b"\x89PNG") {
                    anyhow::bail!(
                        "Screenshot returned non-PNG data ({} bytes). \
                         The sandbox window may not be running.",
                        data.len()
                    );
                }
                data
            };

            std::fs::write(&path, &png_data)?;
            println!("Screenshot saved to {path:?} ({} bytes)", png_data.len());
        }

        Commands::Windows { id } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                let windows = cli.windows().await?;
                for (wid, title) in &windows {
                    println!("  Window ID={wid}: {title}");
                }
                println!("Total: {} windows", windows.len());
            } else {
                let windows = ScreenCapture::list_windows()?;
                for (wid, title) in &windows {
                    println!("  Window ID={wid}: {title}");
                }
                println!("Total: {} windows", windows.len());
            }
        }

        Commands::Processes { id } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                let processes = cli.processes().await?;
                for p in &processes {
                    println!("  {}", serde_json::to_string(p).unwrap_or_default());
                }
                println!("Total: {} processes", processes.len());
            } else {
                let processes = ProcessManager::list_processes()?;
                for p in &processes {
                    println!("  PID={}: {} (running={})", p.pid, p.name, p.is_running);
                }
                println!("Total: {} processes", processes.len());
            }
        }

        Commands::SpawnApp { id, path } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                let info = cli.spawn_app(&path).await?;
                println!("App spawned: {info}");
            } else {
                let info = ProcessManager::spawn_app(&path)?;
                println!("App spawned: PID={}, name={}", info.pid, info.name);
            }
        }

        Commands::SpawnCli { id, command, args } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                let args_refs: Vec<String> = args.iter().map(|s| s.to_string()).collect();
                let info = cli.spawn_cli(&command, &args_refs).await?;
                println!("CLI spawned: {info}");
            } else {
                let args_refs: Vec<String> = args.iter().map(|s| s.to_string()).collect();
                let info = ProcessManager::spawn_cli(&command, &args_refs)?;
                println!("CLI spawned: PID={}, name={}", info.pid, info.name);
            }
        }

        Commands::Click { id, x, y, button } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                cli.click(x, y, &button).await?;
                println!("Clicked at ({x}, {y}) in sandbox {sandbox_id}");
            } else {
                let btn = match button.to_lowercase().as_str() {
                    "left" => MouseButton::Left,
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    other => anyhow::bail!("Unknown button: {other}. Use left, right, or middle."),
                };
                InputSimulator::click(x, y, btn, None)?;
                println!("Clicked at ({x}, {y})");
            }
        }

        Commands::Type { id, text } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                cli.type_text(&text).await?;
                println!("Typed text in sandbox {sandbox_id}");
            } else {
                InputSimulator::type_text(&text, None)?;
                println!("Typed: {text}");
            }
        }

        Commands::Key { id, key, modifiers } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                cli.press_key(&key, &modifiers).await?;
                println!("Pressed key: {key} in sandbox {sandbox_id}");
            } else {
                let mod_refs: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
                InputSimulator::press_key(&key, &mod_refs, None)?;
                println!("Pressed key: {key} {modifiers:?}");
            }
        }

        Commands::Kill { id, pid } => {
            if let Some(sandbox_id) = id {
                let registry = InstanceRegistry::default();
                let instance = registry.get(&sandbox_id)?;
                let cli = client::SandboxClient::new(instance.port);
                cli.kill_process(pid).await?;
                println!("Process {pid} killed in sandbox {sandbox_id}");
            } else {
                ProcessManager::kill_process(pid)?;
                println!("Process {pid} terminated");
            }
        }

        // ── Legacy standalone mode ───────────────────────
        Commands::Serve { port } => {
            tracing::info!("Starting sandbox server on port {port}");

            let state = Arc::new(Mutex::new(sandbox_core::server::AppState {
                sandbox_id: None,
                start_time: Instant::now(),
                window_id: None,
                target_pid: None,
                recorder: ActionRecorder::new(),
            }));

            let app = sandbox_core::server::build_router(state);
            let addr = format!("127.0.0.1:{port}");
            let listener = tokio::net::TcpListener::bind(&addr).await?;

            tracing::info!("HTTP API server listening on http://{addr}");
            println!("Sandbox HTTP API server started on http://{addr}");
            println!("  GET  http://{addr}/health");
            println!("  GET  http://{addr}/screenshot");
            println!("  POST http://{addr}/input/click");
            println!("  POST http://{addr}/cli/spawn");

            axum::serve(listener, app).await?;
        }

        Commands::McpServe => {
            tracing::info!("Starting MCP server on stdio");
            mcp_server::run_stdio_server().await?;
        }
    }

    Ok(())
}

/// Find a free TCP port on 127.0.0.1
fn find_free_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Try to launch the Tauri app with sandbox arguments.
/// Returns true if the app was found and launched.
fn launch_tauri_app(
    sandbox_id: &str,
    port: u16,
    kind: &InstanceKind,
    _width: u32,
    _height: u32,
) -> bool {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or(std::path::Path::new("."));

    // Look for the Tauri .app relative to the CLI binary
    let candidates = [
        exe_dir.join("../System Test Sandbox.app"),
        exe_dir.join("../../System Test Sandbox.app"),
    ];

    let app_path = candidates.iter().find(|p| p.exists());

    let (mode, cmd) = match kind {
        InstanceKind::Cli { command, args } => {
            let full_cmd = if args.is_empty() {
                command.clone()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            ("cli", full_cmd)
        }
        InstanceKind::App { path } => ("app", path.clone()),
    };

    if let Some(app) = app_path {
        tracing::info!("Launching Tauri app: {:?}", app);
        match std::process::Command::new("open")
            .arg("-n")
            .arg(app)
            .arg("--args")
            .arg(format!("--sandbox-id={sandbox_id}"))
            .arg(format!("--sandbox-port={port}"))
            .arg(format!("--mode={mode}"))
            .arg(format!("--cmd={cmd}"))
            .spawn()
        {
            Ok(_) => {
                tracing::info!("Tauri app launched");
                return true;
            }
            Err(e) => {
                tracing::warn!("Failed to launch Tauri app: {e}");
                return false;
            }
        }
    }

    // Also try the binary directly (for development)
    let bin_candidates = [
        exe_dir.join("system-test-sandbox"),
        exe_dir.join("../system-test-sandbox"),
    ];
    for bin_path in &bin_candidates {
        if bin_path.exists() {
            tracing::info!("Launching Tauri binary: {:?}", bin_path);
            match std::process::Command::new(bin_path)
                .arg(format!("--sandbox-id={sandbox_id}"))
                .arg(format!("--sandbox-port={port}"))
                .arg(format!("--mode={mode}"))
                .arg(format!("--cmd={cmd}"))
                .spawn()
            {
                Ok(_) => return true,
                Err(e) => {
                    tracing::warn!("Failed to launch Tauri binary: {e}");
                    return false;
                }
            }
        }
    }

    tracing::info!("No Tauri app found, will use standalone server");
    false
}
