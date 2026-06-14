//! Daemon module — manages multiple sandboxes via a single HTTP API.
//!
//! The daemon is a long-lived process that listens on a single port and routes
//! all sandbox operations through `/box/{id}/...` endpoints. It replaces the
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
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Discover the Electron window ID by title, falling back to PID-based lookup.
/// The Electron app may not always set a window title (e.g., with titleBarStyle: "hiddenInset").
fn find_electron_window() -> crate::error::Result<u32> {
    // Try by title first
    if let Ok(wid) = ScreenCapture::find_window_by_title("CLI Box") {
        return Ok(wid);
    }
    // Fallback: find by PID of the Electron main process
    let electron_pids: Vec<u32> = {
        let mut pids = Vec::new();
        if let Ok(output) = std::process::Command::new("pgrep")
            .args(["-f", "CLI Box.app/Contents/MacOS/CLI Box"])
            .output()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    pids.push(pid);
                }
            }
        }
        pids
    };
    for pid in electron_pids {
        if let Ok(wid) = ScreenCapture::find_window_by_pid(pid) {
            return Ok(wid);
        }
    }
    Err(AppError::WindowNotFound(
        "Electron window not found by title or PID".into(),
    ))
}
use tokio::sync::{oneshot, Mutex};
use tokio::time::interval;
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
    /// Sandboxes whose xterm.js terminal has been mounted and is ready.
    pub terminal_ready_sandboxes: HashSet<String>,
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

/// Daemon info persisted to `~/.cli-box/daemon.json`.
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

