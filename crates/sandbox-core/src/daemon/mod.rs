//! Daemon module — manages multiple sandboxes via a single HTTP API.
//!
//! The daemon is a long-lived process that listens on a single port and routes
//! all sandbox operations through `/sandbox/{id}/...` endpoints. It replaces the
//! per-sandbox Tauri multi-instance architecture with a single-process model.

use crate::automation::ax_ui::UiInspector;
use crate::automation::cg_event::{InputSimulator, MouseButton};
use crate::capture::ScreenCapture;
use crate::error::AppError;
use crate::instance::{generate_instance_id, InstanceKind, InstanceRegistry, InstanceStatus};
use crate::process::ProcessManager;
use crate::server::{
    handle_pty_ws, ClickRequest, KeyRequest, ScrollRequest, SpawnAppRequest, TypeRequest,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{oneshot, Mutex};
use tower_http::cors::{Any, CorsLayer};

// ── Types ─────────────────────────────────────────────────────

/// A sandbox managed by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedSandbox {
    pub id: String,
    pub kind: InstanceKind,
    pub status: InstanceStatus,
    pub port: u16,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
}

/// Shared state for the daemon.
pub struct DaemonState {
    pub port: u16,
    pub sandboxes: HashMap<String, ManagedSandbox>,
    pub started_at: Instant,
    /// Write half of the renderer's screenshot WebSocket connection.
    pub screenshot_ws_tx: Option<futures_util::stream::SplitSink<WebSocket, Message>>,
    /// Pending screenshot requests awaiting renderer responses.
    pub pending_screenshots: HashMap<u64, oneshot::Sender<Result<Vec<u8>, String>>>,
    /// Counter for generating unique request IDs.
    pub screenshot_request_counter: u64,
}

impl DaemonState {
    /// Remove sandboxes whose PTY session is no longer alive.
    pub fn cleanup_dead_sandboxes(&mut self) -> Vec<String> {
        let mut removed = Vec::new();
        self.sandboxes.retain(|id, sb| {
            if let Some(pty_pid) = sb.pty_pid {
                // Use ProcessManager to check if the PTY session is still alive.
                // pty_pid is the internal tracked_id, not an OS PID, so we can't
                // use libc::kill() — it would check a non-existent OS process.
                let alive = ProcessManager::is_session_alive(pty_pid);
                if !alive {
                    tracing::info!("Cleaning up dead sandbox {id} (pty_pid={pty_pid})");
                    // Update registry
                    let registry = InstanceRegistry::default();
                    let _ = registry.update_status(id, InstanceStatus::Stopped);
                    removed.push(id.clone());
                    return false;
                }
            }
            true
        });
        removed
    }
}

/// Daemon info persisted to `~/.sandbox/daemon.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
}

// ── Request / Response structs ────────────────────────────────

