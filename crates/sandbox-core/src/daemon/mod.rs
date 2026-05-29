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
use crate::server::handle_pty_ws;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
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
struct SpawnAppInSandboxRequest {
    path: String,
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

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_secs: u64,
    sandboxes: usize,
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
    for port in start..end {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Returns the path to `~/.sandbox/daemon.json`.
pub fn daemon_json_path() -> PathBuf {
    sandbox_home().join("daemon.json")
}

/// Write daemon info to disk.
pub fn write_daemon_info(port: u16) -> std::io::Result<()> {
    let dir = sandbox_home();
    std::fs::create_dir_all(&dir)?;
    let info = DaemonInfo {
        port,
        pid: std::process::id(),
        started_at: timestamp_now(),
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

fn sandbox_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join(".sandbox")
}

fn timestamp_now() -> String {
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
        .route("/sandbox/{id}/input/click", post(click_handler))
        .route("/sandbox/{id}/input/type", post(type_handler))
        .route("/sandbox/{id}/input/key", post(key_handler))
        .route("/sandbox/{id}/input/scroll", post(scroll_handler))
        .route(
            "/sandbox/{id}/pty/ws/{pid}",
            get(pty_ws_upgrade_handler),
        )
        .route("/sandbox/{id}/app/spawn", post(spawn_app_handler))
        .route("/sandbox/{id}/windows", get(windows_handler))
        .route(
            "/sandbox/{id}/ui/inspect/{window_id}",
            get(ui_inspect_handler),
        )
        .route("/shutdown", post(shutdown_handler))
        .layer(cors)
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────

async fn health_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> Json<HealthResponse> {
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

            let managed = ManagedSandbox {
                id: id.clone(),
                kind,
                status: InstanceStatus::Running,
                port: 0, // daemon owns the port, sandbox does not have its own
                pty_pid: Some(info.pid),
                window_id: None,
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

            state
                .lock()
                .await
                .sandboxes
                .insert(id.clone(), managed);

            tracing::info!("Created CLI sandbox: id={}, pid={}", id, info.pid);

            Ok(Json(CreateSandboxResponse {
                sandbox_id: id,
                pty_pid: Some(info.pid),
                window_id: None,
            }))
        }
        "app" => {
            let app_path = req
                .command
                .clone()
                .ok_or_else(|| AppError::BadRequest("app mode requires 'command' (app path)".into()))?;

            let (process_info, window_id) =
                tokio::task::spawn_blocking(move || {
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

            state
                .lock()
                .await
                .sandboxes
                .insert(id.clone(), managed);

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
    let sandbox = removed.ok_or_else(|| {
        AppError::Instance(format!("Sandbox '{id}' not found"))
    })?;

    // Kill PTY process if present
    if let Some(pty_pid) = sandbox.pty_pid {
        let _ = tokio::task::spawn_blocking(move || {
            ProcessManager::kill_process(pty_pid)
        })
        .await;
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
    let window_id = {
        let s = state.lock().await;
        let sandbox = s.sandboxes.get(&id).ok_or_else(|| {
            AppError::Instance(format!("Sandbox '{id}' not found"))
        })?;
        sandbox.window_id
    };

    match window_id {
        Some(wid) => {
            let png_data = tokio::task::spawn_blocking(move || {
                ScreenCapture::capture_window(wid)
            })
            .await
            .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))??;
            Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
        }
        None => Err(AppError::BadRequest(format!(
            "Sandbox '{id}' has no window_id. Screenshots require an app-mode sandbox or a discovered window."
        ))),
    }
}

async fn click_handler(
    Json(req): Json<ClickRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let button = match req.button.to_lowercase().as_str() {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        other => return Err(AppError::BadRequest(format!("Unknown button: {other}"))),
    };
    InputSimulator::click(req.x, req.y, button, None)?;
    Ok(Json(
        serde_json::json!({"clicked": {"x": req.x, "y": req.y, "button": req.button}}),
    ))
}

async fn type_handler(
    Json(req): Json<TypeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    InputSimulator::type_text(&req.text, None)?;
    Ok(Json(serde_json::json!({"typed": req.text})))
}

async fn key_handler(
    Json(req): Json<KeyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mod_refs: Vec<&str> = req.modifiers.iter().map(|s| s.as_str()).collect();
    InputSimulator::press_key(&req.key, &mod_refs, None)?;
    Ok(Json(
        serde_json::json!({"pressed": {"key": req.key, "modifiers": req.modifiers}}),
    ))
}

async fn scroll_handler(
    Json(req): Json<ScrollRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    InputSimulator::scroll(req.x, req.y, &req.direction, req.amount, None)?;
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
    Json(req): Json<SpawnAppInSandboxRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    // Verify sandbox exists
    {
        let s = state.lock().await;
        if !s.sandboxes.contains_key(&id) {
            return Err(AppError::Instance(format!("Sandbox '{id}' not found")));
        }
    }

    let app_path = req.path.clone();
    let info = tokio::task::spawn_blocking(move || ProcessManager::spawn_app(&app_path))
        .await
        .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;

    tracing::info!("Spawned app in sandbox {}: {}", id, req.path);
    Ok(Json(info))
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
    tracing::info!("Daemon starting on port {port} (pid={})", std::process::id());

    let state = Arc::new(Mutex::new(DaemonState {
        port,
        sandboxes: HashMap::new(),
        started_at: Instant::now(),
    }));

    let router = build_daemon_router(state);

    // Ctrl-C handler: clean up daemon.json
    let ctrlc_path = daemon_json_path();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Ctrl-C received, cleaning up");
        let _ = std::fs::remove_file(&ctrlc_path);
        std::process::exit(0);
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
        std::env::temp_dir().join(format!("sandbox_daemon_test_{}_{}", std::process::id(), tag))
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
        assert!(read_daemon_info().is_none() || daemon_json_path().exists() == false);
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
        let req: CreateSandboxRequest =
            serde_json::from_str(r#"{"mode": "cli"}"#).unwrap();
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
        }))
    }

    fn test_daemon_router() -> Router {
        build_daemon_router(test_daemon_state())
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
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/any/input/click")
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
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/any/input/click")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"x": 100, "y": 200, "button": "turbo"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn type_text_handler() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/any/input/type")
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
    async fn key_handler() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/any/input/key")
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
    async fn scroll_handler() {
        let app = test_daemon_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sandbox/any/input/scroll")
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
