use crate::automation::ax_ui::{UiElement, UiInspector};
use crate::automation::cg_event::{InputSimulator, MouseButton};
use crate::capture::ScreenCapture;
use crate::diff::{diff_images, DiffOptions, DiffResult};
use crate::error::AppError;
use crate::player::ActionPlayer;
use crate::process::ProcessManager;
use crate::recorder::{Action, ActionRecorder};
use crate::scenario::ScenarioRunner;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Shared application state for the HTTP server
pub struct AppState {
    pub sandbox_id: Option<String>,
    pub start_time: Instant,
    pub window_id: Option<u32>,
    pub recorder: ActionRecorder,
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
struct PtyWriteRequest {
    pid: u32,
    data: String,
}

#[derive(Deserialize)]
struct RecordStartRequest {
    #[serde(default)]
    output_path: Option<String>,
}

#[derive(Deserialize)]
struct PlaybackRequest {
    actions: Vec<Action>,
    #[serde(default = "default_speed")]
    speed: f64,
}

fn default_speed() -> f64 {
    1.0
}

#[derive(Deserialize)]
struct ScenarioRequest {
    yaml: String,
    #[serde(default = "default_speed")]
    speed: f64,
}

#[derive(Deserialize)]
struct DiffRequest {
    expected: String,
    actual: String,
    #[serde(default)]
    threshold: Option<u8>,
    #[serde(default)]
    max_diff_percentage: Option<f64>,
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
    Router::new()
        .route("/health", get(health_handler))
        .route("/sandbox/info", get(sandbox_info_handler))
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
        .route("/pty/write", post(pty_write_handler))
        .route("/pty/output/{pid}", get(pty_output_handler))
        .route("/ui/inspect/{window_id}", get(ui_inspect_handler))
        .route("/ui/find", post(ui_find_handler))
        .route("/ui/value", get(ui_value_handler))
        .route("/record/start", post(record_start_handler))
        .route("/record/stop", post(record_stop_handler))
        .route("/record/actions", get(record_actions_handler))
        .route("/playback/actions", post(playback_actions_handler))
        .route("/scenario/run", post(scenario_run_handler))
        .route("/diff", post(diff_handler))
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

async fn shutdown_handler() -> Json<serde_json::Value> {
    tracing::info!("Shutdown requested via HTTP");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(0);
    });
    Json(serde_json::json!({"shutting_down": true}))
}

async fn list_windows_handler() -> Result<Json<Vec<(u32, String)>>, AppError> {
    let windows = ScreenCapture::list_windows()?;
    Ok(Json(windows))
}

async fn list_processes_handler() -> Result<Json<Vec<crate::process::ProcessInfo>>, AppError> {
    let processes = ProcessManager::list_processes()?;
    Ok(Json(processes))
}

async fn spawn_app_handler(
    Json(req): Json<SpawnAppRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let info = ProcessManager::spawn_app(&req.path)?;
    Ok(Json(info))
}

async fn spawn_cli_handler(
    Json(req): Json<SpawnCliRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let info = ProcessManager::spawn_cli(&req.command, &req.args)?;
    Ok(Json(info))
}

async fn kill_process_handler(
    Json(req): Json<KillRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    ProcessManager::kill_process(req.pid)?;
    Ok(Json(serde_json::json!({"killed": req.pid})))
}

async fn click_handler(Json(req): Json<ClickRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let button = match req.button.to_lowercase().as_str() {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        other => return Err(AppError::BadRequest(format!("Unknown button: {other}"))),
    };
    InputSimulator::click(req.x, req.y, button)?;
    Ok(Json(
        serde_json::json!({"clicked": {"x": req.x, "y": req.y, "button": req.button}}),
    ))
}

async fn type_handler(Json(req): Json<TypeRequest>) -> Result<Json<serde_json::Value>, AppError> {
    InputSimulator::type_text(&req.text)?;
    Ok(Json(serde_json::json!({"typed": req.text})))
}