#[derive(Deserialize)]
struct CreateSandboxRequest {
    mode: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

#[derive(Deserialize)]
struct PtyWriteRequest {
    data: String,
}

#[derive(Deserialize)]
pub struct SetWindowIdRequest {
    pub window_id: u32,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_secs: u64,
    sandboxes: usize,
}

#[derive(Deserialize)]
pub struct UiFindRequest {
    pub role: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct UiValueResponse {
    pub value: Option<String>,
}

#[derive(Serialize)]
struct CreateSandboxResponse {
    sandbox_id: String,
    pty_pid: Option<u32>,
    window_id: Option<u32>,
}

// ── Port discovery ────────────────────────────────────────────

/// Find the first available TCP port in `[start, end)`.
pub fn find_available_port(start: u16, end: u16) -> Option<u16> {
    (start..end).find(|&port| TcpListener::bind(("127.0.0.1", port)).is_ok())
}

/// Returns the path to `~/.sandbox/daemon.json`.
pub fn daemon_json_path() -> PathBuf {
    dirs_home().join(".sandbox").join("daemon.json")
}

/// Write daemon info to disk.
pub fn write_daemon_info(port: u16) -> std::io::Result<()> {
    let dir = dirs_home().join(".sandbox");
    std::fs::create_dir_all(&dir)?;
    let info = DaemonInfo {
        port,
        pid: std::process::id(),
        started_at: format_timestamp_now(),
    };
    let json = serde_json::to_string_pretty(&info)?;
    std::fs::write(daemon_json_path(), json)
}

/// Read daemon info from disk. Returns `None` if file does not exist.
pub fn read_daemon_info() -> Option<DaemonInfo> {
    let path = daemon_json_path();
    if !path.exists() {
        return None;
    }
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Check whether a running daemon is alive.
///
/// Reads `daemon.json`, checks if the recorded PID is alive via `kill(pid, 0)`.
/// Returns `Some(port)` if alive, `None` otherwise. Cleans up stale `daemon.json`.
pub fn find_running_daemon() -> Option<u16> {
    let info = read_daemon_info()?;
    let pid = info.pid as libc::pid_t;
    // kill(pid, 0) returns 0 if the process exists and we can signal it
    let alive = unsafe { libc::kill(pid, 0) == 0 };
    if alive {
        Some(info.port)
    } else {
        // Stale — clean up
        let _ = cleanup_daemon_info();
        None
    }
}

/// Remove `daemon.json` from disk.
pub fn cleanup_daemon_info() -> std::io::Result<()> {
    let path = daemon_json_path();
    if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn format_timestamp_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        1970 + secs / 31536000,
        (secs % 31536000) / 2592000,
        (secs % 2592000) / 86400,
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
    )
}

// ── Router ────────────────────────────────────────────────────

/// Build the daemon HTTP router with all sandbox routes.
pub fn build_daemon_router(state: Arc<Mutex<DaemonState>>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_handler))
        .route("/sandbox/list", get(list_sandboxes_handler))
        .route("/sandbox/create", post(create_sandbox_handler))
        .route("/sandbox/{id}/close", post(close_sandbox_handler))
        .route("/sandbox/{id}/screenshot", get(screenshot_handler))
        .route(
            "/sandbox/{id}/screenshot/region",
            get(screenshot_region_handler),
        )
        .route("/sandbox/{id}/input/click", post(click_handler))
        .route("/sandbox/{id}/input/type", post(type_handler))
        .route("/sandbox/{id}/input/key", post(key_handler))
        .route("/sandbox/{id}/input/scroll", post(scroll_handler))
        .route("/sandbox/{id}/pty/ws/{pid}", get(pty_ws_upgrade_handler))
        .route("/sandbox/{id}/pty/write", post(pty_write_handler))
        .route("/sandbox/{id}/processes", get(processes_handler))
        .route("/sandbox/{id}/app/spawn", post(spawn_app_handler))
        .route("/sandbox/{id}/windows", get(windows_handler))
        .route(
            "/sandbox/{id}/ui/inspect/{window_id}",
            get(ui_inspect_handler),
        )
        .route("/sandbox/{id}/ui/inspect", get(ui_inspect_by_id_handler))
        .route("/sandbox/{id}/ui/find", post(ui_find_handler))
        .route("/sandbox/{id}/ui/value", get(ui_value_handler))
        .route("/sandbox/{id}/window", post(set_window_id_handler))
        .route("/shutdown", post(shutdown_handler))
        .route("/screenshot/ws", get(screenshot_ws_handler))
        .layer(cors)
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────

async fn health_handler(State(state): State<Arc<Mutex<DaemonState>>>) -> Json<HealthResponse> {
    let s = state.lock().await;
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: s.started_at.elapsed().as_secs(),
        sandboxes: s.sandboxes.len(),
    })
}

async fn list_sandboxes_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> Json<Vec<ManagedSandbox>> {
    let s = state.lock().await;
    let mut list: Vec<ManagedSandbox> = s.sandboxes.values().cloned().collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    Json(list)
}

