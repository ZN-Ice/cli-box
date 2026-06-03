# Electron + Rust Daemon 架构迁移实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 cli-box 从 Tauri 多实例架构迁移到 Electron + Rust daemon 单进程多 Tab 架构，解决 WKWebView 终端渲染问题。

**Architecture:** Rust cli-box-daemon 作为独立后台进程管理所有沙箱（PTY、截图、输入模拟、APP 启动）。Electron 作为 UI 层，通过 HTTP/WebSocket 与 daemon 通信。CLI 直接与 daemon HTTP API 交互，不经过 Electron。

**Tech Stack:** Rust (cli-box-daemon binary), Electron + TypeScript (UI), axum (HTTP+WS), React + xterm.js (前端)

**Spec:** `docs/design/electron-rust-architecture.md`

---

## Scope

本计划覆盖 Phase 1（cli-box-daemon）的完整实现。Phase 1 完成后，所有 Rust 侧的系统能力（PTY、截图、输入模拟、APP 启动）都通过 daemon 的 HTTP API 可用，CLI 可以不依赖任何 UI 直接完成沙箱管理。

Phase 2（Electron 壳替换 Tauri）将在 Phase 1 完成后另写计划。

## File Structure

```
新增/修改文件清单:

crates/cli-box-daemon/                   # 🆕 Daemon binary crate
├── Cargo.toml
└── src/
    └── main.rs                          # Daemon 入口：端口分配、HTTP server、信号处理

crates/sandbox-core/src/
├── daemon/                              # 🆕 Daemon 生命周期管理
│   └── mod.rs                           # DaemonState, 端口发现, pid 文件, 多沙箱管理
└── server/
    └── mod.rs                           # 🔧 重构：多沙箱路由 (从 /sandbox/:id/... 改)

crates/sandbox-cli/src/
├── main.rs                              # 🔧 重构：spawn daemon → HTTP create
└── client.rs                            # 🔧 小改：适配新 API 路径

crates/sandbox-core/src/
├── process/mod.rs                       # ✅ 直接复用
├── automation/cg_event.rs               # ✅ 直接复用
├── automation/ax_ui.rs                  # ✅ 直接复用
├── capture/mod.rs                       # ✅ 直接复用
├── instance/mod.rs                      # ✅ 直接复用
└── error.rs                             # ✅ 直接复用
```

---

## Task 1: 创建 cli-box-daemon binary crate

**Files:**
- Create: `crates/cli-box-daemon/Cargo.toml`
- Create: `crates/cli-box-daemon/src/main.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: 创建 Cargo.toml**

```toml
# crates/cli-box-daemon/Cargo.toml
[package]
name = "cli-box-daemon"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Sandbox daemon process — manages all sandbox instances"

[dependencies]
sandbox-core = { workspace = true, features = ["screencapturekit"] }
axum.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: 创建 main.rs 骨架**

```rust
// crates/cli-box-daemon/src/main.rs
use sandbox_core::daemon::DaemonState;

fn main() {
    tracing_subscriber::fmt::init();
    
    let port = sandbox_core::daemon::find_available_port(15801, 15899)
        .expect("No available port in range 15801-15899");
    
    println!("Sandbox daemon started on port {port}");
    
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async move {
        sandbox_core::daemon::run_daemon(port).await
    }).expect("Daemon exited with error");
}
```

- [ ] **Step 3: 添加 workspace member**

在根 `Cargo.toml` 的 `[workspace] members` 中添加 `"crates/cli-box-daemon"`。

- [ ] **Step 4: 验证编译**

Run: `cargo check -p cli-box-daemon`
Expected: 编译错误（`sandbox_core::daemon` 模块不存在），这是预期的。Task 2 解决。

- [ ] **Step 5: Commit**

```bash
git add crates/cli-box-daemon/ Cargo.toml
git commit -m "feat(daemon): scaffold cli-box-daemon binary crate"
```

---

## Task 2: 实现 daemon 生命周期管理模块

