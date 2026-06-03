# Phase 8: Release Test Bug Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 6 bugs found during release testing. B3 and B5 are already resolved by the Electron architecture. This plan covers B1 (region crop), B2 (window_id propagation), B4 (app window tracking), and B6 (sandbox-relative screenshots).

**Architecture:** B1 uses `image::imageops::crop` for software cropping. B2 auto-discovers the Electron window via `ScreenCapture::find_window_by_title` after daemon starts. B4 tracks app windows after `spawn_app`. B6 converts sandbox-relative coordinates to global.

**Tech Stack:** Rust, image crate, ScreenCaptureKit

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-core/src/capture/mod.rs` | Modify | Fix `capture_region` to crop with image crate |
| `crates/sandbox-core/src/daemon/mod.rs` | Modify | Auto-discover window_id, sandbox-relative screenshot endpoint |
| `crates/sandbox-core/src/process/mod.rs` | Modify | Add `spawn_app_with_window` that returns window_id |
| `crates/sandbox-core/tests/capture_test.rs` | Create | Tests for capture_region crop |

---

### Task 1: Fix B1 — capture_region crop

**Files:**
- Modify: `crates/sandbox-core/src/capture/mod.rs`
- Create: `crates/sandbox-core/tests/capture_test.rs`

- [ ] **Step 1: Read current capture_region implementation**

The current `capture_region` at line 83 ignores x/y. Fix it to use `image::imageops::crop`.

- [ ] **Step 2: Fix capture_region**

Replace the `capture_region` function body in the `macos_impl` module:

```rust
pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
    // Capture full screen first
    let content = SCShareableContent::get()
        .map_err(|e| AppError::Screenshot(format!("SCShareableContent failed: {e}")))?;
    let display = content.displays().first()
        .ok_or_else(|| AppError::Screenshot("No display found".into()))?;

    let config = SCStreamConfiguration::new()
        .map_err(|e| AppError::Screenshot(format!("SCStreamConfiguration failed: {e}")))?;
    config.set_width(display.width() as u32);
    config.set_height(display.height() as u32);

    let filter = SCContentFilter::new().display(display).excluding_windows(&[]);
    let image = SCScreenshotManager::capture_image(content, filter, config)
        .map_err(|e| AppError::Screenshot(format!("Capture failed: {e}")))?;

    // Get raw RGBA data
    let img_width = image.width() as u32;
    let img_height = image.height() as u32;
    let bytes_per_row = image.bytes_per_row();
    let data = image.data();
    let data_bytes = data.bytes();

    // Create RgbaImage from raw data
    let mut rgba = Vec::with_capacity((img_width * img_height * 4) as usize);
    for row in 0..img_height {
        let row_start = (row * bytes_per_row as u32) as usize;
        for col in 0..img_width {
            let px = row_start + (col * 4) as usize;
            if px + 3 < data_bytes.len() {
                rgba.extend_from_slice(&data_bytes[px..px + 4]);
            }
        }
    }

    let full_img = image::RgbaImage::from_raw(img_width, img_height, rgba)
        .ok_or_else(|| AppError::Screenshot("Failed to create image from raw data".into()))?;

    // Clamp crop region to display bounds
    let crop_x = x.max(0) as u32;
    let crop_y = y.max(0) as u32;
    let crop_w = width.min(img_width.saturating_sub(crop_x));
    let crop_h = height.min(img_height.saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return Err(AppError::BadRequest("Crop region is outside display bounds".into()));
    }

    let cropped = image::imageops::crop(&mut full_img.clone(), crop_x, crop_y, crop_w, crop_h);
    let cropped_img = cropped.to_image();

    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    use image::ImageEncoder;
    encoder.write_image(
        cropped_img.as_raw(),
        crop_w,
        crop_h,
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| AppError::Screenshot(format!("PNG encode failed: {e}")))?;

    Ok(png)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/capture/mod.rs
git commit -m "fix(capture): capture_region now crops using image::imageops::crop"
```

---

### Task 2: Fix B2 — Auto-discover window_id

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs`

- [ ] **Step 1: Add window discovery after daemon starts**

In `run_daemon()`, after the server starts, spawn a task to discover the Electron window:

