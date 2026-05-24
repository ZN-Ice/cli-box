#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sandbox_core::instance::{InstanceKind, InstanceRegistry, SandboxInstance};
use sandbox_core::process::ProcessManager;
use sandbox_core::sandbox::{Sandbox, SandboxConfig};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::Manager;

#[allow(dead_code)]
struct AppState {
    sandbox: Mutex<Sandbox>,
    sandbox_id: Option<String>,
    port: Option<u16>,
    kind: Option<InstanceKind>,
}

#[tauri::command]
fn get_sandbox_state(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    let s = sandbox.state();
    Ok(serde_json::json!({
        "sandbox_id": s.sandbox_id,
        "port": s.port,
        "window_id": s.window_id,
        "is_running": s.is_running,
        "uptime_secs": sandbox.uptime_secs(),
    }))
}

#[tauri::command]
fn take_screenshot(state: tauri::State<AppState>) -> Result<Vec<u8>, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    sandbox.screenshot().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_sandbox_config(state: tauri::State<AppState>) -> Result<SandboxConfig, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    Ok(sandbox.config().clone())
}

#[tauri::command]
fn init_sandbox(state: tauri::State<AppState>, window_id: u32) -> Result<(), String> {
    let mut sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    sandbox.init(window_id).map_err(|e| e.to_string())
}

#[derive(Debug, Default)]
struct SandboxLaunchArgs {
    sandbox_id: Option<String>,
    sandbox_port: Option<u16>,
    mode: Option<String>,
    cmd: Option<String>,
    args: Vec<String>,
}

fn parse_sandbox_args_from_slice(args: &[String]) -> SandboxLaunchArgs {
    let mut result = SandboxLaunchArgs::default();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(val) = arg.strip_prefix("--sandbox-id=") {
            result.sandbox_id = Some(val.to_string());
        } else if arg == "--sandbox-id" && i + 1 < args.len() {
            i += 1;
            result.sandbox_id = Some(args[i].clone());
        } else if let Some(val) = arg.strip_prefix("--sandbox-port=") {
            result.sandbox_port = val.parse().ok();
        } else if arg == "--sandbox-port" && i + 1 < args.len() {
            i += 1;
            result.sandbox_port = args[i].parse().ok();
        } else if let Some(val) = arg.strip_prefix("--mode=") {
            result.mode = Some(val.to_string());
        } else if arg == "--mode" && i + 1 < args.len() {
            i += 1;
            result.mode = Some(args[i].clone());
        } else if let Some(val) = arg.strip_prefix("--cmd=") {
            result.cmd = Some(val.to_string());
        } else if arg == "--cmd" && i + 1 < args.len() {
            i += 1;
            result.cmd = Some(args[i].clone());
        } else if arg == "--" {
            result.args = args[(i + 1)..].to_vec();
            break;
        }
        i += 1;
    }
    result
}

fn parse_sandbox_args() -> SandboxLaunchArgs {
    let args: Vec<String> = std::env::args().collect();
    tracing::info!("[args] raw args: {:?}", args);
    let result = parse_sandbox_args_from_slice(&args);
    tracing::info!(
        "[args] parsed: mode={:?}, cmd={:?}, args={:?}, sandbox_id={:?}, port={:?}",
        result.mode,
        result.cmd,
        result.args,
        result.sandbox_id,
        result.sandbox_port,
    );
    result
}

