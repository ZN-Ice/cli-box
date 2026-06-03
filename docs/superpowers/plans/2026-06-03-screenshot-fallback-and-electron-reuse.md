# Screenshot Fallback Reporting + Electron Window Reuse

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) When screenshot falls back to ScreenCaptureKit, tell the CLI caller why via response headers so the CLI can warn the user. (2) When an Electron window is already running, skip spawning a new one to avoid the visual flash.

**Architecture:** Two independent changes — daemon adds `X-Screenshot-Source` + `X-Screenshot-Fallback-Reason` headers on screenshot responses; CLI reads them and prints a warning. For Electron reuse, the CLI's `cmd_start_daemon` calls the existing `find_running_electron()` helper and skips `Command::new(&electron_bin).spawn()` if it returns `true`.

**Tech Stack:** Rust (axum headers, reqwest response headers), no new dependencies

---

### Task 1: Add fallback headers to daemon screenshot response

**Files:**
- Modify: `crates/cli-box-core/src/daemon/mod.rs:431-510`

The screenshot handler currently returns `(StatusCode::OK, [("content-type", "image/png")], png_data)`. We need to add two extra headers when the ScreenCaptureKit fallback is used:
- `X-Screenshot-Source: renderer` (primary path) or `X-Screenshot-Source: screencapturekit` (fallback)
- `X-Screenshot-Fallback-Reason: <reason>` (only present on fallback)

- [ ] **Step 1: Modify screenshot_handler to include headers**

In `crates/cli-box-core/src/daemon/mod.rs`, change the `screenshot_handler` function. Replace the current return statements with a pattern that tracks which source was used and why.

Replace the entire function body (lines 431-510) with:

```rust
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
    match request_renderer_screenshot(state.clone(), &id).await {
        Ok(png_data) => {
            tracing::info!(
                "[screenshot] sandbox {} captured via renderer ({} bytes)",
                id,
                png_data.len()
            );
            return Ok((
                StatusCode::OK,
                [
                    ("content-type".to_string(), "image/png".to_string()),
                    ("x-screenshot-source".to_string(), "renderer".to_string()),
                ],
                png_data,
            )
                .into_response());
        }
        Err(reason) => {
            tracing::warn!(
                "[screenshot] renderer capture failed for sandbox {}: {}; falling back to ScreenCaptureKit",
                id,
                reason
            );
            // Store reason for use after ScreenCaptureKit capture
            // We'll capture it in the response headers below
            return screenshot_fallback(state, &id, &reason).await;
        }
    }
}

/// Perform ScreenCaptureKit fallback capture and return response with fallback headers.
async fn screenshot_fallback(
    state: Arc<Mutex<DaemonState>>,
    id: &str,
    reason: &str,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(
        "[screenshot] using ScreenCaptureKit fallback for sandbox {} (captures entire window)",
        id
    );

    let window_id = {
        let s = state.lock().await;
        s.sandboxes.get(id).and_then(|sb| sb.window_id)
    };

    let headers = |src: &str| {
        [
            ("content-type".to_string(), "image/png".to_string()),
            ("x-screenshot-source".to_string(), src.to_string()),
            ("x-screenshot-fallback-reason".to_string(), reason.to_string()),
        ]
    };

    if let Some(wid) = window_id {
        let result = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(wid))
            .await
            .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))?;
        match result {
            Ok(png_data) => {
                return Ok((StatusCode::OK, headers("screencapturekit"), png_data).into_response());
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
    let new_wid = tokio::task::spawn_blocking(|| ScreenCapture::find_window_by_title("CLI Box"))
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
    Ok((StatusCode::OK, headers("screencapturekit"), png_data).into_response())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p cli-box-core`