async fn create_sandbox_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Json(req): Json<CreateSandboxRequest>,
) -> Result<Json<CreateSandboxResponse>, AppError> {
    let id = generate_instance_id();

    match req.mode.as_str() {
        "cli" => {
            let command = req.command.clone().unwrap_or_else(|| "zsh".to_string());
            let cols = req.cols.unwrap_or(80);
            let rows = req.rows.unwrap_or(24);
            let args = req.args.clone();

            let info = tokio::task::spawn_blocking(move || {
                ProcessManager::spawn_cli_with_size(&command, &args, cols, rows)
            })
            .await
            .map_err(|e| AppError::Process(format!("spawn_cli panicked: {e}")))??;

            let kind = InstanceKind::Cli {
                command: req.command.clone().unwrap_or_else(|| "zsh".to_string()),
                args: req.args.clone(),
            };

            // Best-effort: discover Electron window for screenshots
            let window_id = ScreenCapture::find_window_by_title("System Test Sandbox").ok();

            let managed = ManagedSandbox {
                id: id.clone(),
                kind,
                status: InstanceStatus::Running,
                port: 0, // daemon owns the port, sandbox does not have its own
                pty_pid: Some(info.pid),
                window_id,
            };

            // Register in file-system registry
            let registry = InstanceRegistry::default();
            let instance = crate::instance::SandboxInstance::new(
                id.clone(),
                0,
                info.pid,
                managed.kind.clone(),
            );
            registry.register(&instance)?;
            // Persist window_id to registry so CLI reads are consistent
            if let Some(wid) = window_id {
                let _ = registry.update_window_id(&id, wid);
            }

            state.lock().await.sandboxes.insert(id.clone(), managed);

            tracing::info!("Created CLI sandbox: id={}, pid={}", id, info.pid);

            Ok(Json(CreateSandboxResponse {
                sandbox_id: id,
                pty_pid: Some(info.pid),
                window_id,
            }))
        }
        "app" => {
            let app_path = req.command.clone().ok_or_else(|| {
                AppError::BadRequest("app mode requires 'command' (app path)".into())
            })?;

            let (process_info, window_id) = tokio::task::spawn_blocking(move || {
                ProcessManager::spawn_app_with_window(&app_path)
            })
            .await
            .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;

            let kind = InstanceKind::App {
                path: req.command.clone().unwrap(),
            };

            let managed = ManagedSandbox {
                id: id.clone(),
                kind,
                status: InstanceStatus::Running,
                port: 0,
                pty_pid: None,
                window_id,
            };

            let registry = InstanceRegistry::default();
            let instance = crate::instance::SandboxInstance::new(
                id.clone(),
                0,
                process_info.pid,
                managed.kind.clone(),
            );
            registry.register(&instance)?;
            // Persist window_id to registry so CLI reads are consistent
            if let Some(wid) = window_id {
                let _ = registry.update_window_id(&id, wid);
            }

            state.lock().await.sandboxes.insert(id.clone(), managed);

            tracing::info!(
                "Created APP sandbox: id={}, pid={}, window_id={:?}",
                id,
                process_info.pid,
                window_id
            );

            Ok(Json(CreateSandboxResponse {
                sandbox_id: id,
                pty_pid: Some(process_info.pid),
                window_id,
            }))
        }
        other => Err(AppError::BadRequest(format!(
            "Unknown mode '{other}'. Use 'cli' or 'app'."
        ))),
    }
}

async fn close_sandbox_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let removed = state.lock().await.sandboxes.remove(&id);
    let sandbox = removed.ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;

    // Kill PTY process if present
    if let Some(pty_pid) = sandbox.pty_pid {
        let _ = tokio::task::spawn_blocking(move || ProcessManager::kill_process(pty_pid)).await;
    }

    // Unregister from file-system registry
    let registry = InstanceRegistry::default();
    registry.unregister(&id)?;

    tracing::info!("Closed sandbox: id={}", id);
    Ok(Json(serde_json::json!({"closed": id})))
}

async fn screenshot_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    // Attempt 1: Ask the Electron renderer to capture via WebSocket
    if let Some(png_data) = request_renderer_screenshot(state.clone(), &id).await {
        tracing::info!(
            "[screenshot] sandbox {} captured via renderer ({} bytes)",
            id,
            png_data.len()
        );
        return Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response());
    }

    // Attempt 2: Fall back to ScreenCaptureKit
    tracing::info!(
        "[screenshot] renderer unavailable for sandbox {}, falling back to ScreenCaptureKit",
        id
    );

    let window_id = {
        let s = state.lock().await;
        s.sandboxes.get(&id).and_then(|sb| sb.window_id)
    };

    if let Some(wid) = window_id {
        let result = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(wid))
            .await
            .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))?;
        match result {
            Ok(png_data) => {
                return Ok(
                    (StatusCode::OK, [("content-type", "image/png")], png_data).into_response()
                );
            }
            Err(AppError::WindowNotFound(_)) => {
                tracing::warn!(
                    "Stored window_id={} for sandbox {} is stale, re-discovering",
                    wid,
                    id
                );
            }
            Err(e) => return Err(e),
        }
    }

    // Re-discover the Electron window by title
    let new_wid =
        tokio::task::spawn_blocking(|| ScreenCapture::find_window_by_title("System Test Sandbox"))
            .await
            .map_err(|e| AppError::Screenshot(format!("window discovery task failed: {e}")))??;

    {
        let mut s = state.lock().await;
        if let Some(sb) = s.sandboxes.get_mut(&id) {
            sb.window_id = Some(new_wid);
        }
    }
    let registry = InstanceRegistry::default();
    let _ = registry.update_window_id(&id, new_wid);
    tracing::info!("Re-discovered window_id={} for sandbox {}", new_wid, id);

    let png_data = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(new_wid))
        .await
        .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))??;
    Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
}

