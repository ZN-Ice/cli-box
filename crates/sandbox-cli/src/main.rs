use std::path::PathBuf;
use clap::{Parser, Subcommand};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, width, height } => {
            println!("Starting sandbox server on port {} ({}x{})", port, width, height);
            // TODO: Start HTTP + MCP server
        }
        Commands::Screenshot { output } => {
            let path = output.unwrap_or_else(|| PathBuf::from("sandbox_screenshot.png"));
            println!("Taking screenshot -> {:?}", path);
            // TODO: Capture sandbox window
        }
        Commands::Windows => {
            println!("Listing sandbox windows...");
            // TODO: List windows
        }
        Commands::Processes => {
            println!("Listing sandbox processes...");
            // TODO: List processes
        }
        Commands::SpawnApp { path } => {
            println!("Spawning app: {}", path);
            // TODO: Launch app
        }
        Commands::SpawnCli { command, args } => {
            println!("Spawning CLI: {} {:?}", command, args);
            // TODO: Launch CLI
        }
        Commands::Click { x, y, button } => {
            println!("Clicking at ({}, {}) button={}", x, y, button);
            // TODO: CGEvent click
        }
        Commands::Type { text } => {
            println!("Typing: {}", text);
            // TODO: CGEvent type
        }
        Commands::Key { key, modifiers } => {
            println!("Pressing key: {} modifiers={:?}", key, modifiers);
            // TODO: CGEvent key press
        }
    }

    Ok(())
}