Expected: `Finished` with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/cli-box-core/src/daemon/mod.rs
git commit -m "feat(daemon): add X-Screenshot-Source header to screenshot responses"
```

---

### Task 2: CLI reads fallback headers and warns user

**Files:**
- Modify: `crates/cli-box-cli/src/client.rs:109-124`
- Modify: `crates/cli-box-cli/src/main.rs:638-651`

- [ ] **Step 1: Update daemon_screenshot to return headers**

In `crates/cli-box-cli/src/client.rs`, change `daemon_screenshot` to return the response headers along with the PNG data. Create a struct for the result:

At the top of client.rs (after imports), add:

```rust
/// Result of a screenshot request, including fallback info.
pub struct ScreenshotResult {
    pub png_data: Vec<u8>,
    pub source: Option<String>,
    pub fallback_reason: Option<String>,
}
```

Then change `daemon_screenshot` (lines 109-124) to:

```rust
pub async fn daemon_screenshot(sandbox_id: &str) -> anyhow::Result<ScreenshotResult> {
    let base = daemon_base_url();
    let url = format!("{base}/box/{sandbox_id}/screenshot");

    let resp = HTTP_CLIENT
        .get(&url)
        .send()
        .await
        .context("Failed to send screenshot request to daemon")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Screenshot failed ({}): {}", resp.status(), body);
    }

    let source = resp
        .headers()
        .get("x-screenshot-source")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let fallback_reason = resp
        .headers()
        .get("x-screenshot-fallback-reason")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let png_data = resp.bytes().await?.to_vec();
    Ok(ScreenshotResult {
        png_data,
        source,
        fallback_reason,
    })
}
```

- [ ] **Step 2: Update cmd_screenshot_daemon to display fallback warning**

In `crates/cli-box-cli/src/main.rs`, change `cmd_screenshot_daemon` (lines 638-651) to read the headers and print a warning:

```rust
async fn cmd_screenshot_daemon(sandbox_id: &str, output: &str) -> anyhow::Result<()> {
    let result = client::daemon_screenshot(sandbox_id).await?;

    if result.source.as_deref() == Some("screencapturekit") {
        eprintln!(
            "Warning: screenshot used ScreenCaptureKit fallback (captured entire window).\n  Reason: {}",
            result.fallback_reason.as_deref().unwrap_or("unknown")
        );
        eprintln!("  For terminal-only screenshots, ensure the Electron app is connected.");
    }

    std::fs::write(output, &result.png_data)
        .with_context(|| format!("Failed to write screenshot to {output}"))?;
    println!("Screenshot saved to \"{output}\" ({} bytes)", result.png_data.len());
    Ok(())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p cli-box-cli`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/client.rs crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): warn user when screenshot uses ScreenCaptureKit fallback"
```

---

### Task 3: Skip Electron spawn when already running

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:478-488`

The `find_running_electron()` function already exists at line 1469 (dead code). We just need to call it in `cmd_start_daemon` before spawning Electron.

- [ ] **Step 1: Add find_running_electron call in cmd_start_daemon**

In `crates/cli-box-cli/src/main.rs`, replace lines 478-488:

```rust
    // Spawn Electron — if already running, requestSingleInstanceLock triggers
    // second-instance event which syncs sandboxes and creates tabs.
    if let Ok(electron_bin) = find_electron_binary() {
        tracing::info!("[start] spawning Electron: {}", electron_bin.display());
        let _child = Command::new(&electron_bin)
            .spawn()
            .context("Failed to launch Electron app")?;
        tracing::info!("[start] Electron launched");
    } else {
        tracing::warn!("[start] Electron app not found, running in headless daemon mode");
    }
```

With:

```rust
    // Spawn Electron only if not already running.
    // If already running, the renderer polls /box/list and will pick up the new sandbox.
    if find_running_electron() {
        tracing::info!("[start] Electron already running, skipping spawn");
    } else if let Ok(electron_bin) = find_electron_binary() {
        tracing::info!("[start] spawning Electron: {}", electron_bin.display());
        let _child = Command::new(&electron_bin)
            .spawn()
            .context("Failed to launch Electron app")?;
        tracing::info!("[start] Electron launched");
    } else {
        tracing::warn!("[start] Electron app not found, running in headless daemon mode");
    }
```

- [ ] **Step 2: Remove #[allow(dead_code)] from find_running_electron**

In `crates/cli-box-cli/src/main.rs`, remove the `#[allow(dead_code)]` attribute from line 1469 since the function is now used.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p cli-box-cli`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "fix(cli): skip Electron spawn when already running to avoid window flash"
```

---

### Task 4: End-to-end verification

- [ ] **Step 1: Build release binaries**

```bash
cargo build --release -p cli-box-cli -p cli-box-daemon
cp target/release/cli-box release/cli-box
cp target/release/cli-box-daemon release/cli-box-daemon
```

- [ ] **Step 2: Test screenshot fallback header**

1. Kill any running daemon/Electron: `pkill -x cli-box-daemon; pkill -x "CLI Box"`
2. Start daemon manually: `RUST_LOG=info ./release/cli-box-daemon &`
3. Start a sandbox: `./release/cli-box start zsh`
4. Wait for Electron to connect, then kill the renderer's WebSocket (restart Electron without the daemon restarting)
5. Take screenshot: `./release/cli-box screenshot --id <id> -o test.png`
6. Verify the CLI prints the fallback warning on stderr

- [ ] **Step 3: Test Electron reuse**

1. Start a sandbox: `./release/cli-box start zsh`
2. Verify Electron window appears
3. Start another sandbox: `./release/cli-box start claude`
4. Verify NO new Electron window flashes — only a new tab appears in the existing window
5. Check daemon logs show "Electron already running, skipping spawn"

- [ ] **Step 4: Run all tests**

```bash
cargo test -p cli-box-core
cd electron-app && pnpm typecheck && npx playwright test --config e2e/playwright.config.ts
```

All tests must pass.
