# cli-box start Readiness Wait Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When `cli-box start` spawns a new Electron app, wait for the renderer WebSocket to connect before returning, showing animated dots ("正在启动...") to the user.

**Architecture:** Add a `/readyz` endpoint to the daemon that reports whether the renderer WebSocket is connected (`screenshot_ws_tx.is_some()`). Add a `daemon_readiness()` client function. Modify `cmd_start_daemon` to poll this endpoint with animated output when Electron was newly spawned, with a 60-second timeout.

**Tech Stack:** Rust (axum HTTP handler, reqwest client, tokio async), CLI terminal output

---

### Task 1: Add `/readyz` endpoint to daemon

**Files:**
- Modify: `crates/cli-box-core/src/daemon/mod.rs` — add `DaemonReadinessResponse` struct + `readyz_handler` + route
- Test: `crates/cli-box-core/tests/daemon_integration.rs` — add readyz integration test

- [ ] **Step 1: Add `DaemonReadinessResponse` struct**

In `crates/cli-box-core/src/daemon/mod.rs`, after the existing `HealthResponse` struct (around line 196), add:

```rust
#[derive(Debug, Serialize)]
pub struct DaemonReadinessResponse {
    /// "ready" if renderer WebSocket is connected, "not_ready" otherwise.
    pub status: String,
    /// Whether the Electron renderer's screenshot WebSocket is connected.
    pub renderer_connected: bool,
}
```

- [ ] **Step 2: Add `readyz_handler`**

After the `health_handler` function (around line 282), add:

```rust
async fn readyz_handler(State(state): State<Arc<Mutex<DaemonState>>>) -> Json<DaemonReadinessResponse> {
    let s = state.lock().await;
    let renderer_connected = s.screenshot_ws_tx.is_some();
    Json(DaemonReadinessResponse {
        status: if renderer_connected { "ready" } else { "not_ready" }.to_string(),
        renderer_connected,
    })
}
```

- [ ] **Step 3: Register `/readyz` route**

In `build_daemon_router` (around line 243), add the route after the `/health` line:

```rust
        .route("/readyz", get(readyz_handler))
```

- [ ] **Step 4: Add integration test for `/readyz`**

In `crates/cli-box-core/tests/daemon_integration.rs`, add:

```rust
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
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p cli-box-core --test daemon_integration -- readyz`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/cli-box-core/src/daemon/mod.rs crates/cli-box-core/tests/daemon_integration.rs
git commit -m "feat(daemon): add /readyz endpoint for renderer WebSocket status"
```

---

### Task 2: Add `daemon_readiness()` client function

**Files:**
- Modify: `crates/cli-box-cli/src/client.rs` — add `DaemonReadinessResponse` struct + `daemon_readiness()` function

- [ ] **Step 1: Add `DaemonReadinessResponse` and `daemon_readiness()`**

In `crates/cli-box-cli/src/client.rs`, after the existing `DaemonHealthResponse` struct (around line 49), add:

```rust
#[derive(Debug, Deserialize)]
pub struct DaemonReadinessResponse {
    pub status: String,
    pub renderer_connected: bool,
}

/// Check daemon readiness (renderer WebSocket connection status).
pub async fn daemon_readiness() -> Result<DaemonReadinessResponse> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/readyz"))
        .send()
        .await
        .with_context(|| "Failed to connect to daemon readyz endpoint")?;
    let readiness: DaemonReadinessResponse = resp.json().await?;
    Ok(readiness)
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cli-box-cli`
Expected: PASS (existing tests still pass, new function compiles)

- [ ] **Step 3: Commit**

```bash
git add crates/cli-box-cli/src/client.rs
git commit -m "feat(client): add daemon_readiness() for renderer WebSocket status"
```

---

### Task 3: Modify `cmd_start_daemon` to poll with animated dots

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:425-498` — `cmd_start_daemon` function

- [ ] **Step 1: Add readiness polling after Electron spawn**

In `cmd_start_daemon`, after the Electron spawn block (after line 495, before `Ok(())`), add the readiness wait logic. The full modified function ending should be:

```rust
    // Spawn Electron only if not already running.
    // If already running, the renderer polls /box/list and will pick up the new sandbox.
    let electron_newly_spawned = if find_running_electron() {
        tracing::info!("[start] Electron already running, skipping spawn");
        false
    } else if let Some(electron_bin) = find_electron_binary() {
        tracing::info!("[start] spawning Electron: {}", electron_bin.display());
        let _child = Command::new(&electron_bin)
            .spawn()
            .context("Failed to launch Electron app")?;
        tracing::info!("[start] Electron launched");
        true
    } else {
        tracing::warn!("[start] Electron app not found, running in headless daemon mode");
        false
    };

    // Wait for renderer WebSocket to connect if Electron was newly spawned.
    if electron_newly_spawned {
        print!("正在启动");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let timeout = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_secs(1);
        let mut dot_count: u8 = 0;

        loop {
            if start.elapsed() > timeout {
                println!();
                tracing::warn!(
                    "[start] Renderer WebSocket did not connect within {}s, continuing anyway",
                    timeout.as_secs()
                );
                break;
            }

            match client::daemon_readiness().await {
                Ok(resp) if resp.renderer_connected => {
                    println!(" 完成");
                    break;
                }
                _ => {}
            }

            dot_count = (dot_count % 3) + 1;
            print!("\r正在启动{}", ".".repeat(dot_count as usize));
            print!("{}", " ".repeat(3 - dot_count as usize)); // clear leftover dots
            let _ = std::io::stdout().flush();

            tokio::time::sleep(poll_interval).await;
        }
    }

    Ok(())
```

- [ ] **Step 2: Build and verify compilation**

Run: `cargo build -p cli-box-cli`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): poll daemon /readyz with animated dots when Electron is newly spawned"
```

---

### Task 4: Manual smoke test

- [ ] **Step 1: Run `sh test.sh` to verify all existing tests pass**

Run: `sh test.sh`
Expected: All tests pass

- [ ] **Step 2: Run `sh release.sh` to build release binary**

Run: `sh release.sh`
Expected: Release binary built successfully

- [ ] **Step 3: Test the animated dots manually**

```bash
# Kill any existing daemon and electron
release/cli-box daemon stop 2>/dev/null; pkill -f "CLI Box" 2>/dev/null

# Run start — should show "正在启动... 完成"
release/cli-box start zsh
```

Expected: Terminal shows "正在启动" with dots cycling (. .. ... . .. ...) then " 完成" when renderer connects.

- [ ] **Step 4: Test with Electron already running**

```bash
# With Electron already open, start another sandbox
release/cli-box start claude
```

Expected: No animated dots (Electron already running, renderer already connected).

- [ ] **Step 5: Verify screenshots work immediately after start**

```bash
# Start fresh, take screenshot right after
release/cli-box start zsh
sleep 2
release/cli-box screenshot --id $(release/cli-box list --json | jq -r '.[0].id') -o /tmp/test-after-start.png
```

Expected: Screenshot succeeds (renderer WebSocket was connected before command returned).
