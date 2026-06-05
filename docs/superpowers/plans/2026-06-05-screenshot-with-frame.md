# Screenshot --with-frame Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ScreenCaptureKit opt-in — default screenshots never trigger Screen Recording permission; `--with-frame` flag explicitly enables SCK capture.

**Architecture:** The daemon's `screenshot_handler` gains a `with_frame` query param. Default path uses renderer WebSocket only (no SCK fallback). `--with-frame` skips renderer and uses SCK directly, with permission error guidance on failure. The entitlements plist removes the screen-capture declaration.

**Tech Stack:** Rust (axum HTTP handlers, clap CLI args), serde query params

---

### Task 1: Add `--with-frame` flag to CLI screenshot command

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:100-112` (Screenshot enum variant)
- Modify: `crates/cli-box-cli/src/main.rs:247-253` (match arm)
- Modify: `crates/cli-box-cli/src/main.rs:641-665` (cmd_screenshot_daemon)

- [ ] **Step 1: Add `--with-frame` flag to Screenshot command**

In `crates/cli-box-cli/src/main.rs`, add the flag to the `Screenshot` variant (after line 111):

```rust
    /// Take a screenshot of a sandbox window
    Screenshot {
        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: PathBuf,

        /// Sandbox instance ID
        #[arg(long)]
        id: Option<String>,

        /// Window ID to capture (overrides auto-detection)
        #[arg(long)]
        window_id: Option<u32>,

        /// Use ScreenCaptureKit to capture the full window frame (requires Screen Recording permission)
        #[arg(long)]
        with_frame: bool,
    },
```

- [ ] **Step 2: Pass `with_frame` through the match arm**

Update the match arm (around line 247) to destructure and pass `with_frame`:

```rust
        Commands::Screenshot {
            output,
            id,
            window_id: _window_id,
            with_frame,
        } => {
            cmd_screenshot_daemon(&output, id.as_deref(), with_frame).await?;
        }
```

- [ ] **Step 3: Update `cmd_screenshot_daemon` to accept and use `with_frame`**

Change the function signature and logic (line 641):

```rust
async fn cmd_screenshot_daemon(
    output: &std::path::Path,
    id: Option<&str>,
    with_frame: bool,
) -> anyhow::Result<()> {
    let sandbox_id = id.ok_or_else(|| {
        anyhow::anyhow!(
            "--id is required for screenshots. Use: cli-box screenshot --id <sandbox-id>"
        )
    })?;

    let result = client::daemon_screenshot(sandbox_id, with_frame).await?;

    if result.source.as_deref() == Some("screencapturekit") {
        eprintln!(
            "Screenshot captured with ScreenCaptureKit (full window frame)."
        );
    }

    std::fs::write(output, &result.png_data)
        .with_context(|| format!("Failed to write screenshot to {:?}", output))?;
    println!(
        "Screenshot saved to {:?} ({} bytes)",
        output,
        result.png_data.len()
    );
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p cli-box-cli`
Expected: compiles without errors (will have warnings about unused `with_frame` in client until Task 2)

- [ ] **Step 5: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): add --with-frame flag to screenshot command"
```

---

### Task 2: Update client to pass `with_frame` query param

**Files:**
- Modify: `crates/cli-box-cli/src/client.rs:117-146` (daemon_screenshot)

- [ ] **Step 1: Add `with_frame` parameter to `daemon_screenshot`**

In `crates/cli-box-cli/src/client.rs`, change the function signature and URL (line 117):

```rust
pub async fn daemon_screenshot(sandbox_id: &str, with_frame: bool) -> Result<ScreenshotResult> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let url = if with_frame {
        format!("{base}/box/{sandbox_id}/screenshot?with_frame=true")
    } else {
        format!("{base}/box/{sandbox_id}/screenshot")
    };
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| "screenshot request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("screenshot failed (HTTP {status}): {text}");
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

- [ ] **Step 2: Update MCP handler call site**

In `crates/cli-box-cli/src/main.rs`, update the MCP `screenshot_sandbox` handler (around line 1319) to pass `with_frame`:

```rust
            "screenshot_sandbox" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let with_frame = args["with_frame"].as_bool().unwrap_or(false);
                let result = client::daemon_screenshot(id, with_frame).await?;
                let b64 = base64_encode(&result.png_data);
                let mut response = serde_json::json!({ "sandbox_id": id, "image_base64": b64 });
                if let Some(ref source) = result.source {
                    response["screenshot_source"] = serde_json::json!(source);
                }
                if let Some(ref reason) = result.fallback_reason {
                    response["fallback_reason"] = serde_json::json!(reason);
                }
                Ok(response)
            }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p cli-box-cli`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/client.rs crates/cli-box-cli/src/main.rs
git commit -m "feat(client): pass with_frame query param to daemon screenshot API"
```

---

### Task 3: Update daemon screenshot handler to respect `with_frame`

**Files:**
- Modify: `crates/cli-box-core/src/daemon/mod.rs:431-462` (screenshot_handler)
- Modify: `crates/cli-box-core/src/daemon/mod.rs:486-546` (screenshot_fallback)

- [ ] **Step 1: Add query param struct and update handler signature**

In `crates/cli-box-core/src/daemon/mod.rs`, add the query struct before `screenshot_handler` (around line 430):

```rust
#[derive(Deserialize)]
struct ScreenshotQuery {
    #[serde(default)]
    with_frame: bool,
}
```

Update the handler signature to accept the query param:

```rust
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
```