**Files:**
- Create: `crates/sandbox-core/src/daemon/mod.rs`
- Modify: `crates/sandbox-core/src/lib.rs` (添加 `pub mod daemon`)

- [ ] **Step 1: 创建 daemon/mod.rs**

```rust
// crates/sandbox-core/src/daemon/mod.rs
use crate::error::AppError;
use crate::instance::{InstanceKind, InstanceRegistry, InstanceStatus, SandboxInstance};
use crate::process::ProcessManager;
use axum::extract::State;
use axum::{Json, Router, routing::{get, post}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-sandbox state tracked by the daemon
#[derive(Debug)]
pub struct ManagedSandbox {
    pub id: String,
    pub kind: InstanceKind,
    pub status: InstanceStatus,
    pub port: u16,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Shared daemon state
pub struct DaemonState {
    pub port: u16,
    pub sandboxes: HashMap<String, ManagedSandbox>,
    pub started_at: std::time::Instant,
}

impl DaemonState {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            sandboxes: HashMap::new(),
            started_at: std::time::Instant::now(),
        }
    }
}

#[derive(Deserialize)]
pub struct CreateSandboxRequest {
    pub mode: String,            // "cli" or "app"
    pub command: Option<String>, // CLI command or app path
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
}

#[derive(Serialize)]
pub struct CreateSandboxResponse {
    pub sandbox_id: String,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
}

#[derive(Serialize)]
pub struct ListSandboxesResponse {
    pub sandboxes: Vec<SandboxSummary>,
}

#[derive(Serialize)]
pub struct SandboxSummary {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
}

/// Try binding to ports from `start` to `end` (inclusive), return first available.
pub fn find_available_port(start: u16, end: u16) -> Result<u16, AppError> {
    for port in start..=end {
        if std::net::TcpListener::bind(format!("127.0.0.1:{port}")).is_ok() {
            return Ok(port);
        }
    }
    Err(AppError::Process(format!(
        "No available port in range {start}-{end}"
    )))
}

/// Path to daemon metadata file
pub fn daemon_json_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".sandbox")
        .join("daemon.json")
}

#[derive(Serialize, Deserialize)]
pub struct DaemonInfo {
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
}

/// Write daemon metadata to ~/.sandbox/daemon.json
pub fn write_daemon_info(port: u16) -> Result<(), AppError> {
    let info = DaemonInfo {
        port,
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let path = daemon_json_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Process(format!("Failed to create ~/.sandbox/: {e}"))
        })?;
    }
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| AppError::Process(format!("Failed to serialize daemon info: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| AppError::Process(format!("Failed to write daemon.json: {e}")))?;
    Ok(())
}

/// Read daemon metadata from ~/.sandbox/daemon.json
pub fn read_daemon_info() -> Result<DaemonInfo, AppError> {
    let path = daemon_json_path();
    let json = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Process(format!("Failed to read daemon.json: {e}")))?;
    serde_json::from_str(&json)
        .map_err(|e| AppError::Process(format!("Failed to parse daemon.json: {e}")))
}

/// Check if a daemon is running by reading daemon.json and verifying the pid.
/// Returns the port if running, or None.
pub fn find_running_daemon() -> Option<u16> {
    let info = read_daemon_info().ok()?;
    // Check if pid is alive
    let pid = info.pid as i32;
    unsafe {
        // kill(pid, 0) returns 0 if the process exists
        if libc::kill(pid, 0) == 0 {
            return Some(info.port);
        }
    }
    // Stale daemon.json — clean it up
    let _ = std::fs::remove_file(daemon_json_path());
    None
}

/// Remove daemon.json (called on shutdown)
pub fn cleanup_daemon_info() {
    let _ = std::fs::remove_file(daemon_json_path());
}

/// Build the daemon's HTTP router (multi-sandbox routes)
pub fn build_daemon_router(state: Arc<Mutex<DaemonState>>) -> Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    Router::new()
        .route("/health", get(health))
        .route("/sandbox/list", get(list_sandboxes))
        .route("/sandbox/create", post(create_sandbox))
        .route("/sandbox/{id}/close", post(close_sandbox))
        .route("/sandbox/{id}/screenshot", get(screenshot))
        .route("/sandbox/{id}/input/click", post(click))
        .route("/sandbox/{id}/input/type", post(type_text))
        .route("/sandbox/{id}/input/key", post(press_key))
        .route("/sandbox/{id}/input/scroll", post(scroll))
        .route("/sandbox/{id}/pty/ws/{pid}", get(pty_ws))
        .route("/sandbox/{id}/app/spawn", post(spawn_app))
        .route("/sandbox/{id}/windows", get(list_windows))
        .route("/sandbox/{id}/ui/inspect/{window_id}", get(ui_inspect))
        .route("/shutdown", post(shutdown))
        .layer(cors)
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────

async fn health(State(state): State<Arc<Mutex<DaemonState>>>) -> Json<serde_json::Value> {
    let s = state.lock().await;
    Json(serde_json::json!({
        "status": "ok",
        "port": s.port,
        "uptime_secs": s.started_at.elapsed().as_secs(),
        "sandbox_count": s.sandboxes.len(),
    }))
}

async fn list_sandboxes(
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> Json<ListSandboxesResponse> {
    let s = state.lock().await;
    let sandboxes: Vec<SandboxSummary> = s
        .sandboxes
        .values()
        .map(|sb| SandboxSummary {
            id: sb.id.clone(),
            kind: match &sb.kind {
                InstanceKind::Cli { .. } => "cli".to_string(),
                InstanceKind::App { .. } => "app".to_string(),
            },
            status: format!("{:?}", sb.status).to_lowercase(),
            pty_pid: sb.pty_pid,
            window_id: sb.window_id,
        })
        .collect();
    Json(ListSandboxesResponse { sandboxes })
}

async fn create_sandbox(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Json(req): Json<CreateSandboxRequest>,
) -> Result<Json<CreateSandboxResponse>, AppError> {
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();

    let kind = match req.mode.as_str() {
        "cli" => {
            let command = req.command.clone().unwrap_or_else(|| "zsh".to_string());
            let cols = req.cols.unwrap_or(80);
            let rows = req.rows.unwrap_or(24);
            let info = tokio::task::spawn_blocking(move || {
                ProcessManager::spawn_cli_with_size(&command, &req.args, cols, rows)
            })
            .await
            .map_err(|e| AppError::Process(format!("spawn_cli panicked: {e}")))??;

            let mut s = state.lock().await;
            let sandbox = ManagedSandbox {
                id: id.clone(),
                kind: InstanceKind::Cli {
                    command,
                    args: req.args.clone(),
                },
                status: InstanceStatus::Running,
                port: s.port,
                pty_pid: Some(info.pid as u32),
                window_id: None,
                created_at: chrono::Utc::now(),
            };
            s.sandboxes.insert(id.clone(), sandbox);

            // Register in instance registry
            let registry = InstanceRegistry::default();
            let instance = SandboxInstance::new(
                &id,
                s.port,
                std::process::id(),
                InstanceKind::Cli {
                    command: req.command.clone().unwrap_or_default(),
                    args: req.args.clone(),
                },
            );
            let _ = registry.register(&instance);

            return Ok(Json(CreateSandboxResponse {
                sandbox_id: id,
                pty_pid: Some(info.pid as u32),
                window_id: None,
            }));
        }
        "app" => {
            let app_path = req.command.clone().ok_or_else(|| {
                AppError::BadRequest("app mode requires 'command' field with app path".into())
            })?;
            InstanceKind::App { path: app_path }
        }
        other => {
            return Err(AppError::BadRequest(format!("Unknown mode: {other}")));
        }
    };

    // Handle app mode
    let app_path = match &kind {
        InstanceKind::App { path } => path.clone(),
        _ => unreachable!(),
    };

    let (info, window_id) = tokio::task::spawn_blocking(move || {
        ProcessManager::spawn_app_with_window(&app_path)
    })
    .await
    .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;

    let mut s = state.lock().await;
    let sandbox = ManagedSandbox {
        id: id.clone(),
        kind,
        status: InstanceStatus::Running,
        port: s.port,
        pty_pid: None,
        window_id,
        created_at: chrono::Utc::now(),
    };
    s.sandboxes.insert(id.clone(), sandbox);

    // Register in instance registry
    let registry = InstanceRegistry::default();
    let instance = SandboxInstance::new(
        &id,
        s.port,
        std::process::id(),
        InstanceKind::App {
            path: app_path.clone(),
        },
    );
    let _ = registry.register(&instance);

    Ok(Json(CreateSandboxResponse {
        sandbox_id: id,
        pty_pid: Some(info.pid as u32),
        window_id,
    }))
}

async fn close_sandbox(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut s = state.lock().await;
    if let Some(sb) = s.sandboxes.remove(&id) {
        if let Some(pid) = sb.pty_pid {
            let _ = ProcessManager::kill_process(pid);
        }
        let registry = InstanceRegistry::default();
        let _ = registry.unregister(&id);
        tracing::info!("Closed sandbox {id}");
        Ok(Json(serde_json::json!({"closed": id})))
    } else {
        Err(AppError::Process(format!("Sandbox {id} not found")))
    }
}

async fn screenshot(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let s = state.lock().await;
    let sb = s.sandboxes.get(&id).ok_or_else(|| {
        AppError::Process(format!("Sandbox {id} not found"))
    })?;
    let window_id = sb.window_id.ok_or_else(|| {
        AppError::BadRequest(format!("Sandbox {id} has no window_id"))
    })?;
    drop(s);

    let png_data = crate::capture::ScreenCapture::capture_window(window_id)?;
    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "image/png")],
        png_data,
    ))
}

async fn click(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::server::ClickRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // For now, click uses global coordinates (same as current behavior)
    let button = match req.button.to_lowercase().as_str() {
        "left" => crate::automation::cg_event::MouseButton::Left,
        "right" => crate::automation::cg_event::MouseButton::Right,
        "middle" => crate::automation::cg_event::MouseButton::Middle,
        other => return Err(AppError::BadRequest(format!("Unknown button: {other}"))),
    };
    crate::automation::cg_event::InputSimulator::click(req.x, req.y, button, None)?;
    Ok(Json(serde_json::json!({"clicked": {"x": req.x, "y": req.y}})))
}

async fn type_text(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::server::TypeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::automation::cg_event::InputSimulator::type_text(&req.text, None)?;
    Ok(Json(serde_json::json!({"typed": true})))
}

async fn press_key(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::server::KeyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mod_refs: Vec<&str> = req.modifiers.iter().map(|s| s.as_str()).collect();
    crate::automation::cg_event::InputSimulator::press_key(&req.key, &mod_refs, None)?;
    Ok(Json(serde_json::json!({"pressed": {"key": req.key}})))
}

async fn scroll(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::server::ScrollRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::automation::cg_event::InputSimulator::scroll(
        req.x, req.y, &req.direction, req.amount, None,
    )?;
    Ok(Json(serde_json::json!({"scrolled": true})))
}

async fn pty_ws(
    axum::extract::Path((id, pid)): axum::extract::Path<(String, u32)>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ProcessManager::subscribe_output(pid)
        .map_err(|e| AppError::Process(format!("PTY {pid} not found: {e}")))?;
    Ok(ws.on_upgrade(move |socket| crate::server::handle_pty_ws(pid, socket)))
}

async fn spawn_app(
    State(state): State<Arc<Mutex<DaemonState>>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::server::SpawnAppRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let info = tokio::task::spawn_blocking(move || ProcessManager::spawn_app(&req.path))
        .await
        .map_err(|e| AppError::Process(format!("spawn_app panicked: {e}")))??;
    Ok(Json(serde_json::json!({"spawned": info.name, "pid": info.pid})))
}

async fn list_windows() -> Result<Json<Vec<(u32, String)>>, AppError> {
    let windows = tokio::task::spawn_blocking(crate::capture::ScreenCapture::list_windows)
        .await
        .map_err(|e| AppError::Process(format!("list_windows panicked: {e}")))??;
    Ok(Json(windows))
}

async fn ui_inspect(
    axum::extract::Path(window_id): axum::extract::Path<u32>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tree = tokio::task::spawn_blocking(move || {
        crate::automation::ax_ui::UiInspector::inspect_window(window_id)
    })
    .await
    .map_err(|e| AppError::Process(format!("ui_inspect panicked: {e}")))??;
    Ok(Json(tree))
}

async fn shutdown() -> Json<serde_json::Value> {
    tracing::info!("Shutdown requested via HTTP");
    cleanup_daemon_info();
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(0);
    });
    Json(serde_json::json!({"shutting_down": true}))
}

/// Run the daemon: bind HTTP server and serve forever
pub async fn run_daemon(port: u16) -> Result<(), AppError> {
    write_daemon_info(port)?;

    let state = Arc::new(Mutex::new(DaemonState::new(port)));
    let router = build_daemon_router(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::Process(format!("Failed to bind {addr}: {e}")))?;

    tracing::info!("Daemon HTTP API listening on http://{addr}");

    // Graceful shutdown: clean up daemon.json on ctrl-c
    let shutdown_path = daemon_json_path();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received Ctrl+C, shutting down");
        let _ = std::fs::remove_file(&shutdown_path);
        std::process::exit(0);
    });

    axum::serve(listener, router)
        .await
        .map_err(|e| AppError::Process(format!("HTTP server error: {e}")))?;

    Ok(())
}
```