#[derive(Debug, Serialize)]
pub struct DaemonReadinessResponse {
    /// "ready" if renderer WebSocket is connected, "not_ready" otherwise.
    pub status: String,
    /// Whether the Electron renderer's screenshot WebSocket is connected.
    pub renderer_connected: bool,
    /// Whether the requested sandbox's terminal is ready (true if no sandbox_id requested).
    pub terminal_ready: bool,
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

/// Returns the path to `~/.cli-box/daemon.json`.
pub fn daemon_json_path() -> PathBuf {
    dirs_home().join(".cli-box").join("daemon.json")
}

/// Write daemon info to disk.
pub fn write_daemon_info(port: u16) -> std::io::Result<()> {
    let dir = dirs_home().join(".cli-box");
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
        .route("/readyz", get(readyz_handler))
        .route("/box/list", get(list_sandboxes_handler))
        .route("/box/create", post(create_sandbox_handler))
        .route("/box/{id}/close", post(close_sandbox_handler))
        .route("/box/{id}/screenshot", get(screenshot_handler))
        .route(
            "/box/{id}/screenshot/region",
            get(screenshot_region_handler),
        )
        .route("/box/{id}/input/click", post(click_handler))
        .route("/box/{id}/input/type", post(type_handler))
        .route("/box/{id}/input/key", post(key_handler))
        .route("/box/{id}/input/scroll", post(scroll_handler))
        .route("/box/{id}/pty/ws/{pid}", get(pty_ws_upgrade_handler))
        .route("/box/{id}/pty/write", post(pty_write_handler))
        .route("/box/{id}/processes", get(processes_handler))
        .route("/box/{id}/app/spawn", post(spawn_app_handler))
        .route("/box/{id}/windows", get(windows_handler))
        .route("/box/{id}/ui/inspect/{window_id}", get(ui_inspect_handler))
        .route("/box/{id}/ui/inspect", get(ui_inspect_by_id_handler))
        .route("/box/{id}/ui/find", post(ui_find_handler))
        .route("/box/{id}/ui/value", get(ui_value_handler))
        .route("/box/{id}/window", post(set_window_id_handler))
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

/// Daemon-level readiness: always returns 200 with readiness in JSON body.
/// Unlike the sandbox-level /readyz (server/mod.rs) which returns 503/200,
/// this is a polling endpoint — callers check `renderer_connected` in the response.
/// Optionally accepts `?sandbox_id=<id>` to check per-sandbox terminal readiness.
async fn readyz_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Json<DaemonReadinessResponse> {
    let s = state.lock().await;
    let renderer_connected = s.screenshot_ws_tx.is_some();
    let terminal_ready = match params.get("sandbox_id") {
        Some(sandbox_id) => s.terminal_ready_sandboxes.contains(sandbox_id.as_str()),
        None => true,
    };
    Json(DaemonReadinessResponse {
        status: if renderer_connected && terminal_ready {
            "ready"
        } else {
            "not_ready"
        }
        .to_string(),
        renderer_connected,
        terminal_ready,
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
            let window_id = find_electron_window().ok();

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

            let sandbox_id = id.clone();
            let (process_info, window_id) = tokio::task::spawn_blocking(move || {
                ProcessManager::spawn_app_with_window(&app_path, Some(&sandbox_id))
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

    // Clean up temporary Chromium user-data-dir if created
    crate::process::cleanup_chromium_data(&id);

    // Unregister from file-system registry
    let registry = InstanceRegistry::default();
    registry.unregister(&id)?;

    tracing::info!("Closed sandbox: id={}", id);
    Ok(Json(serde_json::json!({"closed": id})))
}

#[derive(Deserialize)]
struct ScreenshotQuery {
    #[serde(default)]
    with_frame: bool,
}

async fn screenshot_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<ScreenshotQuery>,
) -> Result<Response, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    if q.with_frame {
        // --with-frame: use ScreenCaptureKit directly, skip renderer
        return screenshot_with_frame(state, &id).await;
    }

    // Default: renderer only, no SCK fallback
    match request_renderer_screenshot(state.clone(), &id).await {
        Ok(png_data) => {
            tracing::info!(
                "[screenshot] sandbox {} captured via renderer ({} bytes)",
                id,
                png_data.len()
            );
            Ok(screenshot_response(png_data, "renderer", None))
        }
        Err(reason) => {
            tracing::warn!(
                "[screenshot] renderer capture failed for sandbox {}: {}",
                id,
                reason
            );
            Err(AppError::Screenshot(format!(
                "Screenshot failed: {}. Use --with-frame to capture via ScreenCaptureKit (requires Screen Recording permission).",
                reason
            )))
        }
    }
}

/// Build a screenshot HTTP response with source/fallback headers.
fn screenshot_response(png_data: Vec<u8>, source: &str, fallback_reason: Option<&str>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("image/png"),
    );
    headers.insert(
        "x-screenshot-source",
        HeaderValue::from_str(source).expect("valid header value"),
    );
    if let Some(reason) = fallback_reason {
        headers.insert(
            "x-screenshot-fallback-reason",
            HeaderValue::from_str(reason).expect("valid header value"),
        );
    }
    (StatusCode::OK, headers, png_data).into_response()
}

/// Capture a screenshot using ScreenCaptureKit (requires Screen Recording permission).
/// Handles stale window IDs by re-discovering the window by title.
async fn screenshot_with_frame(
    state: Arc<Mutex<DaemonState>>,
    id: &str,
) -> Result<Response, AppError> {
    // Switch to the target tab before capturing so SCK sees the correct content.
    if let Err(e) = request_switch_tab(state.clone(), id).await {
        tracing::warn!("Failed to switch tab for sandbox {}: {}", id, e);
    }

    let window_id = {
        let s = state.lock().await;
        s.sandboxes.get(id).and_then(|sb| sb.window_id)
    };

    // Try stored window_id first, handle stale IDs
    if let Some(wid) = window_id {
        let result = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(wid))
            .await
            .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))?;
        match result {
            Ok(png_data) => return Ok(screenshot_response(png_data, "screencapturekit", None)),
            Err(AppError::WindowNotFound(_)) => {
                tracing::warn!(
                    "Stored window_id={} for sandbox {} is stale, re-discovering",
                    wid,
                    id
                );
            }
            Err(AppError::Screenshot(msg))
                if msg.contains("permission") || msg.contains("denied") =>
            {
                return Err(AppError::Screenshot(format!(
                    "{}. Grant Screen Recording in System Settings → Privacy & Security → Screen Recording.",
                    msg
                )));
            }
            Err(e) => return Err(e),
        }
    }

    // Re-discover window by title or PID
    let new_wid = tokio::task::spawn_blocking(find_electron_window)
        .await
        .map_err(|e| AppError::Screenshot(format!("window discovery task failed: {e}")))??;