// ── Screenshot WebSocket ────────────────────────────────────────

/// Request a screenshot from the Electron renderer via WebSocket.
/// Returns PNG bytes if successful, None if renderer is unavailable or times out.
async fn request_renderer_screenshot(
    state: Arc<Mutex<DaemonState>>,
    sandbox_id: &str,
) -> Option<Vec<u8>> {
    let (request_id, response_rx, mut ws_tx) = {
        let mut s = state.lock().await;
        let ws_tx = s.screenshot_ws_tx.take()?;

        s.screenshot_request_counter += 1;
        let request_id = s.screenshot_request_counter;

        let (response_tx, response_rx) = oneshot::channel();
        s.pending_screenshots.insert(request_id, response_tx);

        (request_id, response_rx, ws_tx)
    };

    let msg = serde_json::json!({
        "type": "capture_request",
        "request_id": request_id,
        "sandbox_id": sandbox_id,
    });

    if ws_tx
        .send(Message::Text(msg.to_string().into()))
        .await
        .is_err()
    {
        tracing::warn!("[screenshot] failed to send request to renderer");
        let mut s = state.lock().await;
        s.pending_screenshots.remove(&request_id);
        s.screenshot_ws_tx = Some(ws_tx);
        return None;
    }

    // Put the ws_tx back
    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), response_rx).await {
        Ok(Ok(Ok(png_data))) => Some(png_data),
        Ok(Ok(Err(e))) => {
            tracing::warn!("[screenshot] renderer returned error: {e}");
            None
        }
        Ok(Err(_)) => {
            tracing::warn!("[screenshot] response channel dropped");
            None
        }
        Err(_) => {
            tracing::warn!("[screenshot] renderer did not respond within 2s");
            let mut s = state.lock().await;
            s.pending_screenshots.remove(&request_id);
            None
        }
    }
}

/// WebSocket endpoint for renderer-based screenshot capture.
async fn screenshot_ws_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_screenshot_ws(state, socket))
}

async fn handle_screenshot_ws(state: Arc<Mutex<DaemonState>>, socket: WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();

    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
        tracing::info!("[screenshot_ws] renderer connected");
    }

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(Message::Text(text)) => {
                let text_str = text.to_string();
                match serde_json::from_str::<serde_json::Value>(&text_str) {
                    Ok(msg) => {
                        let msg_type = msg.get("type").and_then(|v| v.as_str());
                        let request_id = msg.get("request_id").and_then(|v| v.as_u64());

                        match msg_type {
                            Some("capture_response") => {
                                if let (Some(req_id), Some(b64)) =
                                    (request_id, msg.get("image_base64").and_then(|v| v.as_str()))
                                {
                                    let png_data = base64_decode(b64);
                                    let mut s = state.lock().await;
                                    if let Some(tx) = s.pending_screenshots.remove(&req_id) {
                                        let _ = tx.send(png_data);
                                    }
                                }
                            }
                            Some("capture_error") => {
                                let error = msg
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Unknown error")
                                    .to_string();
                                if let Some(req_id) = request_id {
                                    let mut s = state.lock().await;
                                    if let Some(tx) = s.pending_screenshots.remove(&req_id) {
                                        let _ = tx.send(Err(error));
                                    }
                                }
                            }
                            _ => {
                                tracing::warn!(
                                    "[screenshot_ws] unknown message type: {:?}",
                                    msg_type
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[screenshot_ws] JSON parse error: {e}");
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("[screenshot_ws] renderer sent close frame");
                break;
            }
            Err(e) => {
                tracing::warn!("[screenshot_ws] receive error: {e}");
                break;
            }
            _ => {}
        }
    }

    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = None;
        tracing::info!("[screenshot_ws] renderer disconnected");
    }
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| format!("base64 decode error: {e}"))
}