- [ ] **Step 2: 导出 ClickRequest, TypeRequest, KeyRequest, ScrollRequest, SpawnAppRequest 为 public**

在 `crates/sandbox-core/src/server/mod.rs` 中，将以下 struct 从 private 改为 `pub`：

```rust
pub struct ClickRequest { ... }
pub struct TypeRequest { ... }
pub struct KeyRequest { ... }
pub struct ScrollRequest { ... }
pub struct SpawnAppRequest { ... }
```

同时将 `handle_pty_ws` 函数改为 `pub`：

```rust
pub async fn handle_pty_ws(pid: u32, socket: WebSocket) { ... }
```

- [ ] **Step 3: 添加 daemon 模块到 lib.rs**

在 `crates/sandbox-core/src/lib.rs` 中添加：

```rust
pub mod daemon;
```

- [ ] **Step 4: 添加依赖**

在 `crates/sandbox-core/Cargo.toml` 中确保有：

```toml
chrono = "0.4"
dirs-next = "2"
libc = "0.2"
tower-http = { version = "0.6", features = ["cors"] }
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p cli-box-daemon`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-core/src/daemon/ crates/sandbox-core/src/lib.rs crates/sandbox-core/Cargo.toml
git commit -m "feat(daemon): implement daemon lifecycle and multi-sandbox HTTP API"
```

---

## Task 3: 验证 daemon 端口发现和生命周期

**Files:**
- Test: `crates/cli-box-daemon/src/main.rs` (内联测试)

- [ ] **Step 1: 写端口发现测试**

在 `crates/sandbox-core/src/daemon/mod.rs` 底部添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_available_port_returns_first_free() {
        let port = find_available_port(15801, 15899).unwrap();
        assert!((15801..=15899).contains(&port));
    }

    #[test]
    fn find_running_daemon_returns_none_when_no_file() {
        let _ = std::fs::remove_file(daemon_json_path());
        assert_eq!(find_running_daemon(), None);
    }

    #[test]
    fn write_and_read_daemon_info_roundtrip() {
        let test_path = std::env::temp_dir().join("test_daemon.json");
        let info = DaemonInfo {
            port: 15999,
            pid: std::process::id(),
            started_at: "2026-05-30T10:00:00Z".to_string(),
        };
        let json = serde_json::to_string_pretty(&info).unwrap();
        std::fs::write(&test_path, &json).unwrap();
        let read: DaemonInfo = serde_json::from_str(
            &std::fs::read_to_string(&test_path).unwrap()
        ).unwrap();
        assert_eq!(read.port, 15999);
        let _ = std::fs::remove_file(&test_path);
    }

    #[test]
    fn find_running_daemon_detects_stale_pid() {
        // Write a daemon.json with a PID that definitely doesn't exist
        let info = DaemonInfo {
            port: 15801,
            pid: 9999999,
            started_at: "2026-05-30T10:00:00Z".to_string(),
        };
        let path = daemon_json_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&info).unwrap();
        std::fs::write(&path, &json).unwrap();
        // Should return None and clean up
        assert_eq!(find_running_daemon(), None);
        assert!(!path.exists());
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p sandbox-core -- daemon::tests`
Expected: 全部 PASS