async fn key_handler(Json(req): Json<KeyRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let mod_refs: Vec<&str> = req.modifiers.iter().map(|s| s.as_str()).collect();
    InputSimulator::press_key(&req.key, &mod_refs)?;
    Ok(Json(
        serde_json::json!({"pressed": {"key": req.key, "modifiers": req.modifiers}}),
    ))
}

async fn scroll_handler(
    Json(req): Json<ScrollRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    InputSimulator::scroll(req.x, req.y, &req.direction, req.amount)?;
    Ok(Json(serde_json::json!({"scrolled": true})))
}

async fn drag_handler(Json(req): Json<DragRequest>) -> Result<Json<serde_json::Value>, AppError> {
    InputSimulator::drag(req.from_x, req.from_y, req.to_x, req.to_y)?;
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
        None => {
            let png_data = ScreenCapture::capture_sandbox()?;
            Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
        }
    }
}

async fn screenshot_region_handler(
    Query(q): Query<RegionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let png_data = ScreenCapture::capture_region(q.x, q.y, q.width, q.height)?;
    Ok((StatusCode::OK, [("content-type", "image/png")], png_data))
}

async fn pty_write_handler(
    Json(req): Json<PtyWriteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    ProcessManager::send_input(req.pid, req.data.as_bytes())?;
    Ok(Json(serde_json::json!({"written": true})))
}

async fn pty_output_handler(Path(pid): Path<u32>) -> Result<Json<serde_json::Value>, AppError> {
    let output = ProcessManager::read_output(pid)?;
    Ok(Json(serde_json::json!({"output": output})))
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

// ── Recording & Playback ──────────────────────────────────

async fn record_start_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<RecordStartRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let app = state.lock().await;
    let path = req.output_path.map(std::path::PathBuf::from);
    app.recorder.start(path)?;
    Ok(Json(serde_json::json!({"recording": true})))
}

async fn record_stop_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let app = state.lock().await;
    let actions = app.recorder.stop()?;
    Ok(Json(serde_json::json!({
        "recording": false,
        "actions_count": actions.len(),
        "actions": actions,
    })))
}

async fn record_actions_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<Vec<Action>>, AppError> {
    let app = state.lock().await;
    Ok(Json(app.recorder.actions()))
}

async fn playback_actions_handler(
    Json(req): Json<PlaybackRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut player = ActionPlayer::new(req.speed);
    let results = player.play(&req.actions).await;
    Ok(Json(serde_json::json!({
        "results_count": results.len(),
        "results": format!("{results:?}"),
    })))
}

async fn scenario_run_handler(
    Json(req): Json<ScenarioRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let scenario = ScenarioRunner::load_from_str(&req.yaml)?;
    let report = ScenarioRunner::run(&scenario, req.speed).await;
    Ok(Json(serde_json::json!({
        "name": report.name,
        "status": format!("{:?}", report.status),
        "passed": report.passed_steps,
        "failed": report.failed_steps,
        "total": report.total_steps,
        "duration_ms": report.duration_ms,
        "report_markdown": report.to_markdown(),
        "report_json": serde_json::to_value(&report).unwrap_or_default(),
    })))
}

async fn diff_handler(Json(req): Json<DiffRequest>) -> Result<Json<DiffResult>, AppError> {
    use base64::Engine;
    let expected = base64::engine::general_purpose::STANDARD
        .decode(&req.expected)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 (expected): {e}")))?;
    let actual = base64::engine::general_purpose::STANDARD
        .decode(&req.actual)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 (actual): {e}")))?;

    let mut options = DiffOptions::default();
    if let Some(t) = req.threshold {
        options.threshold = t;
    }
    if let Some(m) = req.max_diff_percentage {
        options.max_diff_percentage = m;
    }

    let result = diff_images(&expected, &actual, &options)?;
    Ok(Json(result))
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
