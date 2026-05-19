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

/// Launch the Tauri sandbox app with the given CLI command inside it.
///
/// The sandbox app embeds an xterm.js terminal where the command runs.
fn cmd_start(command: &str, args: &[String]) -> anyhow::Result<()> {
    let bundle_path = find_tauri_bundle()?;
    let app_binary = bundle_path.join("Contents/MacOS/system-test-sandbox");

    // Build Tauri args: --mode=cli --cmd=<command> [-- <extra args>]
    let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
    if !args.is_empty() {
        tauri_args.push("--".to_string());
        tauri_args.extend(args.iter().cloned());
    }

    // Run the binary directly (not via open -a) so arguments are passed correctly
    Command::new(&app_binary)
        .args(&tauri_args)
        .spawn()
        .context("Failed to launch Tauri sandbox app")?;

    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };
    println!("Sandbox started: {}", full_cmd);
    println!("Use 'sandbox screenshot' to capture the sandbox window");
    Ok(())
}

/// Find the Tauri app bundle path.
///
/// Search order:
/// 1. <exe_dir>/System Test Sandbox.app              (release layout: side-by-side)
/// 2. <exe_dir>/bundle/macos/System Test Sandbox.app (cargo tauri build layout)
/// 3. <project_root>/target/release/bundle/macos/... (dev build layout)
fn find_tauri_bundle() -> anyhow::Result<PathBuf> {
    let app_name = "System Test Sandbox.app";
    let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
    let exe_dir = exe_path.parent().context("No parent dir for exe")?;

    // Try 1: side-by-side with CLI (release layout)
    let path1 = exe_dir.join(app_name);
    if path1.exists() {
        return Ok(path1);
    }

    // Try 2: <exe_dir>/bundle/macos/ (cargo tauri build output)
    let path2 = exe_dir.join("bundle/macos").join(app_name);
    if path2.exists() {
        return Ok(path2);
    }

    // Try 3: project root layout (exe in <root>/release/, bundle in <root>/target/release/bundle/macos/)
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
        // Truncate long titles, respecting UTF-8 character boundaries
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

/// Close the sandbox window (Tauri app or Terminal.app).
fn cmd_shutdown() -> anyhow::Result<()> {
    // Try closing the Tauri sandbox app window first
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    let tauri_window = windows
        .iter()
        .find(|(_, title)| title.starts_with("System Test Sandbox"));

    if let Some((id, title)) = tauri_window {
        // Close the Tauri app window — this also terminates the process
        println!("Closing sandbox window: {} (ID: {})", title, id);
        // Use osascript to close the window via its process
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
        // Fallback: close Terminal.app first window
        let script = r#"tell application "Terminal"
    close first window
end tell"#;
        let _ = Command::new("osascript").arg("-e").arg(script).output();
    }

    println!("Sandbox shutdown complete.");
    Ok(())
}

/// Auto-discover the sandbox window.
///
/// Priority order:
/// 1. Tauri sandbox app window (title = "System Test Sandbox")
/// 2. Terminal.app window (title pattern: "user — command — W×H")
/// 3. Any window containing the command name (e.g., "claude")
fn discover_sandbox_window() -> anyhow::Result<u32> {
    let windows = ScreenCapture::list_windows()
        .context("Failed to list windows. Is Screen Recording permission granted?")?;

    // Priority 1: Tauri sandbox app window
    // Titles may be "System Test Sandbox" or "System Test Sandbox [claude]"
    for (id, title) in &windows {
        if title.starts_with("System Test Sandbox") {
            return Ok(*id);
        }
    }

    // Priority 2: Terminal.app windows have titles like "user — command — 120×30"
    for (id, title) in &windows {
        if is_terminal_title(title) {
            return Ok(*id);
        }
    }

    // Priority 3: Fallback — match any window containing "claude"
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

/// Returns true if the title matches the Terminal.app window title pattern.
///
/// Terminal.app titles look like: "username — command — 120×30"
/// The trailing dimension suffix " — W×H" is the reliable marker.
fn is_terminal_title(title: &str) -> bool {
    // Find the last " — " separator (space + em dash + space, 5 bytes in UTF-8)
    let sep = " — ";
    let last_sep = match title.rfind(sep) {
        Some(pos) => pos + sep.len(),
        None => return false,
    };

    let suffix = &title[last_sep..];
    // suffix should be "W×H" like "120×30"
    let parts: Vec<&str> = suffix.split('×').collect();
    if parts.len() != 2 {
        return false;
    }
    parts[0].trim().parse::<u32>().is_ok() && parts[1].trim().parse::<u32>().is_ok()
}
