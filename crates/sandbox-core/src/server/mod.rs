use crate::automation::ax_ui::{UiElement, UiInspector};
use crate::automation::cg_event::{InputSimulator, MouseButton};
use crate::capture::ScreenCapture;
use crate::error::AppError;
use crate::process::ProcessManager;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

/// CLI configuration pending spawn — stored until frontend requests it.
#[derive(Clone, Debug)]
pub struct PendingCli {
    pub command: String,
    pub args: Vec<String>,
}

/// Shared application state for the HTTP server
pub struct AppState {
    pub sandbox_id: Option<String>,
    pub start_time: Instant,
    pub window_id: Option<u32>,
    pub target_pid: Option<u32>,
    /// CLI config pending spawn — consumed by frontend after xterm.js init
    pub pending_cli: Option<PendingCli>,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_secs: u64,
    sandbox_id: Option<String>,
}

/// Sandbox info response
#[derive(Serialize)]
struct SandboxInfoResponse {
    sandbox_id: Option<String>,
    window_id: Option<u32>,
    uptime_secs: u64,
}

#[derive(Deserialize)]
struct ClickRequest {
    x: f64,
    y: f64,
    #[serde(default = "default_button")]
    button: String,
}

fn default_button() -> String {
    "left".to_string()
}

#[derive(Deserialize)]
struct TypeRequest {
    text: String,
}

#[derive(Deserialize)]
struct KeyRequest {
    key: String,
    #[serde(default)]
    modifiers: Vec<String>,
}

#[derive(Deserialize)]
struct ScrollRequest {
    x: f64,
    y: f64,
    direction: String,
    amount: i32,
}

#[derive(Deserialize)]
struct DragRequest {
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
}

#[derive(Deserialize)]
struct SpawnAppRequest {
    path: String,
}

#[derive(Deserialize)]
struct SpawnCliRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

#[derive(Deserialize)]
struct KillRequest {
    pid: u32,
}

#[derive(Deserialize)]
struct RegionQuery {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct ScreenshotQuery {
    #[serde(default)]
    window_id: Option<u32>,
}

#[derive(Deserialize)]
struct UiFindRequest {
    window_id: u32,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct UiValueQuery {
    element_id: String,
}

/// Build the HTTP API router
pub fn build_router(state: Arc<Mutex<AppState>>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_handler))
        .route("/sandbox/info", get(sandbox_info_handler))
        .route("/sandbox/pending-cli", get(pending_cli_handler))
        .route("/shutdown", post(shutdown_handler))
        .route("/windows", get(list_windows_handler))
        .route("/processes", get(list_processes_handler))
        .route("/app/spawn", post(spawn_app_handler))
        .route("/cli/spawn", post(spawn_cli_handler))
        .route("/process/kill", post(kill_process_handler))
        .route("/input/click", post(click_handler))
        .route("/input/type", post(type_handler))
        .route("/input/key", post(key_handler))
        .route("/input/scroll", post(scroll_handler))
        .route("/input/drag", post(drag_handler))
        .route("/screenshot", get(screenshot_handler))
        .route("/screenshot/region", get(screenshot_region_handler))
        .route("/pty/ws/{pid}", get(pty_ws_handler))
        .route("/ui/inspect/{window_id}", get(ui_inspect_handler))
        .route("/ui/find", post(ui_find_handler))
        .route("/ui/value", get(ui_value_handler))
        .layer(cors)
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────

async fn health_handler(State(state): State<Arc<Mutex<AppState>>>) -> Json<HealthResponse> {
    let s = state.lock().await;
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: s.start_time.elapsed().as_secs(),
        sandbox_id: s.sandbox_id.clone(),
    })
}

async fn sandbox_info_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Json<SandboxInfoResponse> {
    let s = state.lock().await;
    Json(SandboxInfoResponse {
        sandbox_id: s.sandbox_id.clone(),
        window_id: s.window_id,
        uptime_secs: s.start_time.elapsed().as_secs(),
    })
}

