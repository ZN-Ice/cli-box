use clap::{Parser, Subcommand};
use sandbox_core::automation::cg_event::{InputSimulator, MouseButton};
use sandbox_core::capture::ScreenCapture;
use sandbox_core::process::ProcessManager;
use sandbox_core::recorder::ActionRecorder;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

mod mcp_server;
mod server;

#[derive(Parser)]
#[command(name = "sandbox-cli", about = "macOS Desktop Automation Sandbox CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the sandbox server (HTTP + MCP)
    Serve {
        /// HTTP port
        #[arg(long, default_value = "5801")]
        port: u16,

        /// Sandbox window width
        #[arg(long, default_value = "1280")]
        width: u32,

        /// Sandbox window height
        #[arg(long, default_value = "800")]
        height: u32,
    },

    /// Take a screenshot of the sandbox
    Screenshot {
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List windows in the sandbox
    Windows,

    /// List processes in the sandbox
    Processes,

    /// Spawn an app in the sandbox
    SpawnApp {
        /// Path to the .app bundle
        path: String,
    },

    /// Spawn a CLI in the sandbox
    SpawnCli {
        /// Command to run
        command: String,

        /// Additional arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Simulate mouse click
    Click {
        x: f64,
        y: f64,
        #[arg(short, long, default_value = "left")]
        button: String,
    },

    /// Simulate typing text
    Type {
        /// Text to type
        text: String,
    },

    /// Simulate key press
    Key {
        /// Key name (e.g., Return, Tab, Space)
        key: String,

        /// Modifier keys (e.g., cmd, shift, alt)
        #[arg(short, long)]
        modifiers: Vec<String>,
    },

    /// Kill a process by PID
    Kill {
        /// Process ID
        pid: u32,
    },

    /// Start MCP server via stdio (for Claude Code / OpenCode integration)
    McpServe,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            port,
            width,
            height,
        } => {
            tracing::info!(
                "Starting sandbox server on port {} ({}x{})",
                port,
                width,
                height
            );

            let state = Arc::new(Mutex::new(server::AppState {
                start_time: Instant::now(),
                sandbox_title: "System Test Sandbox".to_string(),
                recorder: ActionRecorder::new(),
            }));

            let app = server::build_router(state);
            let addr = format!("127.0.0.1:{}", port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;

            tracing::info!("HTTP API server listening on http://{}", addr);
            println!("Sandbox HTTP API server started on http://{}", addr);
            println!("  GET  http://{}/health", addr);
            println!("  GET  http://{}/screenshot", addr);
            println!("  POST http://{}/input/click", addr);
            println!("  POST http://{}/cli/spawn", addr);

            axum::serve(listener, app).await?;
        }
        Commands::Screenshot { output } => {
            let path = output.unwrap_or_else(|| PathBuf::from("sandbox_screenshot.png"));
            tracing::info!("Taking screenshot -> {:?}", path);

            let png_data = ScreenCapture::capture_sandbox()?;
            std::fs::write(&path, &png_data)?;
            println!("Screenshot saved to {:?} ({} bytes)", path, png_data.len());
        }
        Commands::Windows => {
            tracing::info!("Listing windows...");
            let windows = ScreenCapture::list_windows()?;
            for (id, title) in &windows {
                println!("  Window ID={}: {}", id, title);
            }
            println!("Total: {} windows", windows.len());
        }
        Commands::Processes => {
            tracing::info!("Listing processes...");
            let processes = ProcessManager::list_processes()?;
            for p in &processes {
                println!("  PID={}: {} (running={})", p.pid, p.name, p.is_running);
            }
            println!("Total: {} processes", processes.len());
        }
        Commands::SpawnApp { path } => {
            tracing::info!("Spawning app: {}", path);
            let info = ProcessManager::spawn_app(&path)?;
            println!("App spawned: PID={}, name={}", info.pid, info.name);
        }
        Commands::SpawnCli { command, args } => {
            tracing::info!("Spawning CLI: {} {:?}", command, args);
            let args_refs: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            let info = ProcessManager::spawn_cli(&command, &args_refs)?;
            println!("CLI spawned: PID={}, name={}", info.pid, info.name);
            println!("Use 'sandbox-cli kill {}' to terminate", info.pid);
        }
        Commands::Click { x, y, button } => {
            let button = match button.to_lowercase().as_str() {
                "left" => MouseButton::Left,
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                other => anyhow::bail!("Unknown button: {}. Use left, right, or middle.", other),
            };
            tracing::info!("Clicking at ({}, {}) button={:?}", x, y, button);
            InputSimulator::click(x, y, button)?;
            println!("Clicked at ({}, {})", x, y);
        }
        Commands::Type { text } => {
            tracing::info!("Typing: {}", text);
            InputSimulator::type_text(&text)?;
            println!("Typed: {}", text);
        }
        Commands::Key { key, modifiers } => {
            tracing::info!("Pressing key: {} modifiers={:?}", key, modifiers);
            let mod_refs: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
            InputSimulator::press_key(&key, &mod_refs)?;
            println!("Pressed key: {} {:?}", key, modifiers);
        }
        Commands::Kill { pid } => {
            tracing::info!("Killing process: {}", pid);
            ProcessManager::kill_process(pid)?;
            println!("Process {} terminated", pid);
        }
        Commands::McpServe => {
            tracing::info!("Starting MCP server on stdio");
            mcp_server::run_stdio_server().await?;
        }
    }

    Ok(())
}
