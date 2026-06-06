//! Integration tests for the daemon HTTP API.
//!
//! These tests use `tower::ServiceExt::oneshot` to test the daemon router
//! without binding to a real TCP port.

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use cli_box_core::daemon::{build_daemon_router, DaemonState, ManagedSandbox};
use cli_box_core::instance::{InstanceKind, InstanceStatus};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;

fn empty_state() -> Arc<Mutex<DaemonState>> {
    Arc::new(Mutex::new(DaemonState {
        port: 0,
        sandboxes: HashMap::new(),
        started_at: std::time::Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        screenshot_request_counter: 0,
        terminal_ready_sandboxes: HashSet::new(),
    }))
}

fn router() -> axum::Router {
    build_daemon_router(empty_state())
}

fn state_with_sandbox() -> Arc<Mutex<DaemonState>> {
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
        port: 0,
        sandboxes,
        started_at: std::time::Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        screenshot_request_counter: 0,
        terminal_ready_sandboxes: HashSet::new(),
    }))
}

fn router_with_sandbox() -> axum::Router {
    build_daemon_router(state_with_sandbox())
}

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let resp = router()
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
async fn list_sandboxes_returns_empty_array() {
    let resp = router()
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
async fn create_sandbox_rejects_unknown_mode() {
    let resp = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/box/create")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"mode": "invalid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn close_nonexistent_returns_404() {
    let resp = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/box/no-such-id/close")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn screenshot_nonexistent_returns_404() {
    let resp = router()
        .oneshot(
            Request::builder()
                .uri("/box/no-such-id/screenshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn screenshot_with_frame_nonexistent_returns_404() {
    let resp = router()
        .oneshot(
            Request::builder()
                .uri("/box/no-such-id/screenshot?with_frame=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let resp = router()
        .oneshot(
            Request::builder()
                .uri("/does/not/exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn screenshot_with_frame_attempts_tab_switch() {
    // with_frame=true should attempt a tab switch before SCK capture.
    // Without a WebSocket connection, the switch fails gracefully and
    // the handler continues to the SCK path (which also fails — no real window).
    // The key assertion: it does NOT return a client error.
    let resp = router_with_sandbox()
        .oneshot(
            Request::builder()
                .uri("/box/test-sb/screenshot?with_frame=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    // SCK path fails with 404 (WindowNotFound) or 500 (Screenshot error),
    // but must NOT be 400 (Bad Request) — proves query param is parsed.
    assert_ne!(
        status,
        StatusCode::BAD_REQUEST,
        "with_frame=true should be parsed, not rejected as bad request"
    );
}

#[tokio::test]
async fn readyz_returns_not_ready_without_renderer() {
    let resp = router()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "not_ready");
    assert_eq!(json["renderer_connected"], false);
}