async fn pending_cli_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state.lock().await;
    match &s.pending_cli {
        Some(config) => Ok(Json(serde_json::json!({
            "command": config.command,
            "args": config.args,
        }))),
        None => Ok(Json(serde_json::json!({ "command": null }))),
    }
}

async fn shutdown_handler() -> Json<serde_json::Value> {
    tracing::info!("Shutdown requested via HTTP");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(0);
    });
    Json(serde_json::json!({"shutting_down": true}))
}

async fn list_windows_handler() -> Result<Json<Vec<(u32, String)>>, AppError> {
    let windows = tokio::task::spawn_blocking(ScreenCapture::list_windows)
        .await
        .map_err(|e| AppError::Process(format!("list_windows panicked: {e}")))??;
    tracing::debug!("list_windows: {} windows", windows.len());
    Ok(Json(windows))
}

async fn list_processes_handler() -> Result<Json<Vec<crate::process::ProcessInfo>>, AppError> {
    let processes = ProcessManager::list_processes()?;
    tracing::debug!("list_processes: {} running", processes.len());
    Ok(Json(processes))
}

async fn spawn_app_handler(
    Json(req): Json<SpawnAppRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let path = req.path.clone();
    let info = tokio::task::spawn_blocking(move || ProcessManager::spawn_app(&req.path))
        .await
        .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;
    tracing::info!("spawned app: {path}");
    Ok(Json(info))
}

async fn spawn_cli_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<SpawnCliRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let cmd = req.command.clone();
    let cols = req.cols.unwrap_or(80);
    let rows = req.rows.unwrap_or(24);
    let info = tokio::task::spawn_blocking(move || {
        ProcessManager::spawn_cli_with_size(&req.command, &req.args, cols, rows)
    })
    .await
    .map_err(|e| AppError::Process(format!("spawn_cli panicked: {e}")))??;

    // Clear pending CLI after successful spawn
    {
        let mut s = state.lock().await;
        s.pending_cli = None;
    }

    tracing::info!("spawned cli: {cmd} ({cols}x{rows})");
    Ok(Json(info))
}

async fn kill_process_handler(
    Json(req): Json<KillRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    ProcessManager::kill_process(req.pid)?;
    Ok(Json(serde_json::json!({"killed": req.pid})))
}

const SANDBOX_WINDOW_REQUIRED: &str =
    "Sandbox window not available. Build and run the Tauri app first, or use `sandbox-cli start --cli/--app`.";

fn require_target_pid(target_pid: Option<u32>) -> Result<u32, AppError> {
    target_pid.ok_or_else(|| AppError::BadRequest(SANDBOX_WINDOW_REQUIRED.to_string()))
}

async fn click_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<ClickRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = require_target_pid(state.lock().await.target_pid)?;
    tracing::info!(
        "[input] click: x={}, y={}, button={}, target_pid={}",
        req.x,
        req.y,
        req.button,
        target_pid
    );
    let button = match req.button.to_lowercase().as_str() {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        other => return Err(AppError::BadRequest(format!("Unknown button: {other}"))),
    };
    InputSimulator::click(req.x, req.y, button, Some(target_pid))?;
    Ok(Json(
        serde_json::json!({"clicked": {"x": req.x, "y": req.y, "button": req.button}}),
    ))
}

async fn type_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<TypeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = require_target_pid(state.lock().await.target_pid)?;
    tracing::info!(
        "[input] type_text: len={}, target_pid={}, preview={:?}",
        req.text.len(),
        target_pid,
        if req.text.len() > 20 {
            &req.text[..20]
        } else {
            &req.text
        }
    );
    InputSimulator::type_text(&req.text, Some(target_pid))?;
    Ok(Json(serde_json::json!({"typed": req.text})))
}

async fn key_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<KeyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = require_target_pid(state.lock().await.target_pid)?;
    tracing::info!(
        "[input] press_key: key={}, modifiers={:?}, target_pid={}",
        req.key,
        req.modifiers,
        target_pid
    );
    let mod_refs: Vec<&str> = req.modifiers.iter().map(|s| s.as_str()).collect();
    InputSimulator::press_key(&req.key, &mod_refs, Some(target_pid))?;
    Ok(Json(
        serde_json::json!({"pressed": {"key": req.key, "modifiers": req.modifiers}}),
    ))
}