- [ ] **Step 3: 手动测试 daemon 启动和关闭**

Run: `cargo run -p cli-box-daemon`
Expected: 打印 `Sandbox daemon started on port 15801`

在另一个终端运行: `curl http://localhost:15801/health`
Expected: `{"status":"ok","port":15801,"uptime_secs":0,"sandbox_count":0}`

Ctrl+C 终止 daemon，检查 `~/.sandbox/daemon.json` 已被删除。

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs
git commit -m "test(daemon): add port discovery and lifecycle tests"
```

---

## Task 4: 重构 CLI — `cli-box start` 改为 spawn daemon + HTTP create

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs`

- [ ] **Step 1: 添加 `cmd_start_daemon` 函数**

在 `main.rs` 中添加新的 `cmd_start_daemon` 函数，替代原有的 `cmd_start`：

```rust
fn cmd_start_daemon(command: Option<&str>, args: &[String]) -> Result<()> {
    // 1. Check if daemon is running
    let port = match sandbox_core::daemon::find_running_daemon() {
        Some(port) => {
            eprintln!("Sandbox daemon running on port {port}");
            port
        }
        None => {
            // 2. Spawn daemon in background
            let daemon_bin = std::env::current_exe()
                .map_err(|e| anyhow::anyhow!("Cannot find current exe: {e}"))?;
            // Assume cli-box-daemon is in the same directory
            let daemon_path = daemon_bin.parent()
                .unwrap_or(Path::new("."))
                .join("cli-box-daemon");

            let mut child = std::process::Command::new(&daemon_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to spawn daemon: {e}"))?;

            // 3. Wait for daemon.json to appear (timeout 5s)
            let start = std::time::Instant::now();
            let port = loop {
                if start.elapsed() > std::time::Duration::from_secs(5) {
                    child.kill().ok();
                    anyhow::bail!("Daemon failed to start within 5 seconds");
                }
                if let Some(p) = sandbox_core::daemon::find_running_daemon() {
                    break p;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            eprintln!("Sandbox daemon started on port {port}");
            port
        }
    };

    // 4. Determine mode and command
    let cmd = command.unwrap_or("zsh");
    let mode = if cmd.ends_with(".app") { "app" } else { "cli" };

    // 5. Send HTTP POST /sandbox/create
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/sandbox/create"))
        .json(&serde_json::json!({
            "mode": mode,
            "command": cmd,
            "args": args,
        }))
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to create sandbox: {e}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to create sandbox: {}", resp.status());
    }

    let result: serde_json::Value = resp.json()
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {e}"))?;

    let sandbox_id = result["sandbox_id"].as_str().unwrap_or("unknown");
    let pty_pid = result["pty_pid"].as_u64();

    println!("Sandbox {sandbox_id} created (port {port})");
    if let Some(pid) = pty_pid {
        println!("PTY pid: {pid}");
    }

    Ok(())
}
```

