//! Integration tests for the daemon HTTP API.
//!
//! These tests use `tower::ServiceExt::oneshot` to test the daemon router
//! without binding to a real TCP port.

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use sandbox_core::daemon::{build_daemon_router, DaemonState};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;

fn empty_state() -> Arc<Mutex<DaemonState>> {
    Arc::new(Mutex::new(DaemonState {
        port: 0,
        sandboxes: HashMap::new(),
        started_at: std::time::Instant::now(),
    }))
}

fn router() -> axum::Router {
    build_daemon_router(empty_state())
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
async fn create_sandbox_rejects_unknown_mode() {
    let resp = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sandbox/create")
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
                .uri("/sandbox/no-such-id/close")
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
                .uri("/sandbox/no-such-id/screenshot")
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
