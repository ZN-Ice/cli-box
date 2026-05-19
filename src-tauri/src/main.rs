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

fn parse_sandbox_args() -> SandboxLaunchArgs {
    let args: Vec<String> = std::env::args().collect();
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
            // Everything after -- is passed as trailing args to the CLI command
            result.args = args[(i + 1)..].to_vec();
            break;
        }
        i += 1;
    }
    result
}

fn main() {
    let launch_args = parse_sandbox_args();

    // Auto-generate sandbox_id and port if not provided
    let sandbox_id = launch_args.sandbox_id.clone().or_else(|| {
        Some(format!(
            "{}",
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        ))
    });
    let sandbox_port = launch_args.sandbox_port.or(Some(5801));

    let config = SandboxConfig {
        id: launch_args.sandbox_id.clone(),
        port: launch_args.sandbox_port,
        mode: launch_args.mode.clone(),
        command: launch_args.cmd.clone(),
        args: launch_args.args.clone(),
        ..SandboxConfig::default()
    };

    let kind = match (launch_args.mode.as_deref(), &launch_args.cmd) {
        (Some("cli"), Some(cmd)) => Some(InstanceKind::Cli {
            command: cmd.clone(),
            args: launch_args.args.clone(),
        }),
        (Some("app"), Some(path)) => Some(InstanceKind::App { path: path.clone() }),
        _ => None,
    };

    let title = match &kind {
        Some(InstanceKind::Cli { command, .. }) => format!("System Test Sandbox [{command}]"),
        Some(InstanceKind::App { path }) => {
            let name = std::path::Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            format!("System Test Sandbox [{name}]")
        }
        None => "System Test Sandbox".to_string(),
    };

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
            // Set window title
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.set_title(&title);
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
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        match ProcessManager::spawn_cli(&cmd, &cmd_args) {
                            Ok(info) => {
                                tracing::info!("Auto-spawned CLI: {} (pid={})", cmd, info.pid);
                            }
                            Err(e) => {
                                tracing::error!("Failed to auto-spawn CLI '{cmd}': {e}");
                            }
                        }
                    });
                }

                // Auto-discover the Tauri window's SCWindow ID for screenshot support.
                // The window needs time to render before ScreenCaptureKit can find it.
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    match sandbox_core::capture::ScreenCapture::find_window_by_title(
                        "System Test Sandbox",
                    ) {
                        Ok(id) => {
                            tracing::info!("Discovered sandbox window: SCWindow ID={id}");
                            state_for_window.lock().await.window_id = Some(id);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to discover sandbox window: {e}");
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