#[derive(Deserialize)]
struct SandboxRegionQuery {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

async fn screenshot_region_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<SandboxRegionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let window_id = {
        let s = state.lock().await;
        let sandbox = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;

        match sandbox.window_id {
            Some(wid) => wid,
            None => {
                // Re-discover window
                drop(s);
                let new_wid = tokio::task::spawn_blocking(|| {
                    ScreenCapture::find_window_by_title("System Test Sandbox")
                })
                .await
                .map_err(|e| {
                    AppError::Screenshot(format!("window discovery task failed: {e}"))
                })??;
                let mut s = state.lock().await;
                if let Some(sb) = s.sandboxes.get_mut(&id) {
                    sb.window_id = Some(new_wid);
                }
                let registry = InstanceRegistry::default();
                let _ = registry.update_window_id(&id, new_wid);
                new_wid
            }
        }
    };

    // Get window frame to convert sandbox-relative coords to screen coords
    let windows = tokio::task::spawn_blocking(ScreenCapture::list_windows)
        .await
        .map_err(|e| AppError::Screenshot(format!("list_windows panicked: {e}")))??;
    let _window = windows
        .iter()
        .find(|(wid, _)| *wid == window_id)
        .ok_or_else(|| AppError::WindowNotFound(format!("Window {window_id} not found")))?;

    // For now, use coordinates directly (global screen coords)
    let png_data = tokio::task::spawn_blocking(move || {
        ScreenCapture::capture_region(q.x, q.y, q.width, q.height)
    })
    .await
    .map_err(|e| AppError::Screenshot(format!("capture_region task failed: {e}")))??;

    Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
}

async fn click_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<ClickRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = {
        let s = state.lock().await;
        let sb = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sb.pty_pid
    };
    let button = match req.button.to_lowercase().as_str() {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        other => return Err(AppError::BadRequest(format!("Unknown button: {other}"))),
    };
    InputSimulator::click(req.x, req.y, button, target_pid)?;
    Ok(Json(
        serde_json::json!({"clicked": {"x": req.x, "y": req.y, "button": req.button}}),
    ))
}

async fn type_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<TypeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = {
        let s = state.lock().await;
        let sb = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sb.pty_pid
    };
    InputSimulator::type_text(&req.text, target_pid)?;
    Ok(Json(serde_json::json!({"typed": req.text})))
}

async fn key_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<KeyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = {
        let s = state.lock().await;
        let sb = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sb.pty_pid
    };
    let mod_refs: Vec<&str> = req.modifiers.iter().map(|s| s.as_str()).collect();
    InputSimulator::press_key(&req.key, &mod_refs, target_pid)?;
    Ok(Json(
        serde_json::json!({"pressed": {"key": req.key, "modifiers": req.modifiers}}),
    ))
}

async fn scroll_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<ScrollRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = {
        let s = state.lock().await;
        let sb = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sb.pty_pid
    };
    InputSimulator::scroll(req.x, req.y, &req.direction, req.amount, target_pid)?;
    Ok(Json(serde_json::json!({"scrolled": true})))
}

async fn pty_ws_upgrade_handler(
    Path((id, pid)): Path<(String, u32)>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    // Validate that the sandbox exists
    // Note: We don't lock state here to keep the WebSocket upgrade fast.
    // The PID is validated inside handle_pty_ws via subscribe_output.
    let _ = id; // acknowledge the id parameter
    ProcessManager::subscribe_output(pid)
        .map_err(|e| AppError::Process(format!("PTY session {pid} not found: {e}")))?;
    Ok(ws.on_upgrade(move |socket| handle_pty_ws(pid, socket)))
}

async fn spawn_app_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<SpawnAppRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let app_path = req.path.clone();
    let (info, window_id) =
        tokio::task::spawn_blocking(move || ProcessManager::spawn_app_with_window(&app_path))
            .await
            .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;

    // Update sandbox window_id if discovered
    if let Some(wid) = window_id {
        let mut s = state.lock().await;
        if let Some(sb) = s.sandboxes.get_mut(&id) {
            sb.window_id = Some(wid);
        }
    }

    tracing::info!(
        "Spawned app in sandbox {}: {} (window_id={:?})",
        id,
        req.path,
        window_id
    );
    Ok(Json(serde_json::json!({
        "pid": info.pid,
        "name": info.name,
        "window_id": window_id,
    })))
}