    {
        let mut s = state.lock().await;
        if let Some(sb) = s.sandboxes.get_mut(id) {
            sb.window_id = Some(new_wid);
        }
    }
    let registry = InstanceRegistry::default();
    let _ = registry.update_window_id(id, new_wid);
    tracing::info!("Re-discovered window_id={} for sandbox {}", new_wid, id);

    let png_data = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(new_wid))
        .await
        .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))??;
    Ok(screenshot_response(png_data, "screencapturekit", None))
}

// ── Screenshot WebSocket ────────────────────────────────────────

/// Request a screenshot from the Electron renderer via WebSocket.
/// Returns PNG bytes if successful, or a descriptive error string explaining why it failed.
async fn request_renderer_screenshot(
    state: Arc<Mutex<DaemonState>>,
    sandbox_id: &str,
) -> Result<Vec<u8>, String> {
    let (request_id, response_rx, mut ws_tx) = {
        let mut s = state.lock().await;
        let ws_tx = s
            .screenshot_ws_tx
            .take()
            .ok_or("WebSocket not connected (renderer may be closed or not yet connected)")?;

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
        let mut s = state.lock().await;
        s.pending_screenshots.remove(&request_id);
        s.screenshot_ws_tx = Some(ws_tx);
        return Err("Failed to send request over WebSocket (connection broken)".to_string());
    }

    // Put the ws_tx back
    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), response_rx).await {
        Ok(Ok(Ok(png_data))) => Ok(png_data),
        Ok(Ok(Err(e))) => Err(format!("Renderer returned error: {e}")),
        Ok(Err(_)) => Err("Response channel dropped (renderer may have disconnected)".to_string()),
        Err(_) => {
            let mut s = state.lock().await;
            s.pending_screenshots.remove(&request_id);
            Err("Renderer did not respond within 2s timeout".to_string())
        }
    }
}