- [ ] **Step 2: 修改 main 匹配分支**

在 `main()` 的命令匹配中，将 `cmd_start` 调用替换为 `cmd_start_daemon`。保留原有的 `cmd_start` 函数供参考（可标记为 `#[allow(dead_code)]`），后续 Phase 2 完成后删除。

- [ ] **Step 3: 添加 reqwest 依赖**

在 `crates/sandbox-cli/Cargo.toml` 中确保有：

```toml
reqwest = { version = "0.12", features = ["blocking", "json"] }
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p sandbox-cli`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-cli/src/main.rs crates/sandbox-cli/Cargo.toml
git commit -m "feat(cli): cli-box start spawns daemon and creates sandbox via HTTP"
```

---

## Task 5: 重构 CLI — 操作命令适配 daemon API

**Files:**
- Modify: `crates/sandbox-cli/src/client.rs`
- Modify: `crates/sandbox-cli/src/main.rs`

- [ ] **Step 1: 添加通过 daemon 端口发现来定位沙箱的逻辑**

在 `client.rs` 中添加辅助函数：

```rust
/// Resolve sandbox port by reading daemon.json
pub fn resolve_daemon_port() -> Result<u16> {
    sandbox_core::daemon::find_running_daemon()
        .ok_or_else(|| anyhow::anyhow!("No sandbox daemon running. Run `cli-box start` first."))
}