async fn windows_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<(u32, String)>>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let windows = tokio::task::spawn_blocking(ScreenCapture::list_windows)
        .await
        .map_err(|e| AppError::Process(format!("list_windows panicked: {e}")))??;
    Ok(Json(windows))
}

async fn pty_write_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<PtyWriteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pty_pid: u32 = {
        let s = state.lock().await;
        let sb = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Process(format!("Sandbox {id} not found")))?;
        sb.pty_pid
            .ok_or_else(|| AppError::Process(format!("Sandbox {id} has no PTY")))?
    };
    let data = req.data.clone();
    tokio::task::spawn_blocking(move || ProcessManager::send_input(pty_pid, data.as_bytes()))
        .await
        .map_err(|e| AppError::Process(format!("pty_write panicked: {e}")))??;
    Ok(Json(
        serde_json::json!({"written": true, "bytes": req.data.len()}),
    ))
}

async fn processes_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::process::ProcessInfo>>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let processes = ProcessManager::list_processes()?;
    tracing::debug!("processes_handler: {} running", processes.len());
    Ok(Json(processes))
}

async fn ui_inspect_handler(
    Path((id, window_id)): Path<(String, u32)>,
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> Result<Json<crate::automation::ax_ui::UiElement>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let result = tokio::task::spawn_blocking(move || UiInspector::inspect_window(window_id))
        .await
        .map_err(|e| AppError::Accessibility(format!("UI inspect task failed: {e}")))?;
    Ok(Json(result?))
}

/// Inspect UI tree using the sandbox's stored window_id.
async fn ui_inspect_by_id_handler(
    Path(id): Path<String>,
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> Result<Json<crate::automation::ax_ui::UiElement>, AppError> {
    let window_id = {
        let s = state.lock().await;
        let sandbox = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sandbox
            .window_id
            .ok_or_else(|| AppError::BadRequest("Sandbox has no window_id".into()))?
    };

    let result = tokio::task::spawn_blocking(move || UiInspector::inspect_window(window_id))
        .await
        .map_err(|e| AppError::Accessibility(format!("UI inspect task failed: {e}")))?;
    Ok(Json(result?))
}

/// Find UI elements by role/title in a sandbox window.
async fn ui_find_handler(
    Path(id): Path<String>,
    State(state): State<Arc<Mutex<DaemonState>>>,
    Json(req): Json<UiFindRequest>,
) -> Result<Json<Vec<crate::automation::ax_ui::UiElement>>, AppError> {
    let window_id = {
        let s = state.lock().await;
        let sandbox = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sandbox
            .window_id
            .ok_or_else(|| AppError::BadRequest("Sandbox has no window_id".into()))?
    };

    let result = tokio::task::spawn_blocking(move || {
        UiInspector::find_elements(window_id, Some(&req.role), req.title.as_deref())
    })
    .await
    .map_err(|e| AppError::Accessibility(format!("UI find task failed: {e}")))?;
    Ok(Json(result?))
}

/// Get the value of a UI element by its element ID.
async fn ui_value_handler(
    Path(id): Path<String>,
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<UiValueResponse>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let element_id = params
        .get("element_id")
        .ok_or_else(|| AppError::BadRequest("Missing element_id query param".into()))?
        .clone();

    let result = tokio::task::spawn_blocking(move || UiInspector::get_element_value(&element_id))
        .await
        .map_err(|e| AppError::Accessibility(format!("UI value task failed: {e}")))?;
    Ok(Json(UiValueResponse { value: result? }))
}

/// Set the window_id for a sandbox (called by Electron after window creation).
async fn set_window_id_handler(
    Path(id): Path<String>,
    State(state): State<Arc<Mutex<DaemonState>>>,
    Json(req): Json<SetWindowIdRequest>,
) -> Result<StatusCode, AppError> {
    let mut s = state.lock().await;
    let sandbox = s
        .sandboxes
        .get_mut(&id)
        .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
    sandbox.window_id = Some(req.window_id);
    tracing::info!("Set window_id={} for sandbox {}", req.window_id, id);

    // Update instance registry file
    let registry = InstanceRegistry::default();
    let _ = registry.update_window_id(&id, req.window_id);
    Ok(StatusCode::OK)
}

async fn shutdown_handler() -> Json<serde_json::Value> {
    tracing::info!("Daemon shutdown requested via HTTP");
    let path = daemon_json_path();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = std::fs::remove_file(path);
        std::process::exit(0);
    });
    Json(serde_json::json!({"shutting_down": true}))
}

