use anyhow::Context;
use clap::{Parser, Subcommand};
use sandbox_core::capture::ScreenCapture;
use std::path::PathBuf;
use std::process::Command;

/// macOS Desktop Automation Sandbox CLI
///
/// Run CLI commands or macOS apps in isolated sandbox windows,
/// take screenshots, and simulate mouse/keyboard input.
#[derive(Parser)]
#[command(name = "sandbox", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a sandbox with a CLI command in a Terminal window
    ///
    /// Opens Terminal.app and runs the specified command.
    /// Use 'sandbox screenshot' to capture the sandbox window.
    Start {
        /// Command to run (e.g., "claude", "node", "echo")
        command: String,

        /// Additional arguments passed to the command
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Take a screenshot of the sandbox window
    Screenshot {
        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: PathBuf,

        /// Window ID to capture (auto-detected if not specified)
        #[arg(long)]
        window_id: Option<u32>,
    },

    /// List all visible windows on the system
    Windows,

    /// Shutdown the sandbox (close the Terminal window)
    Shutdown,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { command, args } => {
            cmd_start(&command, &args)?;
        }
        Commands::Screenshot { output, window_id } => {
            cmd_screenshot(&output, window_id)?;
        }
        Commands::Windows => {
            cmd_windows()?;
        }
        Commands::Shutdown => {
            cmd_shutdown()?;
        }
    }

    Ok(())
}

/// Open Terminal.app and run the given command.
fn cmd_start(command: &str, args: &[String]) -> anyhow::Result<()> {
    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    // Escape double quotes for AppleScript
    let escaped = full_cmd.replace('\\', "\\\\").replace('"', "\\\"");

    let script = format!(
        r#"tell application "Terminal"
    activate
    do script "{}"
end tell"#,
        escaped
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to run osascript — is Terminal.app available?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to launch Terminal: {}", stderr.trim());
    }

    println!("Sandbox started: {}", full_cmd);
    println!("Use 'sandbox screenshot' to capture the sandbox window");
    Ok(())
}

/// Take a screenshot of the sandbox window.
///
/// Tries to auto-discover the Terminal window by title,
/// or uses --window-id if explicitly provided.
fn cmd_screenshot(output: &PathBuf, window_id: Option<u32>) -> anyhow::Result<()> {
    let id = if let Some(id) = window_id {
        id
    } else {
        // Auto-discover: look for a Terminal window
        discover_sandbox_window()?
    };

    let png = ScreenCapture::capture_window(id).context(format!(
        "Failed to capture window {id}. Is Screen Recording permission granted?"
    ))?;

    std::fs::write(output, &png).context(format!("Failed to write screenshot to {:?}", output))?;

    println!("Screenshot saved to {:?} ({} bytes)", output, png.len());
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
        // Truncate long titles
        let title_display = if title.len() > 64 {
            format!("{}...", &title[..61])
        } else {
            title.clone()
        };
        println!("{:<12}  {}", id, title_display);
    }
    println!("Total: {} windows", windows.len());
    Ok(())
}

/// Close the sandbox Terminal window.
fn cmd_shutdown() -> anyhow::Result<()> {
    let script = r#"tell application "Terminal"
    close first window
end tell"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("Failed to run osascript")?;

    if !output.status.success() {
        // Terminal may already be closed — not an error
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("osascript warning: {}", stderr.trim());
    }

    println!("Sandbox shutdown complete.");
    Ok(())
}

/// Auto-discover the sandbox Terminal window.
///
/// Searches for a window whose title contains "Terminal" or "sandbox" (case-insensitive).
fn discover_sandbox_window() -> anyhow::Result<u32> {
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    // Try finding a Terminal window first
    for (id, title) in &windows {
        let lower = title.to_lowercase();
        if lower.contains("terminal") || lower.contains("sandbox") {
            return Ok(*id);
        }
    }

    anyhow::bail!(
        "No sandbox window found automatically.\n\
         Use 'sandbox windows' to list all windows, then 'sandbox screenshot --window-id <ID>'."
    )
}