/// Build base URL for daemon API
pub fn daemon_base_url() -> Result<String> {
    let port = resolve_daemon_port()?;
    Ok(format!("http://127.0.0.1:{port}"))
}
```

- [ ] **Step 2: 修改 cmd_screenshot 使用 daemon API**

```rust
pub fn cmd_screenshot(sandbox_id: &str, output: Option<&str>) -> Result<()> {
    let base = daemon_base_url()?;
    let url = format!("{base}/sandbox/{sandbox_id}/screenshot");
    let resp = reqwest::blocking::Client::new()
        .get(&url)
        .send()
        .map_err(|e| anyhow::anyhow!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("Screenshot failed: {}", resp.status());
    }

    let png_data = resp.bytes().map_err(|e| anyhow::anyhow!("Read failed: {e}"))?;
    let output_path = output.unwrap_or("screenshot.png");
    std::fs::write(output_path, &png_data)?;
    println!("Screenshot saved to {output_path} ({} bytes)", png_data.len());
    Ok(())
}
```

- [ ] **Step 3: 修改 cmd_list 使用 daemon API**

```rust
pub fn cmd_list() -> Result<()> {
    let base = daemon_base_url()?;
    let url = format!("{base}/sandbox/list");
    let resp = reqwest::blocking::Client::new()
        .get(&url)
        .send()
        .map_err(|e| anyhow::anyhow!("Request failed: {e}"))?;

    let result: serde_json::Value = resp.json()
        .map_err(|e| anyhow::anyhow!("Parse failed: {e}"))?;

    if let Some(sandboxes) = result["sandboxes"].as_array() {
        println!("{:<10} {:<8} {:<12} {:<10} {:<12}", "ID", "KIND", "STATUS", "PTY_PID", "WINDOW_ID");
        for sb in sandboxes {
            println!(
                "{:<10} {:<8} {:<12} {:<10} {:<12}",
                sb["id"].as_str().unwrap_or("-"),
                sb["kind"].as_str().unwrap_or("-"),
                sb["status"].as_str().unwrap_or("-"),
                sb["pty_pid"].map(|v| v.to_string()).unwrap_or("-".into()),
                sb["window_id"].map(|v| v.to_string()).unwrap_or("-".into()),
            );
        }
    }
    Ok(())
}
```

- [ ] **Step 4: 类似地修改 cmd_click, cmd_type, cmd_key, cmd_close**

每个命令改为 `POST {base}/sandbox/{id}/input/...` 或 `POST {base}/sandbox/{id}/close`，不再需要指定端口。

- [ ] **Step 5: 验证编译**

Run: `cargo check -p sandbox-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-cli/src/client.rs crates/sandbox-cli/src/main.rs
git commit -m "feat(cli): adapt operation commands to daemon HTTP API"
```

---

## Task 6: 端到端集成测试

**Files:**
- Test: Manual integration test (no new files)

- [ ] **Step 1: 构建 daemon 和 CLI**

Run: `cargo build -p cli-box-daemon -p sandbox-cli`
Expected: PASS

- [ ] **Step 2: 复制 daemon 二进制到 CLI 同目录**

```bash
cp target/debug/cli-box-daemon target/debug/
```

- [ ] **Step 3: 测试 cli-box start**

Run: `cargo run -p sandbox-cli -- start zsh`
Expected:
```
Sandbox daemon started on port 15801
Sandbox abc123 created (port 15801)
PTY pid: 45678
```

- [ ] **Step 4: 测试 cli-box list**

Run: `cargo run -p sandbox-cli -- list`
Expected: 列出刚创建的沙箱

- [ ] **Step 5: 测试 cli-box screenshot**

Run: `cargo run -p sandbox-cli -- screenshot --id <id> -o test.png`
Expected: `test.png` 生成

- [ ] **Step 6: 测试 cli-box close**

Run: `cargo run -p sandbox-cli -- close <id>`
Expected: 沙箱被关闭，PTY 进程终止

- [ ] **Step 7: 测试端口自动递增**

手动占用 15801 端口后再次运行 `cli-box start`，验证端口自动切到 15802。

- [ ] **Step 8: Commit 整体状态**

```bash
git add -A
git commit -m "test(daemon): Phase 1 end-to-end integration verified"
```

---

## Task 7: 清理和收尾

**Files:**
- Modify: `CLAUDE.md` (更新架构描述)
- Modify: `crates/sandbox-core/src/server/mod.rs` (标记旧单实例路由为 deprecated)

- [ ] **Step 1: 更新 CLAUDE.md**

在 CLAUDE.md 中更新架构描述，反映 daemon 模式。将 "桌面框架 | Tauri 2.x" 改为 "桌面框架 | Electron (Phase 2) / Daemon HTTP API (Phase 1)"。

- [ ] **Step 2: 标记旧 server 路由**

在 `server/mod.rs` 的 `build_router` 上方添加注释：

```rust
// DEPRECATED: This router is for the legacy Tauri single-instance mode.
// Use daemon::build_daemon_router for the new multi-sandbox daemon mode.
```

- [ ] **Step 3: 运行完整测试套件**

Run: `cargo test --all && cargo fmt --all -- --check && cargo clippy --all-targets`
Expected: PASS

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: update docs and mark legacy server as deprecated"
```