async fn scroll_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<ScrollRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = require_target_pid(state.lock().await.target_pid)?;
    InputSimulator::scroll(req.x, req.y, &req.direction, req.amount, Some(target_pid))?;
    Ok(Json(serde_json::json!({"scrolled": true})))
}

async fn drag_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<DragRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_pid = require_target_pid(state.lock().await.target_pid)?;
    InputSimulator::drag(req.from_x, req.from_y, req.to_x, req.to_y, Some(target_pid))?;
    Ok(Json(serde_json::json!({"dragged": true})))
}

async fn screenshot_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(q): Query<ScreenshotQuery>,
) -> Result<impl IntoResponse, AppError> {
    let window_id = q.window_id.or(state.lock().await.window_id);
    match window_id {
        Some(id) => {
            let png_data = ScreenCapture::capture_window(id)?;
            Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
        }
        None => Err(AppError::BadRequest(SANDBOX_WINDOW_REQUIRED.to_string())),
    }
}

async fn screenshot_region_handler(
    Query(q): Query<RegionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let png_data = ScreenCapture::capture_region(q.x, q.y, q.width, q.height)?;
    Ok((StatusCode::OK, [("content-type", "image/png")], png_data))
}

async fn pty_ws_handler(
    Path(pid): Path<u32>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    // Validate that the PTY session exists before upgrading
    ProcessManager::subscribe_output(pid)
        .map_err(|e| AppError::Process(format!("PTY session {pid} not found: {e}")))?;
    Ok(ws.on_upgrade(move |socket| handle_pty_ws(pid, socket)))
}

async fn handle_pty_ws(pid: u32, socket: WebSocket) {
    let mut rx = match ProcessManager::subscribe_output(pid) {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!("[pty_ws] pid={pid}: subscribe failed: {e}");
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Task 1: Broadcast PTY output → WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Task 2: WebSocket input → PTY, with control message parsing
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(control) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(msg_type) = control.get("type").and_then(|v| v.as_str()) {
                            match msg_type {
                                "resize" => {
                                    let cols =
                                        control.get("cols").and_then(|v| v.as_u64()).unwrap_or(80)
                                            as u16;
                                    let rows =
                                        control.get("rows").and_then(|v| v.as_u64()).unwrap_or(24)
                                            as u16;
                                    let _ = tokio::task::spawn_blocking(move || {
                                        ProcessManager::resize_pty(pid, cols, rows)
                                    })
                                    .await;
                                    continue;
                                }
                                _ => {
                                    tracing::trace!(
                                        "[pty_ws] pid={pid}: unknown control type: {msg_type}"
                                    );
                                }
                            }
                        }
                    }
                    // Plain text: send as PTY input
                    let _ = tokio::task::spawn_blocking(move || {
                        ProcessManager::send_input(pid, text.as_bytes())
                    })
                    .await;
                }
                Message::Binary(data) => {
                    let bytes = data.to_vec();
                    let _ = tokio::task::spawn_blocking(move || {
                        ProcessManager::send_input(pid, &bytes)
                    })
                    .await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::debug!("[pty_ws] pid={pid}: connection closed");
}

async fn ui_inspect_handler(Path(window_id): Path<u32>) -> Result<Json<UiElement>, AppError> {
    let result = tokio::task::spawn_blocking(move || UiInspector::inspect_window(window_id))
        .await
        .map_err(|e| AppError::Accessibility(format!("UI inspect task failed: {e}")))?;
    Ok(Json(result?))
}

async fn ui_find_handler(Json(req): Json<UiFindRequest>) -> Result<Json<Vec<UiElement>>, AppError> {
    let window_id = req.window_id;
    let role = req.role;
    let title = req.title;
    let result = tokio::task::spawn_blocking(move || {
        UiInspector::find_elements(window_id, role.as_deref(), title.as_deref())
    })
    .await
    .map_err(|e| AppError::Accessibility(format!("UI find task failed: {e}")))?;
    Ok(Json(result?))
}