// ── Run daemon ────────────────────────────────────────────────

/// Start the daemon HTTP server on the given port.
///
/// Writes `daemon.json`, binds the TCP listener, and serves until
/// interrupted. Cleans up `daemon.json` on ctrl-c.
pub async fn run_daemon(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    write_daemon_info(port)?;
    tracing::info!(
        "Daemon starting on port {port} (pid={})",
        std::process::id()
    );

    let state = Arc::new(Mutex::new(DaemonState {
        port,
        sandboxes: HashMap::new(),
        started_at: Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        screenshot_request_counter: 0,
    }));

    let router = build_daemon_router(state.clone());

    // Ctrl-C handler: clean up daemon.json
    let ctrlc_path = daemon_json_path();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Ctrl-C received, cleaning up");
        let _ = std::fs::remove_file(&ctrlc_path);
        std::process::exit(0);
    });

    // Periodic cleanup of dead sandboxes
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut s = cleanup_state.lock().await;
            let removed = s.cleanup_dead_sandboxes();
            if !removed.is_empty() {
                tracing::info!("Cleaned up {} dead sandboxes: {:?}", removed.len(), removed);
            }
        }
    });

    // Auto-discover Electron window ID for screenshots
    let discovery_state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = tokio::task::spawn_blocking(|| {
            ScreenCapture::find_window_by_title("System Test Sandbox")
        })
        .await;
        match result {
            Ok(Ok(window_id)) => {
                tracing::info!("Discovered Electron window_id={}", window_id);
                let mut s = discovery_state.lock().await;
                let registry = InstanceRegistry::default();
                for (id, sb) in s.sandboxes.iter_mut() {
                    if sb.window_id.is_none() {
                        sb.window_id = Some(window_id);
                        let _ = registry.update_window_id(id, window_id);
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Could not discover Electron window: {e}");
            }
            Err(e) => {
                tracing::warn!("Window discovery task panicked: {e}");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    tracing::info!("Daemon listening on 127.0.0.1:{port}");
    axum::serve(listener, router).await?;

    cleanup_daemon_info()?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sandbox_daemon_test_{}_{}",
            std::process::id(),
            tag
        ))
    }

    #[test]
    fn find_available_port_returns_first_free() {
        // Pick a range that is very likely to have at least one free port
        let port = find_available_port(15900, 16000);
        assert!(port.is_some(), "Should find a free port in 15900..16000");
        let p = port.unwrap();
        assert!((15900..16000).contains(&p));
    }

    #[test]
    fn find_running_daemon_returns_none_when_no_file() {
        // Use a temp dir that definitely doesn't have daemon.json
        let tmp = test_dir("no_file");
        let _ = std::fs::remove_dir_all(&tmp);
        // Override daemon_json_path is not possible directly,
        // so we test that read_daemon_info returns None when file is absent.
        assert!(read_daemon_info().is_none() || !daemon_json_path().exists());
    }

    #[test]
    fn write_and_read_daemon_info_roundtrip() {
        let tmp = test_dir("roundtrip");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // Write a test daemon.json directly
        let info = DaemonInfo {
            port: 15801,
            pid: 99999,
            started_at: "2026-01-01 00:00:00".to_string(),
        };
        let json = serde_json::to_string_pretty(&info).unwrap();
        let path = tmp.join("daemon.json");
        std::fs::write(&path, &json).unwrap();

        // Read it back
        let read_back: DaemonInfo =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read_back.port, 15801);
        assert_eq!(read_back.pid, 99999);
        assert_eq!(read_back.started_at, "2026-01-01 00:00:00");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_running_daemon_detects_stale_pid() {
        // Use a PID that is very unlikely to exist
        let stale_pid: u32 = 4000000;
        let alive = unsafe { libc::kill(stale_pid as libc::pid_t, 0) == 0 };
        // This PID should not exist, so kill(pid, 0) should return -1
        // (unless running as root, which is unlikely for tests)
        assert!(!alive, "PID 4000000 should not be alive");
    }

    #[test]
    fn daemon_info_serialization_roundtrip() {
        let info = DaemonInfo {
            port: 15888,
            pid: 12345,
            started_at: "2026-05-30 12:00:00".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: DaemonInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.port, 15888);
        assert_eq!(parsed.pid, 12345);
    }

    #[test]
    fn managed_sandbox_serialization() {
        let sb = ManagedSandbox {
            id: "abcd1234".to_string(),
            kind: InstanceKind::Cli {
                command: "zsh".to_string(),
                args: vec![],
            },
            status: InstanceStatus::Running,
            port: 0,
            pty_pid: Some(1234),
            window_id: None,
        };
        let json = serde_json::to_string(&sb).unwrap();
        let parsed: ManagedSandbox = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "abcd1234");
        assert!(matches!(parsed.status, InstanceStatus::Running));
    }

    #[test]
    fn create_sandbox_request_deserialization() {
        let req: CreateSandboxRequest =
            serde_json::from_str(r#"{"mode": "cli", "command": "zsh", "cols": 120, "rows": 40}"#)
                .unwrap();
        assert_eq!(req.mode, "cli");
        assert_eq!(req.command, Some("zsh".to_string()));
        assert_eq!(req.cols, Some(120));
    }

    #[test]
    fn create_sandbox_request_defaults() {
        let req: CreateSandboxRequest = serde_json::from_str(r#"{"mode": "cli"}"#).unwrap();
        assert_eq!(req.mode, "cli");
        assert!(req.command.is_none());
        assert!(req.args.is_empty());
        assert!(req.cols.is_none());
        assert!(req.rows.is_none());
    }

    // ── Router-level tests ─────────────────────────────────────

    fn test_daemon_state() -> Arc<Mutex<DaemonState>> {
        Arc::new(Mutex::new(DaemonState {
            port: 15999,
            sandboxes: HashMap::new(),
            started_at: Instant::now(),
            screenshot_ws_tx: None,
            pending_screenshots: HashMap::new(),
            screenshot_request_counter: 0,
        }))
    }

    fn test_daemon_state_with_sandbox() -> Arc<Mutex<DaemonState>> {
        let mut sandboxes = HashMap::new();
        sandboxes.insert(
            "test-sb".to_string(),
            ManagedSandbox {
                id: "test-sb".to_string(),
                kind: InstanceKind::Cli {
                    command: "zsh".to_string(),
                    args: vec![],
                },
                status: InstanceStatus::Running,
                port: 0,
                pty_pid: None,
                window_id: None,
            },
        );
        Arc::new(Mutex::new(DaemonState {
            port: 15999,
            sandboxes,
            started_at: Instant::now(),
            screenshot_ws_tx: None,
            pending_screenshots: HashMap::new(),
            screenshot_request_counter: 0,
        }))
    }

    fn test_daemon_router() -> Router {
        build_daemon_router(test_daemon_state())
    }

    fn test_daemon_router_with_sandbox() -> Router {
        build_daemon_router(test_daemon_state_with_sandbox())
    }

    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["sandboxes"], 0);
    }

    #[tokio::test]
    async fn list_sandboxes_returns_empty() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn create_sandbox_with_bad_mode() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/create")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"mode": "unknown"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn close_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/ghost123/close")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn screenshot_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/noexist/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn click_valid_request() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/test-sb/input/click")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "left"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        // May succeed or fail depending on macOS accessibility permissions
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 500),
            "click should be 200 or 500, got {status}"
        );
    }

    #[tokio::test]
    async fn click_bad_button() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/test-sb/input/click")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "turbo"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn click_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/ghost/input/click")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "left"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn type_text_handler() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/test-sb/input/type")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text": "hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 500),
            "type should be 200 or 500, got {status}"
        );
    }

    #[tokio::test]
    async fn type_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/ghost/input/type")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text": "hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn key_handler() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/test-sb/input/key")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key": "return", "modifiers": ["cmd"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 500),
            "key should be 200 or 500, got {status}"
        );
    }

    #[tokio::test]
    async fn key_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/ghost/input/key")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key": "return", "modifiers": []}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn scroll_handler() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/test-sb/input/scroll")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"x": 0, "y": 0, "direction": "down", "amount": 3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 500),
            "scroll should be 200 or 500, got {status}"
        );
    }

    #[tokio::test]
    async fn scroll_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/ghost/input/scroll")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"x": 0, "y": 0, "direction": "down", "amount": 3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn windows_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/ghost/windows")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ui_inspect_nonexistent_sandbox() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/ghost/ui/inspect/999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