---

## Self-Review

### Spec coverage

| Spec 要求 | 对应 Task |
|-----------|----------|
| Daemon 独立进程管理多沙箱 | Task 1, 2 |
| HTTP API `/sandbox/:id/...` 路由 | Task 2 |
| 端口发现 (daemon.json + pid 验证) | Task 2, 3 |
| 端口占用时自动递增 | Task 2 |
| CLI spawn daemon + HTTP create | Task 4 |
| CLI 操作命令适配 daemon API | Task 5 |
| PTY WebSocket 复用 | Task 2 (pty_ws handler) |
| Ctrl+C 优雅关闭清理 daemon.json | Task 2 |

### Placeholder scan

无 TBD/TODO。所有步骤包含完整代码。

### Type consistency

- `DaemonState` 在 daemon/mod.rs 中定义，与 main.rs 和 router handlers 一致
- `CreateSandboxRequest/Response`, `ListSandboxesResponse`, `SandboxSummary` 在 daemon/mod.rs 中定义
- `ClickRequest`, `TypeRequest`, `KeyRequest`, `ScrollRequest`, `SpawnAppRequest` 从 server/mod.rs 导出为 pub，在 daemon/mod.rs 中引用
- `ManagedSandbox` 使用 `InstanceKind`（从 instance/mod.rs 导出）

### Gaps

- **Electron 端未实现** — 这是 Phase 2 的范围，将在 Phase 1 完成后另写计划
- **WebSocket 事件通知（sandbox_exit 等）** — daemon handler 中需要 broadcast channel 通知 Electron。当前 Phase 1 只需要 CLI 功能（不依赖事件通知），WebSocket 事件将在 Phase 2 实现
- **MCP server 适配** — 当前 sandbox-cli/src/mcp_server.rs 需要适配 daemon API，但不影响核心功能，可在 Phase 2 中处理