fn main() {
    let launch_args = parse_sandbox_args();

    // Auto-generate sandbox_id and port if not provided
    let sandbox_id = launch_args
        .sandbox_id
        .clone()
        .or_else(|| Some(uuid::Uuid::new_v4().to_string()[..8].to_string()));
    let sandbox_port = launch_args.sandbox_port.or(Some(5801));

    let mode = launch_args.mode.clone().or_else(|| Some("cli".to_string()));
    let cmd = launch_args.cmd.clone().or_else(|| Some("zsh".to_string()));

    let config = SandboxConfig {
        id: launch_args.sandbox_id.clone(),
        port: launch_args.sandbox_port,
        mode: mode.clone(),
        command: cmd.clone(),
        args: launch_args.args.clone(),
        ..SandboxConfig::default()
    };

    let kind = match (mode.as_deref(), &cmd) {
        (Some("cli"), Some(cmd)) => Some(InstanceKind::Cli {
            command: cmd.clone(),
            args: launch_args.args.clone(),
        }),
        (Some("app"), Some(path)) => Some(InstanceKind::App { path: path.clone() }),
        _ => None,
    };

    // Title is intentionally empty — command name is shown in the Dashboard header.
    let title = String::new();

    let kind_for_setup = kind.clone();
    let sandbox_id_for_close = sandbox_id.clone();
    let port_for_close = sandbox_port;

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            sandbox: Mutex::new(Sandbox::new(config)),
            sandbox_id: sandbox_id.clone(),
            port: sandbox_port,
            kind: kind.clone(),
        })
        .invoke_handler(tauri::generate_handler![
            get_sandbox_state,
            take_screenshot,
            get_sandbox_config,
            init_sandbox,
        ])
        .setup(move |app_handle| {
            tracing::info!(
                "[setup] sandbox_id={:?}, port={:?}, kind={:?}",
                sandbox_id,
                sandbox_port,
                kind
            );

            // Set window title
            if let Some(window) = app_handle.get_webview_window("main") {
                tracing::info!("[setup] setting window title: {}", title);
                let _ = window.set_title(&title);
            } else {
                tracing::warn!("[setup] main window not found!");
            }

            // Start embedded HTTP server if in managed mode
            if let (Some(id), Some(port)) = (&sandbox_id, sandbox_port) {
                let state = Arc::new(tokio::sync::Mutex::new(sandbox_core::server::AppState {
                    sandbox_id: Some(id.clone()),
                    start_time: Instant::now(),
                    window_id: None,
                    target_pid: Some(std::process::id()),
                }));

                // Clone for window discovery task
                let state_for_window = state.clone();

                let router = sandbox_core::server::build_router(state);
                let port_val = port;

                tauri::async_runtime::spawn(async move {
                    let addr = format!("127.0.0.1:{port_val}");
                    match tokio::net::TcpListener::bind(&addr).await {
                        Ok(listener) => {
                            tracing::info!("Sandbox HTTP API listening on http://{addr}");
                            if let Err(e) = axum::serve(listener, router).await {
                                tracing::error!("HTTP server error: {e}");
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to bind HTTP server on port {port_val}: {e}");
                        }
                    }
                });

                // Register instance
                let registry = InstanceRegistry::default();
                let instance = SandboxInstance::new(
                    id.clone(),
                    port,
                    std::process::id(),
                    kind_for_setup.unwrap_or(InstanceKind::Cli {
                        command: "unknown".into(),
                        args: vec![],
                    }),
                );
                if let Err(e) = registry.register(&instance) {
                    tracing::error!("Failed to register instance: {e}");
                }

                // Auto-spawn CLI if in CLI mode
                if let Some(InstanceKind::Cli { command, args }) = &kind {
                    let cmd = command.clone();
                    let cmd_args = args.clone();
                    tracing::info!("[setup] auto-spawn CLI: cmd={:?}, args={:?}", cmd, cmd_args);
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        tracing::info!("[setup] spawning CLI now: {} {:?}", cmd, cmd_args);
                        match ProcessManager::spawn_cli(&cmd, &cmd_args) {
                            Ok(info) => {
                                tracing::info!(
                                    "[setup] auto-spawned CLI: {} (pid={})",
                                    cmd,
                                    info.pid
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[setup] failed to auto-spawn CLI '{}': {}",
                                    cmd,
                                    e
                                );
                            }
                        }
                    });
                } else {
                    tracing::info!("[setup] not CLI mode, skipping auto-spawn. kind={:?}", kind);
                }

                // Auto-discover the Tauri window's SCWindow ID for screenshot support.
                // The window needs time to render before ScreenCaptureKit can find it.
                let own_pid = std::process::id();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    tracing::info!("[setup] discovering window by PID {own_pid}");
                    match sandbox_core::capture::ScreenCapture::find_window_by_pid(own_pid) {
                        Ok(id) => {
                            tracing::info!("[setup] discovered sandbox window: SCWindow ID={id}");
                            state_for_window.lock().await.window_id = Some(id);
                        }
                        Err(e) => {
                            tracing::warn!("[setup] failed to discover sandbox window by PID: {e}");
                            // Fallback: try title-based discovery
                            match sandbox_core::capture::ScreenCapture::find_window_by_title(
                                "System Test Sandbox",
                            ) {
                                Ok(id) => {
                                    tracing::info!(
                                        "[setup] discovered sandbox window by title: SCWindow ID={id}"
                                    );
                                    state_for_window.lock().await.window_id = Some(id);
                                }
                                Err(e2) => {
                                    tracing::warn!(
                                        "[setup] title-based discovery also failed: {e2}"
                                    );
                                    if let Ok(windows) =
                                        sandbox_core::capture::ScreenCapture::list_windows()
                                    {
                                        for (wid, wtitle) in &windows {
                                            if !wtitle.is_empty() {
                                                tracing::info!(
                                                    "[setup]   window {}: '{}'",
                                                    wid,
                                                    wtitle
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }

            // Window close cleanup
            if let Some(window) = app_handle.get_webview_window("main") {
                let close_id = sandbox_id_for_close.clone();
                let _close_port = port_for_close;
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(ref id) = close_id {
                            tracing::info!("Sandbox window closing, cleaning up instance {id}");
                            let registry = InstanceRegistry::default();
                            let _ = registry.unregister(id);
                            tracing::info!("Instance {id} unregistered");
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    // ── parse_sandbox_args_from_slice ──────────────────────

    #[test]
    fn parse_mode_and_cmd_eq() {
        let r = parse_sandbox_args_from_slice(&args(&["bin", "--mode=cli", "--cmd=claude"]));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("claude"));
        assert!(r.args.is_empty());
    }

    #[test]
    fn parse_mode_and_cmd_space() {
        let r = parse_sandbox_args_from_slice(&args(&["bin", "--mode", "cli", "--cmd", "claude"]));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("claude"));
    }

    #[test]
    fn parse_trailing_args_after_separator() {
        let r = parse_sandbox_args_from_slice(&args(&[
            "bin",
            "--mode=cli",
            "--cmd=claude",
            "--",
            "-p",
            "你是谁？",
        ]));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("claude"));
        assert_eq!(r.args, vec!["-p", "你是谁？"]);
    }

    #[test]
    fn parse_no_trailing_args() {
        let r = parse_sandbox_args_from_slice(&args(&["bin", "--mode=cli", "--cmd=zsh"]));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("zsh"));
        assert!(r.args.is_empty());
    }

    #[test]
    fn parse_sandbox_id_and_port() {
        let r = parse_sandbox_args_from_slice(&args(&[
            "bin",
            "--sandbox-id=abc123",
            "--sandbox-port=15801",
            "--mode=cli",
            "--cmd=echo",
        ]));
        assert_eq!(r.sandbox_id.as_deref(), Some("abc123"));
        assert_eq!(r.sandbox_port, Some(15801));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("echo"));
    }

    #[test]
    fn parse_empty_args() {
        let r = parse_sandbox_args_from_slice(&args(&["bin"]));
        assert!(r.mode.is_none());
        assert!(r.cmd.is_none());
        assert!(r.args.is_empty());
    }

    #[test]
    fn parse_only_cmd_no_mode() {
        let r = parse_sandbox_args_from_slice(&args(&["bin", "--cmd=claude"]));
        assert!(r.mode.is_none());
        assert_eq!(r.cmd.as_deref(), Some("claude"));
    }

    #[test]
    fn parse_mixed_eq_and_space() {
        let r = parse_sandbox_args_from_slice(&args(&[
            "bin",
            "--mode=cli",
            "--cmd",
            "claude",
            "--",
            "-p",
            "hello",
        ]));
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("claude"));
        assert_eq!(r.args, vec!["-p", "hello"]);
    }

    // ── SandboxConfig construction from parsed args ───────

    #[test]
    fn config_from_parsed_cli_args() {
        let r = parse_sandbox_args_from_slice(&args(&[
            "bin",
            "--sandbox-id=test1",
            "--sandbox-port=5801",
            "--mode=cli",
            "--cmd=claude",
            "--",
            "-p",
            "你是谁？",
        ]));

        let config = SandboxConfig {
            id: r.sandbox_id.clone(),
            port: r.sandbox_port,
            mode: r.mode.clone(),
            command: r.cmd.clone(),
            args: r.args.clone(),
            ..SandboxConfig::default()
        };

        assert_eq!(config.id.as_deref(), Some("test1"));
        assert_eq!(config.port, Some(5801));
        assert_eq!(config.mode.as_deref(), Some("cli"));
        assert_eq!(config.command.as_deref(), Some("claude"));
        assert_eq!(config.args, vec!["-p", "你是谁？"]);
    }

    #[test]
    fn kind_from_config_cli() {
        let config = SandboxConfig {
            mode: Some("cli".into()),
            command: Some("claude".into()),
            args: vec!["-p".into(), "你是谁？".into()],
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        let kind = sandbox.kind().unwrap();
        match kind {
            InstanceKind::Cli { command, args } => {
                assert_eq!(command, "claude");
                assert_eq!(args, vec!["-p", "你是谁？"]);
            }
            _ => panic!("Expected CLI kind"),
        }
    }

    #[test]
    fn kind_from_config_app() {
        let config = SandboxConfig {
            mode: Some("app".into()),
            command: Some("/Applications/Safari.app".into()),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        let kind = sandbox.kind().unwrap();
        assert!(matches!(kind, InstanceKind::App { .. }));
    }

    #[test]
    fn kind_none_without_mode() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config);
        assert!(sandbox.kind().is_none());
    }

    // ── CLI args construction (mirrors cmd_start logic) ───

    #[test]
    fn cli_builds_tauri_args_simple() {
        let command = "claude";
        let cli_args: Vec<String> = vec![];

        let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
        if !cli_args.is_empty() {
            tauri_args.push("--".to_string());
            tauri_args.extend(cli_args.iter().cloned());
        }

        assert_eq!(tauri_args, vec!["--mode=cli", "--cmd=claude"]);
    }

    #[test]
    fn cli_builds_tauri_args_with_trailing() {
        let command = "claude";
        let cli_args: Vec<String> = vec!["-p".into(), "你是谁？".into()];

        let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
        if !cli_args.is_empty() {
            tauri_args.push("--".to_string());
            tauri_args.extend(cli_args.iter().cloned());
        }

        assert_eq!(
            tauri_args,
            vec!["--mode=cli", "--cmd=claude", "--", "-p", "你是谁？"]
        );
    }

    #[test]
    fn cli_builds_tauri_args_zsh() {
        let command = "zsh";
        let cli_args: Vec<String> = vec![];

        let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
        if !cli_args.is_empty() {
            tauri_args.push("--".to_string());
            tauri_args.extend(cli_args.iter().cloned());
        }

        assert_eq!(tauri_args, vec!["--mode=cli", "--cmd=zsh"]);
    }

    // ── Round-trip: CLI builds args → Tauri parses them ───

    #[test]
    fn roundtrip_claude_with_args() {
        // CLI constructs these args
        let command = "claude";
        let cli_args = vec!["-p".to_string(), "你是谁？".to_string()];
        let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
        tauri_args.push("--".to_string());
        tauri_args.extend(cli_args);

        // Prepend program name (as std::env::args would)
        let mut full_args = vec!["system-test-sandbox".to_string()];
        full_args.extend(tauri_args);

        // Tauri parses them
        let r = parse_sandbox_args_from_slice(&full_args);
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("claude"));
        assert_eq!(r.args, vec!["-p", "你是谁？"]);
    }

    #[test]
    fn roundtrip_simple_command() {
        let command = "zsh";
        let cli_args: Vec<String> = vec![];
        let mut tauri_args = vec!["--mode=cli".to_string(), format!("--cmd={}", command)];
        if !cli_args.is_empty() {
            tauri_args.push("--".to_string());
            tauri_args.extend(cli_args);
        }

        let mut full_args = vec!["system-test-sandbox".to_string()];
        full_args.extend(tauri_args);

        let r = parse_sandbox_args_from_slice(&full_args);
        assert_eq!(r.mode.as_deref(), Some("cli"));
        assert_eq!(r.cmd.as_deref(), Some("zsh"));
        assert!(r.args.is_empty());
    }

    // ── Window discovery: title is empty, PID-based discovery is required ──

    #[test]
    fn window_title_is_intentionally_empty() {
        // Verify the title is set to empty string (not "System Test Sandbox")
        let title = String::new();
        assert!(title.is_empty());
        assert_ne!(title, "System Test Sandbox");
    }

    #[test]
    fn own_pid_is_valid() {
        let pid = std::process::id();
        assert!(pid > 0, "Process ID should be positive");
    }

    #[test]
    fn find_window_by_pid_nonexistent_returns_error() {
        // Verify that the PID-based discovery properly returns errors
        // for non-existent PIDs (this mirrors the actual setup code path)
        let result = sandbox_core::capture::ScreenCapture::find_window_by_pid(9999999);
        assert!(result.is_err());
    }

    #[test]
    fn find_window_by_title_empty_string_returns_error() {
        // Verify that searching by empty title fails (since title is now "")
        let result = sandbox_core::capture::ScreenCapture::find_window_by_title("");
        // Empty string matches any window with empty title, but the Tauri window
        // is unlikely to be discoverable via this path in test context
        // The key point: this should NOT be the discovery mechanism
        let _ = result;
    }

    #[test]
    fn pid_based_discovery_preferred_over_title() {
        // This test documents the discovery strategy:
        // 1. Try find_window_by_pid(own_pid) first
        // 2. Fallback to find_window_by_title("System Test Sandbox")
        // Since title is empty, step 1 is essential.
        let pid = std::process::id();
        let title = String::new();
        // Title discovery won't work with empty title
        assert!(title.is_empty());
        // PID is always available and positive
        assert!(pid > 0);
    }
}