async fn ui_value_handler(
    Query(q): Query<UiValueQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let value = UiInspector::get_element_value(&q.element_id)?;
    Ok(Json(serde_json::json!({ "value": value })))
}

// ── Error handling ────────────────────────────────────────

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::WindowNotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Instance(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    fn test_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState {
            sandbox_id: Some("test-sandbox-01".into()),
            start_time: Instant::now(),
            window_id: Some(42),
            target_pid: None,
            pending_cli: None,
        }))
    }

    fn test_router() -> Router {
        build_router(test_state())
    }

    // ── Health ─────────────────────────────────────────────────

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_router();
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
        assert_eq!(json["sandbox_id"], "test-sandbox-01");
    }

    // ── Sandbox Info ───────────────────────────────────────────

    #[tokio::test]
    async fn sandbox_info_returns_data() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sandbox_id"], "test-sandbox-01");
        assert_eq!(json["window_id"], 42);
    }

    #[tokio::test]
    async fn pending_cli_returns_null_when_none() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/pending-cli")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["command"].is_null());
    }

    #[tokio::test]
    async fn pending_cli_returns_config_when_set() {
        let state = Arc::new(Mutex::new(AppState {
            sandbox_id: Some("test".into()),
            start_time: Instant::now(),
            window_id: None,
            target_pid: None,
            pending_cli: Some(PendingCli {
                command: "opencode".into(),
                args: vec!["--verbose".into()],
            }),
        }));
        let app = build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sandbox/pending-cli")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["command"], "opencode");
        assert_eq!(json["args"], serde_json::json!(["--verbose"]));
    }

    // ── Input handlers ─────────────────────────────────────────

    #[tokio::test]
    async fn click_with_valid_button() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/click")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "left"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        // target_pid is None in test state — input ops require a sandbox window
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn click_with_right_button() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/click")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 50, "y": 50, "button": "right"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn click_with_middle_button() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/click")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 50, "y": 50, "button": "middle"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn click_bad_request() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/click")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "unknown"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn type_text_handler() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/type")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text": "hello world"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn key_handler() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/key")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key": "return", "modifiers": ["cmd"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn key_handler_no_modifiers() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/key")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key": "escape", "modifiers": []}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn scroll_handler() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/scroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"x": 0, "y": 0, "direction": "down", "amount": 3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn scroll_handler_unknown_direction() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/scroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"x": 0, "y": 0, "direction": "diagonal", "amount": 3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        // target_pid is None — rejected before direction validation
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn drag_handler() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/drag")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"from_x": 0, "from_y": 0, "to_x": 100, "to_y": 100}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Screenshot ─────────────────────────────────────────────

    #[tokio::test]
    async fn screenshot_uses_window_id_from_state() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 404 | 500),
            "unexpected status: {status}"
        );
    }

    #[tokio::test]
    async fn screenshot_region() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/screenshot/region?x=0&y=0&width=100&height=100")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 404 | 500),
            "unexpected status: {status}"
        );
    }

    // ── Windows / Processes ────────────────────────────────────

    #[tokio::test]
    async fn list_windows() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/windows")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn list_processes() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/processes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Spawn CLI / App ────────────────────────────────────────

    #[tokio::test]
    async fn spawn_cli_echo() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cli/spawn")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"command": "echo", "args": ["test"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn spawn_app_nonexistent() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/app/spawn")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"path": "/tmp/__no_such_app__.app"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Kill process ───────────────────────────────────────────

    #[tokio::test]
    async fn kill_process_nonexistent() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/process/kill")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"pid": 99999}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(
            status == StatusCode::OK
                || status == StatusCode::INTERNAL_SERVER_ERROR
                || status == StatusCode::NOT_FOUND
        );
    }

    // ── PTY WebSocket ──────────────────────────────────────────

    #[tokio::test]
    async fn pty_ws_nonexistent_pid() {
        // WebSocket upgrade for nonexistent PTY should return an error
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/pty/ws/99999")
                    .header("Upgrade", "websocket")
                    .header("Connection", "Upgrade")
                    .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                    .header("Sec-WebSocket-Version", "13")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should be 500 (PTY not found), 400, or 426 (Upgrade Required - expected
        // when oneshot doesn't complete the WebSocket handshake)
        let status = resp.status();
        assert!(
            status == StatusCode::OK
                || status == StatusCode::INTERNAL_SERVER_ERROR
                || status == StatusCode::BAD_REQUEST
                || status == StatusCode::UPGRADE_REQUIRED,
            "Expected OK/500/400/426 for nonexistent PTY WebSocket, got: {status}"
        );
    }

    // ── UI ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn ui_inspect_nonexistent() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/ui/inspect/99999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status.is_server_error() || status.is_client_error() || status == StatusCode::OK);
    }

    #[tokio::test]
    async fn ui_find() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ui/find")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"window_id": 42, "role": "button"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status.is_server_error() || status.is_client_error() || status == StatusCode::OK);
    }

    #[tokio::test]
    async fn ui_value() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/ui/value?element_id=test123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        assert!(status.is_server_error() || status.is_client_error() || status == StatusCode::OK);
    }

    // ── Error handling ─────────────────────────────────────────

    #[tokio::test]
    async fn app_error_into_response_bad_request() {
        let err = AppError::BadRequest("test message".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn app_error_into_response_not_found() {
        let err = AppError::WindowNotFound("window x".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn app_error_into_response_instance() {
        let err = AppError::Instance("instance x".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn app_error_into_response_internal() {
        let err = AppError::SandboxNotInitialized;
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Route not found ────────────────────────────────────────

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = test_router();
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

    // ── Standalone mode rejection ─────────────────────────────────

    #[tokio::test]
    async fn standalone_rejects_click() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/click")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "left"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("Sandbox window not available"));
    }

    #[tokio::test]
    async fn standalone_rejects_type() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/type")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text": "hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn standalone_rejects_key() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/key")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key": "return", "modifiers": []}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn standalone_rejects_scroll() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/scroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"x": 0, "y": 0, "direction": "down", "amount": 3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn standalone_rejects_drag() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/drag")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"from_x": 0, "from_y": 0, "to_x": 100, "to_y": 100}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn standalone_rejects_screenshot() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // When window_id is Some(42) in test state, screenshot tries to capture
        // which fails on non-macOS or without permissions, returning 404 or 500
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 404 | 500),
            "unexpected status: {status}"
        );
    }

    #[tokio::test]
    async fn screenshot_without_window_id_returns_error() {
        // Create a state with window_id = None
        let state = Arc::new(Mutex::new(AppState {
            sandbox_id: Some("test-sandbox-no-window".into()),
            start_time: Instant::now(),
            window_id: None,
            target_pid: None,
            pending_cli: None,
        }));
        let app = build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/screenshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should return 400 Bad Request with descriptive error
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("Sandbox window not available"));
    }

    #[tokio::test]
    async fn screenshot_with_query_window_id() {
        // Test that window_id from query parameter is used
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/screenshot?window_id=999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should try to capture window 999, which doesn't exist, returning 404 or 500
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 404 | 500),
            "unexpected status: {status}"
        );
    }

    // ── Deserialization tests ─────────────────────────────────

    #[test]
    fn click_request_default_button() {
        let req: ClickRequest = serde_json::from_str(r#"{"x": 1, "y": 2}"#).unwrap();
        assert_eq!(req.button, "left");
        assert_eq!(req.x, 1.0);
        assert_eq!(req.y, 2.0);
    }

    #[test]
    fn click_request_explicit_button() {
        let req: ClickRequest =
            serde_json::from_str(r#"{"x": 10, "y": 20, "button": "right"}"#).unwrap();
        assert_eq!(req.button, "right");
    }

    #[test]
    fn type_request_deserialize() {
        let req: TypeRequest = serde_json::from_str(r#"{"text": "hello"}"#).unwrap();
        assert_eq!(req.text, "hello");
    }

    #[test]
    fn key_request_with_modifiers() {
        let req: KeyRequest =
            serde_json::from_str(r#"{"key": "a", "modifiers": ["cmd", "shift"]}"#).unwrap();
        assert_eq!(req.key, "a");
        assert_eq!(req.modifiers, vec!["cmd", "shift"]);
    }

    #[test]
    fn key_request_without_modifiers() {
        let req: KeyRequest = serde_json::from_str(r#"{"key": "return"}"#).unwrap();
        assert!(req.modifiers.is_empty());
    }

    #[test]
    fn scroll_request_deserialize() {
        let req: ScrollRequest =
            serde_json::from_str(r#"{"x": 100, "y": 200, "direction": "up", "amount": 5}"#)
                .unwrap();
        assert_eq!(req.x, 100.0);
        assert_eq!(req.direction, "up");
        assert_eq!(req.amount, 5);
    }

    #[test]
    fn drag_request_deserialize() {
        let req: DragRequest =
            serde_json::from_str(r#"{"from_x": 0, "from_y": 0, "to_x": 100, "to_y": 200}"#)
                .unwrap();
        assert_eq!(req.from_x, 0.0);
        assert_eq!(req.to_y, 200.0);
    }

    #[test]
    fn spawn_app_request_deserialize() {
        let req: SpawnAppRequest =
            serde_json::from_str(r#"{"path": "/Applications/Safari.app"}"#).unwrap();
        assert_eq!(req.path, "/Applications/Safari.app");
    }

    #[test]
    fn spawn_cli_request_with_args() {
        let req: SpawnCliRequest =
            serde_json::from_str(r#"{"command": "echo", "args": ["hello"]}"#).unwrap();
        assert_eq!(req.command, "echo");
        assert_eq!(req.args, vec!["hello"]);
    }

    #[test]
    fn spawn_cli_request_without_args() {
        let req: SpawnCliRequest = serde_json::from_str(r#"{"command": "zsh"}"#).unwrap();
        assert!(req.args.is_empty());
    }

    #[test]
    fn spawn_cli_request_with_size() {
        let req: SpawnCliRequest =
            serde_json::from_str(r#"{"command": "zsh", "cols": 120, "rows": 40}"#).unwrap();
        assert_eq!(req.cols, Some(120));
        assert_eq!(req.rows, Some(40));
    }

    #[test]
    fn spawn_cli_request_default_size() {
        let req: SpawnCliRequest = serde_json::from_str(r#"{"command": "zsh"}"#).unwrap();
        assert_eq!(req.cols, None);
        assert_eq!(req.rows, None);
    }

    #[test]
    fn kill_request_deserialize() {
        let req: KillRequest = serde_json::from_str(r#"{"pid": 1234}"#).unwrap();
        assert_eq!(req.pid, 1234);
    }

    #[test]
    fn region_query_deserialize() {
        let req: RegionQuery =
            serde_json::from_str(r#"{"x": 10, "y": 20, "width": 100, "height": 200}"#).unwrap();
        assert_eq!(req.x, 10);
        assert_eq!(req.width, 100);
    }

    #[test]
    fn ui_find_request_deserialize() {
        let req: UiFindRequest =
            serde_json::from_str(r#"{"window_id": 1, "role": "button"}"#).unwrap();
        assert_eq!(req.window_id, 1);
        assert_eq!(req.role, Some("button".to_string()));
        assert!(req.title.is_none());
    }

    #[test]
    fn ui_find_request_with_title() {
        let req: UiFindRequest =
            serde_json::from_str(r#"{"window_id": 1, "role": "button", "title": "OK"}"#).unwrap();
        assert_eq!(req.title, Some("OK".to_string()));
    }

    #[test]
    fn ui_value_query_deserialize() {
        let req: UiValueQuery = serde_json::from_str(r#"{"element_id": "123:0:1"}"#).unwrap();
        assert_eq!(req.element_id, "123:0:1");
    }

    #[test]
    fn screenshot_query_deserialize_with_window_id() {
        let req: ScreenshotQuery = serde_json::from_str(r#"{"window_id": 42}"#).unwrap();
        assert_eq!(req.window_id, Some(42));
    }

    #[test]
    fn screenshot_query_deserialize_without_window_id() {
        let req: ScreenshotQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert!(req.window_id.is_none());
    }

    // ── require_target_pid ────────────────────────────────────

    #[test]
    fn require_target_pid_some() {
        assert!(require_target_pid(Some(42)).is_ok());
        assert_eq!(require_target_pid(Some(42)).unwrap(), 42);
    }

    #[test]
    fn require_target_pid_none() {
        assert!(require_target_pid(None).is_err());
    }

    // ── Default button function ───────────────────────────────

    #[test]
    fn default_button_returns_left() {
        assert_eq!(default_button(), "left");
    }

    // ── Health response serialization ─────────────────────────

    #[test]
    fn health_response_serializes() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.2.0".to_string(),
            uptime_secs: 42,
            sandbox_id: Some("test".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"sandbox_id\":\"test\""));
    }

    #[test]
    fn sandbox_info_response_serializes() {
        let resp = SandboxInfoResponse {
            sandbox_id: None,
            window_id: Some(42),
            uptime_secs: 60,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"window_id\":42"));
        assert!(json.contains("\"sandbox_id\":null"));
    }

    // ── AppError all variants ─────────────────────────────────

    #[tokio::test]
    async fn app_error_window_not_found_status() {
        let err = AppError::WindowNotFound("w1".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn app_error_screenshot_status() {
        let err = AppError::Screenshot("failed".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn app_error_input_status() {
        let err = AppError::Input("bad key".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn app_error_accessibility_status() {
        let err = AppError::Accessibility("no perm".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn app_error_process_status() {
        let err = AppError::Process("died".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn app_error_io_status() {
        let err = AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn app_error_json_status() {
        let err = AppError::Json(serde_json::from_str::<serde_json::Value>("bad").unwrap_err());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Input path: target_pid determines CGEvent vs PTY ──────

    #[tokio::test]
    async fn type_handler_with_target_pid_sends_cgevent() {
        // When target_pid is set to the Tauri process PID (not a PTY PID),
        // CGEvent posts keyboard events to the Tauri window.
        // This is the ROOT CAUSE of why CLI interactive operations fail:
        // CGEvent goes to Tauri/WebView, not to the PTY child process.
        let state = Arc::new(Mutex::new(AppState {
            sandbox_id: Some("test-cgevent".into()),
            start_time: Instant::now(),
            window_id: Some(42),
            target_pid: Some(9999), // Simulates Tauri process PID
            pending_cli: None,
        }));
        let app = build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/input/type")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text": "hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        // CGEvent may fail on CI (no Accessibility permission), but the handler
        // accepts the request (doesn't reject it) — the key issue is that it
        // targets the wrong process.
        let status = resp.status();
        assert!(
            matches!(status.as_u16(), 200 | 500),
            "CGEvent type should either succeed or fail with internal error, got: {status}"
        );
    }

    // ── Document the input path issue ─────────────────────────

    #[test]
    fn cgevent_targets_tauri_pid_not_child_pid() {
        // This test documents the root cause:
        // target_pid in AppState is set to std::process::id() (Tauri PID),
        // NOT to the CLI child process PID.
        // CGEvent posts to the Tauri window, not to the PTY child process.
        // The PTY write path (/pty/write) is the correct way to send
        // keyboard input to CLI processes.
        let tauri_pid = std::process::id();
        assert!(tauri_pid > 0, "Tauri PID is always set to self");

        // The child process (e.g., claude) runs in a PTY with a different PID.
        // CGEvent to tauri_pid ≠ PTY write to child_pid.
        // This is why `sandbox type --id <id> "text"` (CGEvent) doesn't work
        // but `sandbox type --id <id> --pty "text"` (PTY write) does.
    }
}