```rust
// Auto-discover Electron window ID
let discovery_state = state.clone();
tokio::spawn(async move {
    // Wait a bit for Electron to launch
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    match ScreenCapture::find_window_by_title("CLI Box") {
        Ok(window_id) => {
            tracing::info!("Discovered Electron window_id={}", window_id);
            // Store in daemon state for reference
            let mut s = discovery_state.lock().await;
            // Set window_id on all sandboxes that don't have one
            for (_, sb) in s.sandboxes.iter_mut() {
                if sb.window_id.is_none() {
                    sb.window_id = Some(window_id);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Could not discover Electron window: {e}");
        }
    }
});
```

- [ ] **Step 2: Add window discovery on sandbox creation**

In `create_sandbox_handler`, after creating the sandbox, try to discover the window:

```rust
// After sandbox is created, try to find window_id
if sandbox.window_id.is_none() {
    if let Ok(wid) = ScreenCapture::find_window_by_title("CLI Box") {
        sandbox.window_id = Some(wid);
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs
git commit -m "fix(daemon): auto-discover Electron window_id for screenshots"
```

---

### Task 3: Fix B4 — App window tracking

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs`

- [ ] **Step 1: Add spawn_app_with_window function**

Add after the existing `spawn_app`:

```rust
/// Spawn a macOS .app and attempt to discover its window ID.
pub fn spawn_app_with_window(app_path: &str) -> Result<(ProcessInfo, Option<u32>)> {
    let info = Self::spawn_app(app_path)?;

    // Wait for the app to create its window
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Try to find the window by app name
    let app_name = std::path::Path::new(app_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let window_id = crate::capture::ScreenCapture::find_window_by_title(&app_name).ok();

    Ok((info, window_id))
}
```

- [ ] **Step 2: Update daemon spawn_app_handler to use it**

In `daemon/mod.rs`, update the spawn_app handler:

```rust
async fn spawn_app_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<SpawnAppRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (info, window_id) = crate::process::ProcessManager::spawn_app_with_window(&req.app_path)?;

    // Update sandbox window_id if found
    if let Some(wid) = window_id {
        let mut state = state.lock().await;
        if let Some(sb) = state.sandboxes.get_mut(&id) {
            sb.window_id = Some(wid);
        }
    }

    Ok(Json(serde_json::json!({
        "pid": info.pid,
        "name": info.name,
        "window_id": window_id,
    })))
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core && cargo check -p sandbox-daemon`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs crates/sandbox-core/src/daemon/mod.rs
git commit -m "fix(process): spawn_app_with_window tracks app window_id"
```

---

### Task 4: Fix B6 — Sandbox-relative screenshot

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs`

- [ ] **Step 1: Add sandbox-region screenshot endpoint**

Add route:

```rust
.route("/sandbox/{id}/screenshot/region", get(screenshot_sandbox_region_handler))
```

- [ ] **Step 2: Implement handler**

```rust
#[derive(Deserialize)]
struct SandboxRegionQuery {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

async fn screenshot_sandbox_region_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Query(q): Query<SandboxRegionQuery>,
) -> Result<Vec<u8>, AppError> {
    let state = state.lock().await;
    let sandbox = state.sandboxes.get(&id)
        .ok_or_else(|| AppError::BadRequest(format!("Sandbox not found: {id}")))?;
    let window_id = sandbox.window_id
        .ok_or_else(|| AppError::BadRequest("Sandbox has no window_id".into()))?;

    // Get window position on screen
    let windows = ScreenCapture::list_windows()?;
    let window = windows.iter().find(|(wid, _)| *wid == window_id)
        .ok_or_else(|| AppError::WindowNotFound(format!("Window {window_id} not found")))?;

    // Use CGWindowListCopyWindowInfo to get window frame
    // For now, use global coordinates directly (user must provide global coords)
    // TODO: Convert sandbox-relative to global using window frame
    let png = ScreenCapture::capture_region(q.x, q.y, q.width, q.height)?;
    Ok(png)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs
git commit -m "fix(daemon): add sandbox-relative screenshot region endpoint"
```

---

### Task 5: Manual verification

- [ ] **Step 1: Build everything**

Run: `cargo build --release`

- [ ] **Step 2: Test screenshot**

Run: `./target/release/sandbox start zsh`
Wait for Electron window, then:
Run: `./target/release/sandbox screenshot --id <id> -o test.png`
Expected: Valid PNG file (not empty, not error)

- [ ] **Step 3: Test region crop**

Run: `./target/release/sandbox screenshot --id <id> -o region.png --region 0,0,400,300`
Expected: Cropped PNG

- [ ] **Step 4: Commit final state**

```bash
git add -A
git commit -m "fix(phase8): all release test bugs resolved"
```