/// Request the renderer to switch to a specific tab via WebSocket.
/// Waits for the renderer to acknowledge the tab switch.
async fn request_switch_tab(
    state: Arc<Mutex<DaemonState>>,
    sandbox_id: &str,
) -> Result<(), String> {
    let (request_id, response_rx, mut ws_tx) = {
        let mut s = state.lock().await;
        let ws_tx = s
            .screenshot_ws_tx
            .take()
            .ok_or("WebSocket not connected (renderer may be closed or not yet connected)")?;

        s.screenshot_request_counter += 1;
        let request_id = s.screenshot_request_counter;

        let (response_tx, response_rx) = oneshot::channel();
        s.pending_screenshots.insert(request_id, response_tx);

        (request_id, response_rx, ws_tx)
    };

    let msg = serde_json::json!({
        "type": "switch_tab_request",
        "request_id": request_id,
        "sandbox_id": sandbox_id,
    });

    if ws_tx
        .send(Message::Text(msg.to_string().into()))
        .await
        .is_err()
    {
        let mut s = state.lock().await;
        s.pending_screenshots.remove(&request_id);
        s.screenshot_ws_tx = Some(ws_tx);
        return Err("Failed to send switch_tab request over WebSocket".to_string());
    }

    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), response_rx).await {
        Ok(Ok(Ok(_))) => Ok(()),
        Ok(Ok(Err(e))) => Err(format!("Renderer returned error on switch_tab: {e}")),
        Ok(Err(_)) => Err("Response channel dropped during switch_tab".to_string()),
        Err(_) => {
            let mut s = state.lock().await;
            s.pending_screenshots.remove(&request_id);
            Err("Renderer did not respond to switch_tab within 2s timeout".to_string())
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

    let mut ping_interval = interval(std::time::Duration::from_secs(10));
    // Skip the first immediate tick
    ping_interval.tick().await;

    loop {
        tokio::select! {
            // Incoming message from renderer
            result = ws_rx.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
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
                                    Some("switch_tab_response") => {
                                        if let Some(req_id) = request_id {
                                            let mut s = state.lock().await;
                                            if let Some(tx) = s.pending_screenshots.remove(&req_id) {
                                                let _ = tx.send(Ok(vec![]));
                                            }
                                        }
                                    }
                                    Some("terminal_ready") => {
                                        if let Some(sandbox_id) = msg.get("sandbox_id").and_then(|v| v.as_str()) {
                                            let mut s = state.lock().await;
                                            s.terminal_ready_sandboxes.insert(sandbox_id.to_string());
                                            tracing::info!(
                                                "[screenshot_ws] terminal ready: {}",
                                                sandbox_id
                                            );
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
                    Some(Ok(Message::Pong(_))) => {
                        // Pong received — connection is alive
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("[screenshot_ws] renderer sent close frame");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("[screenshot_ws] receive error: {e}");
                        break;
                    }
                    None => {
                        tracing::info!("[screenshot_ws] renderer stream ended");
                        break;
                    }
                    _ => {}
                }
            }
            // Periodic ping to keep connection alive
            _ = ping_interval.tick() => {
                let ping_sent = {
                    let mut s = state.lock().await;
                    if let Some(ref mut tx) = s.screenshot_ws_tx {
                        tx.send(Message::Ping(vec![].into())).await.is_ok()
                    } else {
                        false
                    }
                };
                if !ping_sent {
                    tracing::warn!("[screenshot_ws] ping failed, renderer may have disconnected");
                    break;
                }
            }
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
                let new_wid =
                    tokio::task::spawn_blocking(find_electron_window)
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
    let sandbox_id = id.clone();
    let (info, window_id) = tokio::task::spawn_blocking(move || {
        ProcessManager::spawn_app_with_window(&app_path, Some(&sandbox_id))
    })
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
) -> Result<Json<serde_json::Value>, AppError> {
    let (kind, pty_pid, window_id) = {
        let s = state.lock().await;
        let sandbox = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        (sandbox.kind.clone(), sandbox.pty_pid, sandbox.window_id)
    };

    match kind {
        InstanceKind::Cli { .. } => {
            // CLI sandbox: return PTY output as markdown
            let pty_pid = pty_pid
                .ok_or_else(|| AppError::BadRequest("CLI sandbox has no PTY process".into()))?;
            let text = ProcessManager::read_output(pty_pid)
                .map_err(|e| AppError::Process(format!("Failed to read PTY output: {e}")))?
                .unwrap_or_default();
            // Strip ANSI escape sequences and TUI artifacts for clean text output
            let clean = strip_ansi_escapes(&text);
            let readable = extract_readable_text(&clean);
            // Format as markdown with line breaks
            let markdown = format_terminal_markdown(&readable, text.len());
            Ok(Json(serde_json::json!({
                "type": "terminal",
                "content": clean,
                "readable": readable,
                "markdown": markdown,
                "raw_length": text.len(),
            })))
        }
        InstanceKind::App { .. } => {
            // App sandbox: use AX UI inspection
            let window_id =
                window_id.ok_or_else(|| AppError::BadRequest("Sandbox has no window_id".into()))?;
            let result =
                tokio::task::spawn_blocking(move || UiInspector::inspect_window(window_id))
                    .await
                    .map_err(|e| AppError::Accessibility(format!("UI inspect task failed: {e}")))?;
            Ok(Json(serde_json::to_value(result?)?))
        }
    }
}

/// Strip ANSI escape sequences from text.
fn strip_ansi_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() || next == 'm' {
                        chars.next(); // consume terminator
                        break;
                    }
                    chars.next(); // consume parameter byte
                }
            } else if chars.peek() == Some(&']') {
                // OSC sequence (e.g., title set) — skip until ST or BEL
                chars.next(); // consume ']'
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next(); // consume backslash (ST = ESC \)
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Extract readable text from terminal output by removing TUI box-drawing
/// characters and cleaning up whitespace.
fn extract_readable_text(text: &str) -> String {
    // Box-drawing and TUI characters to remove
    let tui_chars: &[char] = &[
        // Box-drawing characters
        '┌', '┐', '└', '┘', '─', '│', '├', '┤', '┬', '┴', '┼',
        '╔', '╗', '╚', '╝', '═', '║', '╠', '╣', '╦', '╩', '╬',
        '┃', '━', '┏', '┓', '┗', '┛', '┣', '┫', '┳', '┻', '╋',
        '╸', '╹', '╺', '╻',
        // Block elements (used for progress bars)
        '▄', '▀', '▐', '▌', '█', '░', '▒', '▓',
        '■', '□', '▢', '▣', '▤', '▥', '▦', '▧', '▨', '▩',
        '⬝', '⬚', '⬒', '⬓', '⬔', '⬕', '⬖', '⬗', '⬘', '⬙',
        // Braille characters (used for spinners/progress)
        '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏',
        '⠿', '⠾', '⠽', '⠼', '⠻', '⠺', '⠹', '⠸', '⠷', '⠶',
        '⠵', '⠴', '⠳', '⠲', '⠱', '⠰', '⠯', '⠮', '⠭', '⠬',
        '⠫', '⠪', '⠩', '⠨', '⠧', '⠦', '⠥', '⠤', '⠣', '⠢',
        '⠡', '⠠', '⠟', '⠞', '⠝', '⠜', '⠛', '⠚', '⠙', '⠘',
        '⠗', '⠖', '⠕', '⠔', '⠓', '⠒', '⠑', '⠐', '⠏', '⠎',
        '⠍', '⠌', '⠋', '⠊', '⠉', '⠈', '⠇', '⠆', '⠅', '⠄',
        '⠃', '⠂', '⠁', '⠀',
        // Geometric shapes (used for progress indicators)
        '◉', '◎', '○', '●', '◐', '◑', '◒', '◓', '◔', '◕',
        '◖', '◗', '◘', '◙', '◚', '◛', '◜', '◝', '◞', '◟',
        '◠', '◡', '◢', '◣', '◤', '◥', '◦', '◧', '◨', '◩',
        '◪', '◫', '◬', '◭', '◮', '◯', '◌', '△', '▽', '▲', '▼',
        '▴', '▾', '◂', '▸', '◆', '◇', '◈', '◉', '◊', '○',
    ];

    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();

    for line in lines {
        // Remove TUI box-drawing characters
        let cleaned: String = line
            .chars()
            .filter(|c| !tui_chars.contains(c))
            .collect();

        // Trim whitespace
        let trimmed = cleaned.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip lines that are mostly spaces (TUI padding)
        let non_space_count = trimmed.chars().filter(|c| !c.is_whitespace()).count();
        if non_space_count < 3 {
            continue;
        }

        result.push(trimmed.to_string());
    }

    result.join("\n")
}

/// Format terminal output as markdown with proper line breaks and structure.
fn format_terminal_markdown(readable: &str, raw_length: usize) -> String {
    let mut md = String::new();

    // Header
    md.push_str("# Terminal Content\n\n");

    // Split by lines and format each line
    let lines: Vec<&str> = readable.lines().collect();
    let non_empty_lines: Vec<&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();

    if non_empty_lines.is_empty() {
        md.push_str("*No content captured*\n");
    } else {
        for line in &non_empty_lines {
            let trimmed = line.trim();

            // Split long lines at logical points for better readability
            let formatted = split_long_line(trimmed);

            // Detect prompt lines (ending with $ or %)
            if trimmed.ends_with('$') || trimmed.ends_with('%') {
                md.push_str(&format!("```\n{}\n```\n\n", formatted));
            }
            // Detect error/warning lines
            else if trimmed.starts_with("Error:")
                || trimmed.starts_with("Warning:")
                || trimmed.starts_with("INFO:")
                || trimmed.starts_with("DEBUG:")
            {
                md.push_str(&format!("> {}\n\n", formatted));
            }
            // Regular content
            else {
                md.push_str(&format!("{}\n\n", formatted));
            }
        }
    }

    // Footer with stats
    md.push_str(&format!("---\n*Raw length: {} bytes*\n", raw_length));

    md
}

/// Split long lines at logical points for better readability.
/// Looks for patterns like multiple spaces (column separators) or
/// specific TUI patterns to add line breaks.
fn split_long_line(line: &str) -> String {
    // If line is short enough, return as-is
    if line.len() < 120 {
        return line.to_string();
    }

    // Try to split at multiple consecutive spaces (TUI column separators)
    let parts: Vec<&str> = line.split("   ").collect();
    if parts.len() > 2 {
        // Join with newlines, trimming each part
        let cleaned: Vec<String> = parts
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if cleaned.len() > 1 {
            return cleaned.join("\n");
        }
    }

    // If no good split points, return as-is
    line.to_string()
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
        terminal_ready_sandboxes: HashSet::new(),
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

        let result = tokio::task::spawn_blocking(find_electron_window).await;

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
        // If a real daemon is running, skip this test — it tests the "no daemon" case.
        if let Some(_port) = find_running_daemon() {
            return;
        }
        // After find_running_daemon, stale daemon.json is cleaned up.
        // With no daemon running, daemon_json_path should not exist (or be cleaned up).
        assert!(
            read_daemon_info().is_none(),
            "Expected no daemon info after cleanup"
        );
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
            terminal_ready_sandboxes: HashSet::new(),
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
            terminal_ready_sandboxes: HashSet::new(),
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
                    .uri("/box/list")
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
                    .uri("/box/create")
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
                    .uri("/box/ghost123/close")
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
                    .uri("/box/noexist/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn screenshot_with_frame_attempts_sck() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/box/test-sb/screenshot?with_frame=true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // with_frame=true routes to SCK path. Without a real window, it fails
        // with either 404 (WindowNotFound with SCK) or 500 (Screenshot error).
        // Either way, it must NOT be 400 (Bad Request) — proves query param is parsed.
        let status = resp.status();
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "with_frame=true should be parsed, not rejected as bad request"
        );
    }

    #[tokio::test]
    async fn screenshot_without_frame_fails_when_renderer_unavailable() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/box/test-sb/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Default path (no with_frame) tries renderer only, should fail with 500
        // since no renderer WebSocket is connected in tests
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn click_valid_request() {
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/box/test-sb/input/click")
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
                    .uri("/box/test-sb/input/click")
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
                    .uri("/box/ghost/input/click")
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
                    .uri("/box/test-sb/input/type")
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
                    .uri("/box/ghost/input/type")
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
                    .uri("/box/test-sb/input/key")
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
                    .uri("/box/ghost/input/key")
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
                    .uri("/box/test-sb/input/scroll")
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
                    .uri("/box/ghost/input/scroll")
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
                    .uri("/box/ghost/windows")
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
                    .uri("/box/ghost/ui/inspect/999")
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

    #[tokio::test]
    async fn request_renderer_screenshot_returns_error_when_ws_not_connected() {
        let state = test_daemon_state_with_sandbox();
        let result = request_renderer_screenshot(state, "test-sb").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("WebSocket not connected"),
            "Expected 'WebSocket not connected', got: {err}"
        );
    }

    #[test]
    fn screenshot_response_has_renderer_source() {
        let resp = screenshot_response(vec![0x89, 0x50], "renderer", None);
        let headers = resp.headers();
        assert_eq!(headers.get("x-screenshot-source").unwrap(), "renderer");
        assert!(headers.get("x-screenshot-fallback-reason").is_none());
        assert_eq!(headers.get("content-type").unwrap(), "image/png");
    }

    #[test]
    fn screenshot_response_has_fallback_source_and_reason() {
        let resp = screenshot_response(
            vec![0x89, 0x50],
            "screencapturekit",
            Some("renderer_unavailable"),
        );
        let headers = resp.headers();
        assert_eq!(
            headers.get("x-screenshot-source").unwrap(),
            "screencapturekit"
        );
        assert_eq!(
            headers.get("x-screenshot-fallback-reason").unwrap(),
            "renderer_unavailable"
        );
    }

    #[tokio::test]
    async fn request_switch_tab_returns_error_when_ws_not_connected() {
        let state = test_daemon_state_with_sandbox();
        let result = request_switch_tab(state, "test-sb").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("WebSocket not connected"),
            "Expected 'WebSocket not connected', got: {err}"
        );
    }

    #[tokio::test]
    async fn screenshot_with_frame_logs_warning_on_switch_tab_failure() {
        // screenshot_with_frame calls request_switch_tab first, which will fail
        // (no WebSocket), then proceeds to SCK capture. The function should not
        // return 400 (Bad Request) — the tab switch failure should be a warning.
        let app = test_daemon_router_with_sandbox();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/box/test-sb/screenshot?with_frame=true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // SCK capture also fails (no real window) → 404 or 500, but NOT 400.
        let status = resp.status();
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "switch_tab failure should not cause bad request, got {status}"
        );
    }
}
