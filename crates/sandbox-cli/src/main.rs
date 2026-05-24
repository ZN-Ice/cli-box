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
        Commands::Start {
            command,
            args,
            shell,
        } => {
            let (cmd, cmd_args) = match (shell, command) {
                (true, _) | (false, None) => ("zsh".to_string(), args),
                (false, Some(c)) => (c, args),
            };
            cmd_start(&cmd, &cmd_args)?;
        }
        Commands::List => {
            cmd_list()?;
        }
        Commands::Inspect { id } => {
            cmd_inspect(&id).await?;
        }
        Commands::Close { id } => {
            cmd_close(&id).await?;
        }
        Commands::TypeText { text, id, pty } => {
            cmd_type(&text, &id, pty).await?;
        }
        Commands::Key {
            key,
            id,
            modifiers,
            pty,
        } => {
            cmd_key(&key, &id, &modifiers, pty).await?;
        }
        Commands::Click { x, y, id, button } => {
            cmd_click(x, y, &id, &button).await?;
        }
        Commands::Screenshot {
            output,
            id,
            window_id,
        } => {
            cmd_screenshot(&output, id.as_deref(), window_id).await?;
        }
        Commands::Windows => {
            cmd_windows()?;
        }
        Commands::Processes { id } => {
            cmd_processes(&id).await?;
        }
        Commands::Shutdown => {
            cmd_shutdown()?;
        }
    }

    Ok(())
}

// ── Command Implementations ─────────────────────────────

/// Launch the Tauri sandbox app with the given CLI command inside it.
fn cmd_start(command: &str, args: &[String]) -> anyhow::Result<()> {
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

    tracing::info!("[start] child pid: {:?}", child.id());

    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };
    println!("Sandbox started: {}", full_cmd);
    println!("Use 'sandbox list' to find the sandbox ID");
    Ok(())
}

/// List all registered sandbox instances.
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

/// Show details of a specific sandbox instance.
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

/// Close a sandbox instance via HTTP API.
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

/// Type text into a sandbox.
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

/// Press a key in a sandbox.
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

/// Click at coordinates in a sandbox.
async fn cmd_click(x: f64, y: f64, id: &str, button: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    client.click(x, y, button).await?;
    println!("Clicked ({}, {}) [{}] → sandbox {}", x, y, button, id);
    Ok(())
}

/// Take a screenshot.
async fn cmd_screenshot(
    output: &PathBuf,
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

/// List processes running inside a sandbox instance.
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

/// Legacy shutdown command.
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