- [ ] **Step 2: Add `screenshot_with_frame` function**

Add a new function after `screenshot_fallback` (around line 546):

```rust
/// Capture a screenshot using ScreenCaptureKit (requires Screen Recording permission).
async fn screenshot_with_frame(
    state: Arc<Mutex<DaemonState>>,
    id: &str,
) -> Result<Response, AppError> {
    let window_id = {
        let s = state.lock().await;
        s.sandboxes.get(id).and_then(|sb| sb.window_id)
    };

    let wid = match window_id {
        Some(wid) => wid,
        None => {
            // Re-discover window
            let new_wid =
                tokio::task::spawn_blocking(|| ScreenCapture::find_window_by_title("CLI Box"))
                    .await
                    .map_err(|e| {
                        AppError::Screenshot(format!("window discovery task failed: {e}"))
                    })??;
            let mut s = state.lock().await;
            if let Some(sb) = s.sandboxes.get_mut(id) {
                sb.window_id = Some(new_wid);
            }
            let registry = InstanceRegistry::default();
            let _ = registry.update_window_id(id, new_wid);
            new_wid
        }
    };

    let result = tokio::task::spawn_blocking(move || ScreenCapture::capture_window(wid))
        .await
        .map_err(|e| AppError::Screenshot(format!("screenshot task failed: {e}")))?;

    match result {
        Ok(png_data) => Ok(screenshot_response(png_data, "screencapturekit", None)),
        Err(e) => Err(AppError::Screenshot(format!(
            "{}. If permission was denied, grant Screen Recording in System Settings → Privacy & Security → Screen Recording.",
            e
        ))),
    }
}
```

- [ ] **Step 3: Keep `screenshot_fallback` as-is**

The existing `screenshot_fallback` function stays unchanged — it's still used by other internal code paths if needed.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p cli-box-core`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/cli-box-core/src/daemon/mod.rs
git commit -m "feat(daemon): add with_frame query param to screenshot handler"
```

---

### Task 4: Update MCP tool schema for `screenshot_sandbox`

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:1194-1203` (MCP tool schema)

- [ ] **Step 1: Add `with_frame` to tool schema**

In the `mcp_tools()` function, update the `screenshot_sandbox` tool definition:

```rust
            {
                "name": "screenshot_sandbox",
                "description": "Take a screenshot of a sandbox (returns base64 PNG). Default: renderer capture (no permission needed). Use with_frame=true for full window capture (requires Screen Recording permission).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" },
                        "with_frame": { "type": "boolean", "description": "Use ScreenCaptureKit for full window frame capture (requires Screen Recording permission)", "default": false }
                    },
                    "required": ["sandbox_id"]
                }
            },
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p cli-box-cli`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(mcp): add with_frame parameter to screenshot_sandbox tool"
```

---

### Task 5: Remove screen-capture entitlement

**Files:**
- Modify: `entitlements.plist`

- [ ] **Step 1: Remove screen-capture entitlement**

Remove lines 9-10 from `entitlements.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.cs.allow-unsigned-executable-memory</key>
	<true/>
	<key>com.apple.security.cs.disable-library-validation</key>
	<true/>
</dict>
</plist>
```

- [ ] **Step 2: Commit**

```bash
git add entitlements.plist
git commit -m "fix(entitlements): remove screen-capture entitlement"
```

---

### Task 6: Update legacy server screenshot handler

**Files:**
- Modify: `crates/cli-box-core/src/server/mod.rs:402-414` (screenshot_handler)

- [ ] **Step 1: Add `with_frame` query param to legacy server**

In `crates/cli-box-core/src/server/mod.rs`, the existing `ScreenshotQuery` already exists (used for `window_id`). Add `with_frame` to it and update the handler:

First, find the existing `ScreenshotQuery` struct (search for it in the file) and add the field:

```rust
#[derive(Deserialize)]
struct ScreenshotQuery {
    window_id: Option<u32>,
    #[serde(default)]
    with_frame: bool,
}
```

Update the handler to respect `with_frame`:

```rust
async fn screenshot_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(q): Query<ScreenshotQuery>,
) -> Result<impl IntoResponse, AppError> {
    let window_id = q.window_id.or(state.lock().await.window_id);
    match window_id {
        Some(id) => {
            if !q.with_frame {
                return Err(AppError::Screenshot(
                    "Default screenshot is not supported in legacy mode. Use ?with_frame=true (requires Screen Recording permission).".to_string()
                ));
            }
            let png_data = ScreenCapture::capture_window(id)?;
            Ok((StatusCode::OK, [("content-type", "image/png")], png_data).into_response())
        }
        None => Err(AppError::BadRequest(SANDBOX_WINDOW_REQUIRED.to_string())),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p cli-box-core`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/cli-box-core/src/server/mod.rs
git commit -m "feat(server): add with_frame query param to legacy screenshot handler"
```

---

### Task 7: Verify full build and tests

**Files:** None (verification only)

- [ ] **Step 1: Run full cargo check**

Run: `cargo check --all-targets`
Expected: compiles without errors

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets`
Expected: no errors (warnings acceptable)

- [ ] **Step 3: Run existing tests**

Run: `cargo test --all`
Expected: all tests pass

- [ ] **Step 4: Run frontend checks**

Run: `pnpm typecheck && pnpm format:check && pnpm test:unit`
Expected: all pass

- [ ] **Step 5: Commit any fixes**

If any fixes were needed, commit them:

```bash
git add -A
git commit -m "fix: address review feedback from build verification"
```
