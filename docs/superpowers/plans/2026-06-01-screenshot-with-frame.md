# Screenshot --with-frame Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--with-frame` flag to `sandbox screenshot` command that captures the full macOS window (title bar, traffic lights, tab bar, status bar) by briefly switching the active tab and using ScreenCaptureKit.

**Architecture:** A WebSocket bridge between daemon and renderer enables cooperative tab switching. When `--with-frame` is specified, the daemon tells the renderer to switch to the target tab, waits for confirmation, captures the full Electron window with ScreenCaptureKit, then tells the renderer to switch back. The switch happens in <16ms, invisible to the user. Falls back to terminal-only capture if the renderer is unavailable or the window is minimized.

**Tech Stack:** Rust (axum WebSocket, tokio oneshot), TypeScript (React, xterm.js), ScreenCaptureKit

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-cli/src/main.rs` | Modify | Add `--with-frame` flag to Screenshot command |
| `crates/sandbox-cli/src/client.rs` | Modify | Pass `with_frame` query param to daemon |
| `crates/sandbox-core/src/daemon/mod.rs` | Modify | Add WebSocket bridge, modify screenshot_handler |
| `crates/sandbox-core/tests/daemon_integration.rs` | Modify | Update DaemonState construction |
| `electron-app/src/renderer/components/Terminal.tsx` | Modify | Add forwardRef + captureToPng |
| `electron-app/src/renderer/main.tsx` | Modify | Add terminalRefs, screenshot WebSocket, tab switching |

## WebSocket Protocol

```
Daemon → Renderer: {"type":"switch_and_capture","request_id":N,"sandbox_id":"..."}
Renderer → Daemon: {"type":"tab_switched","request_id":N}
Daemon: ScreenCaptureKit captures window
Daemon → Renderer: {"type":"capture_done","request_id":N}
Renderer: switches back to original tab
Renderer → Daemon: {"type":"capture_response","request_id":N,"sandbox_id":"...","image_base64":"..."}
```

---

### Task 1: Add `--with-frame` flag to CLI

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs:99-112`
- Modify: `crates/sandbox-cli/src/main.rs:247-253`
- Modify: `crates/sandbox-cli/src/main.rs:639-651`
- Modify: `crates/sandbox-cli/src/client.rs:108-124`

- [ ] **Step 1: Add `--with-frame` arg to Screenshot command**

In `crates/sandbox-cli/src/main.rs`, modify the `Screenshot` variant (lines 99-112):

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

    /// Capture full macOS window frame (title bar, tabs, status bar)
    #[arg(long)]
    with_frame: bool,
},
```

- [ ] **Step 2: Pass `with_frame` through dispatch**

In `crates/sandbox-cli/src/main.rs`, modify the dispatch (lines 247-253):

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

- [ ] **Step 3: Pass `with_frame` to client function**

In `crates/sandbox-cli/src/main.rs`, modify `cmd_screenshot_daemon` (lines 639-651):

```rust
async fn cmd_screenshot_daemon(
    output: &std::path::Path,
    id: Option<&str>,
    with_frame: bool,
) -> anyhow::Result<()> {
    let sandbox_id = id.ok_or_else(|| {
        anyhow::anyhow!("--id is required for screenshots. Use: sandbox screenshot --id <sandbox-id>")
    })?;
    let png = client::daemon_screenshot(sandbox_id, with_frame).await?;
    std::fs::write(output, &png)
        .with_context(|| format!("Failed to write screenshot to {:?}", output))?;
    println!("Screenshot saved to {:?} ({} bytes)", output, png.len());
    Ok(())
}
```

- [ ] **Step 4: Add `with_frame` param to client function**

In `crates/sandbox-cli/src/client.rs`, modify `daemon_screenshot` (lines 108-124):

```rust
/// Take a screenshot of a sandbox via the daemon HTTP API. Returns PNG bytes.
pub async fn daemon_screenshot(sandbox_id: &str, with_frame: bool) -> Result<Vec<u8>> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let url = if with_frame {
        format!("{base}/sandbox/{sandbox_id}/screenshot?with_frame=true")
    } else {
        format!("{base}/sandbox/{sandbox_id}/screenshot")
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
    let bytes = resp.bytes().await?.to_vec();
    Ok(bytes)
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p sandbox-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-cli/src/main.rs crates/sandbox-cli/src/client.rs
git commit -m "feat(cli): add --with-frame flag to screenshot command"
```

---

### Task 2: Add WebSocket screenshot bridge to daemon

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs:46-50` (DaemonState)
- Modify: `crates/sandbox-core/src/daemon/mod.rs:812-816` (startup init)
- Modify: `crates/sandbox-core/src/daemon/mod.rs:228-263` (routes)
- Modify: `crates/sandbox-core/src/daemon/mod.rs:417-443` (screenshot_handler)
- Modify: `crates/sandbox-core/src/daemon/mod.rs:1002-1031` (test helpers)
- Modify: `crates/sandbox-core/tests/daemon_integration.rs:14-20` (test state)

- [ ] **Step 1: Add WebSocket fields to DaemonState**

In `crates/sandbox-core/src/daemon/mod.rs`, modify `DaemonState` (lines 46-50):

```rust
/// Shared state for the daemon.
pub struct DaemonState {
    pub port: u16,
    pub sandboxes: HashMap<String, ManagedSandbox>,
    pub started_at: Instant,
    /// Write half of the renderer's screenshot WebSocket connection.
    pub screenshot_ws_tx: Option<futures_util::stream::SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>>,
    /// Pending screenshot requests awaiting renderer responses.
    pub pending_screenshots: HashMap<u64, tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>>,
    /// Counter for generating unique request IDs.
    pub screenshot_request_counter: u64,
}
```

- [ ] **Step 2: Add WebSocket route**

In `crates/sandbox-core/src/daemon/mod.rs`, add route after line 261 (before `.layer(cors)`):

```rust
.route("/screenshot/ws", get(screenshot_ws_upgrade_handler))
```

- [ ] **Step 3: Add with_frame query param to screenshot_handler**

In `crates/sandbox-core/src/daemon/mod.rs`, add a query struct and modify `screenshot_handler` (lines 417-443):

```rust
#[derive(Deserialize)]
struct ScreenshotQuery {
    #[serde(default)]
    with_frame: bool,
}

async fn screenshot_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<ScreenshotQuery>,
) -> Result<impl IntoResponse, AppError> {
    let window_id = {
        let s = state.lock().await;
        let sandbox = s
            .sandboxes
            .get(&id)
            .ok_or_else(|| AppError::Instance(format!("Sandbox '{id}' not found")))?;
        sandbox.window_id
    };

    // --with-frame: try renderer-based tab switch + ScreenCaptureKit
    if q.with_frame {
        if let Some(png) = request_renderer_screenshot_with_frame(state.clone(), &id).await {
            return Ok((StatusCode::OK, [("content-type", "image/png")], png).into_response());
        }
        // Fall through to ScreenCaptureKit if renderer unavailable
    }

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
```

- [ ] **Step 4: Add WebSocket upgrade handler**

Add after the `screenshot_handler` function:

```rust
async fn screenshot_ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<Mutex<DaemonState>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_screenshot_ws(socket, state))
}
```

- [ ] **Step 5: Add WebSocket message handler**

Add after the upgrade handler:

```rust
async fn handle_screenshot_ws(
    socket: axum::extract::ws::WebSocket,
    state: Arc<Mutex<DaemonState>>,
) {
    use futures_util::{SinkExt, StreamExt};
    let (ws_tx, mut ws_rx) = socket.split();

    // Store the write half
    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }
    tracing::info!("Renderer screenshot WebSocket connected");

    while let Some(msg) = ws_rx.next().await {
        let msg = match msg {
            Ok(axum::extract::ws::Message::Text(t)) => t,
            Ok(_) => continue,
            Err(e) => {
                tracing::warn!("Screenshot WS error: {e}");
                break;
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Screenshot WS parse error: {e}");
                continue;
            }
        };

        let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let request_id = parsed.get("request_id").and_then(|v| v.as_u64());

        match msg_type {
            "tab_switched" => {
                // Renderer has switched to target tab, notify the waiting handler
                if let Some(rid) = request_id {
                    let mut s = state.lock().await;
                    if let Some(tx) = s.pending_tab_switches.remove(&rid) {
                        let _ = tx.send(Ok(()));
                    }
                }
            }
            "capture_response" => {
                if let Some(rid) = request_id {
                    let base64_data = parsed
                        .get("image_base64")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let result = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        base64_data,
                    )
                    .map_err(|e| format!("base64 decode error: {e}"));
                    let mut s = state.lock().await;
                    if let Some(tx) = s.pending_screenshots.remove(&rid) {
                        let _ = tx.send(result);
                    }
                }
            }
            "capture_error" => {
                if let Some(rid) = request_id {
                    let error = parsed
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error")
                        .to_string();
                    let mut s = state.lock().await;
                    if let Some(tx) = s.pending_screenshots.remove(&rid) {
                        let _ = tx.send(Err(error));
                    }
                }
            }
            _ => {
                tracing::debug!("Unknown screenshot WS message type: {msg_type}");
            }
        }
    }

    // Clean up on disconnect
    let mut s = state.lock().await;
    s.screenshot_ws_tx = None;
    tracing::info!("Renderer screenshot WebSocket disconnected");
}
```

- [ ] **Step 6: Add `pending_tab_switches` to DaemonState**

Update the DaemonState struct to include the new field:

```rust
pub struct DaemonState {
    pub port: u16,
    pub sandboxes: HashMap<String, ManagedSandbox>,
    pub started_at: Instant,
    pub screenshot_ws_tx: Option<futures_util::stream::SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>>,
    pub pending_screenshots: HashMap<u64, tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>>,
    pub pending_tab_switches: HashMap<u64, tokio::sync::oneshot::Sender<Result<(), String>>>,
    pub screenshot_request_counter: u64,
}
```

- [ ] **Step 7: Add `request_renderer_screenshot_with_frame` function**

Add after the WebSocket handler:

```rust
/// Ask the renderer to switch to a tab, then capture the full window with ScreenCaptureKit.
async fn request_renderer_screenshot_with_frame(
    state: Arc<Mutex<DaemonState>>,
    sandbox_id: &str,
) -> Option<Vec<u8>> {
    use futures_util::SinkExt;

    // Get the Electron window_id for ScreenCaptureKit
    let window_id = {
        let s = state.lock().await;
        s.sandboxes.get(sandbox_id)?.window_id?
    };

    // Take ws_tx and generate request_id
    let (mut ws_tx, request_id) = {
        let mut s = state.lock().await;
        let ws_tx = s.screenshot_ws_tx.take()?;
        let rid = s.screenshot_request_counter;
        s.screenshot_request_counter += 1;
        (ws_tx, rid)
    };

    // Create oneshot channels
    let (switch_tx, switch_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let (capture_tx, capture_rx) = tokio::sync::oneshot::channel::<Result<Vec<u8>, String>>();
    {
        let mut s = state.lock().await;
        s.pending_tab_switches.insert(request_id, switch_tx);
        s.pending_screenshots.insert(request_id, capture_tx);
    }

    // Send switch_and_capture to renderer
    let msg = serde_json::json!({
        "type": "switch_and_capture",
        "request_id": request_id,
        "sandbox_id": sandbox_id,
    });
    if ws_tx
        .send(axum::extract::ws::Message::Text(msg.to_string()))
        .await
        .is_err()
    {
        let mut s = state.lock().await;
        s.pending_tab_switches.remove(&request_id);
        s.pending_screenshots.remove(&request_id);
        s.screenshot_ws_tx = Some(ws_tx);
        return None;
    }

    // Put ws_tx back
    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }

    // Wait for renderer to confirm tab switch (2s timeout)
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), switch_rx).await;

    // Capture with ScreenCaptureKit
    let png_data = tokio::task::spawn_blocking(move || {
        crate::capture::ScreenCapture::capture_window(window_id)
    })
    .await
    .ok()
    .and_then(|r| r.ok());

    let png_data = match png_data {
        Some(data) => data,
        None => {
            // Clean up pending capture request
            let mut s = state.lock().await;
            s.pending_screenshots.remove(&request_id);
            return None;
        }
    };

    // Send capture_done to renderer (tells it to switch back)
    let mut ws_tx = {
        let mut s = state.lock().await;
        match s.screenshot_ws_tx.take() {
            Some(tx) => tx,
            None => {
                s.pending_screenshots.remove(&request_id);
                return Some(png_data);
            }
        }
    };
    let done_msg = serde_json::json!({
        "type": "capture_done",
        "request_id": request_id,
    });
    let _ = ws_tx
        .send(axum::extract::ws::Message::Text(done_msg.to_string()))
        .await;
    {
        let mut s = state.lock().await;
        s.screenshot_ws_tx = Some(ws_tx);
    }

    // Don't wait for capture_response — we already have the PNG from ScreenCaptureKit
    let mut s = state.lock().await;
    s.pending_screenshots.remove(&request_id);

    Some(png_data)
}
```

- [ ] **Step 8: Add base64 dependency**

Check if `base64` is already in `crates/sandbox-core/Cargo.toml`:

```bash
grep -n "base64" crates/sandbox-core/Cargo.toml
```

If not present, add it:

```bash
cargo add base64 -p sandbox-core
```

- [ ] **Step 9: Update startup DaemonState initialization**

In `crates/sandbox-core/src/daemon/mod.rs`, update the startup init (lines 812-816):

```rust
let state = Arc::new(Mutex::new(DaemonState {
    port,
    sandboxes: HashMap::new(),
    started_at: Instant::now(),
    screenshot_ws_tx: None,
    pending_screenshots: HashMap::new(),
    pending_tab_switches: HashMap::new(),
    screenshot_request_counter: 0,
}));
```

- [ ] **Step 10: Update test DaemonState helpers**

In `crates/sandbox-core/src/daemon/mod.rs`, update `test_daemon_state` (lines 1002-1008):

```rust
fn test_daemon_state() -> Arc<Mutex<DaemonState>> {
    Arc::new(Mutex::new(DaemonState {
        port: 15999,
        sandboxes: HashMap::new(),
        started_at: Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        pending_tab_switches: HashMap::new(),
        screenshot_request_counter: 0,
    }))
}
```

Update `test_daemon_state_with_sandbox` (lines 1010-1031):

```rust
fn test_daemon_state_with_sandbox() -> Arc<Mutex<DaemonState>> {
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
        port: 15999,
        sandboxes,
        started_at: Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        pending_tab_switches: HashMap::new(),
        screenshot_request_counter: 0,
    }))
}
```

- [ ] **Step 11: Update integration test DaemonState**

In `crates/sandbox-core/tests/daemon_integration.rs`, update `empty_state` (lines 14-20):

```rust
fn empty_state() -> Arc<Mutex<DaemonState>> {
    Arc::new(Mutex::new(DaemonState {
        port: 0,
        sandboxes: HashMap::new(),
        started_at: std::time::Instant::now(),
        screenshot_ws_tx: None,
        pending_screenshots: HashMap::new(),
        pending_tab_switches: HashMap::new(),
        screenshot_request_counter: 0,
    }))
}
```

- [ ] **Step 12: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 13: Run tests**

Run: `cargo test -p sandbox-core`
Expected: PASS (all existing tests still pass)

- [ ] **Step 14: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs crates/sandbox-core/tests/daemon_integration.rs
git commit -m "feat(daemon): add WebSocket screenshot bridge for --with-frame"
```

---

### Task 3: Add forwardRef + captureToPng to Terminal.tsx

**Files:**
- Modify: `electron-app/src/renderer/components/Terminal.tsx:1-18`

- [ ] **Step 1: Convert to forwardRef with captureToPng**

Replace the imports and function signature in `electron-app/src/renderer/components/Terminal.tsx`:

```typescript
import { useEffect, useRef, useCallback, forwardRef, useImperativeHandle } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { connectPty } from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  sandboxId: string;
  ptyPid: number;
  onReady?: (cols: number, rows: number) => void;
}

export interface SandboxTerminalHandle {
  captureToPng(): Promise<string>;
}

const SandboxTerminal = forwardRef<SandboxTerminalHandle, TerminalProps>(function SandboxTerminal(
  { sandboxId, ptyPid, onReady },
  ref
) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const fitFnRef = useRef<(() => void) | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);

  useImperativeHandle(ref, () => ({
    async captureToPng(): Promise<string> {
      const term = xtermRef.current;
      if (!term) throw new Error("Terminal not initialized");

      // Try canvas first (works for active/visible tabs)
      const canvasEl = term.element?.querySelector("canvas");
      if (canvasEl) {
        const dataUrl = canvasEl.toDataURL("image/png");
        return dataUrl.split(",")[1];
      }

      // Fallback: render xterm buffer to canvas (works for hidden/offscreen tabs)
      const cols = term.cols;
      const rows = term.rows;
      const fontSize = 13;
      const lineHeight = Math.ceil(fontSize * 1.4);
      const charWidth = Math.ceil(fontSize * 0.6);
      const canvas = document.createElement("canvas");
      canvas.width = cols * charWidth;
      canvas.height = rows * lineHeight;
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#1a1a1a";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.font = `${fontSize}px "SF Mono", "Menlo", "Monaco", monospace`;
      ctx.textBaseline = "top";

      const buffer = term.buffer.active;
      for (let y = 0; y < rows; y++) {
        const line = buffer.getLine(y);
        if (!line) continue;
        for (let x = 0; x < line.length; x++) {
          const char = line.getCell(x)?.getChars() || " ";
          const fg = line.getCell(x)?.getFgColor();
          if (fg && fg !== 0) {
            ctx.fillStyle = `rgb(${(fg >> 16) & 0xff},${(fg >> 8) & 0xff},${fg & 0xff})`;
          } else {
            ctx.fillStyle = "#cccccc";
          }
          ctx.fillText(char, x * charWidth, y * lineHeight);
        }
      }
      return canvas.toDataURL("image/png").split(",")[1];
    },
  }), []);
```

Keep the rest of the component (useEffect hooks, return JSX) unchanged. At the end of the file, change:

```typescript
export default SandboxTerminal;
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd electron-app && pnpm typecheck`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/renderer/components/Terminal.tsx
git commit -m "feat(terminal): add forwardRef with captureToPng for per-tab screenshots"
```

---

### Task 4: Add terminalRefs and screenshot WebSocket to main.tsx

**Files:**
- Modify: `electron-app/src/renderer/main.tsx:1-4` (imports)
- Modify: `electron-app/src/renderer/main.tsx:35-45` (state)
- Modify: `electron-app/src/renderer/main.tsx:59-66` (init useEffect)
- Modify: `electron-app/src/renderer/main.tsx:183-205` (terminal rendering)

- [ ] **Step 1: Add imports**

In `electron-app/src/renderer/main.tsx`, update imports (lines 1-12):

```typescript
import { useState, useEffect, useCallback, useRef } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal, { SandboxTerminalHandle } from "./components/Terminal";
import {
  SandboxInfo,
  fetchSandboxList,
  setDaemonPort,
  getDaemonPort,
  createSandbox,
} from "./api";
import AppPanel from "./components/AppPanel";
import "./styles.css";
```

- [ ] **Step 2: Add terminalRefs**

In the `App` function, add after the existing refs (around line 45):

```typescript
const terminalRefs = useRef<Map<string, React.RefObject<SandboxTerminalHandle>>>(new Map());
```

- [ ] **Step 3: Add screenshot WebSocket useEffect**

Add a new useEffect after the existing init useEffect (after line 66):

**Important:** `activeTabId` is stale in the WebSocket closure. We need two refs:

```typescript
const activeTabIdRef = useRef<string | null>(null);
const prevActiveTabRef = useRef<string | null>(null);

// Keep ref in sync with state
useEffect(() => { activeTabIdRef.current = activeTabId; }, [activeTabId]);
```

The full corrected WebSocket handler:

```typescript
ws.onmessage = async (event) => {
  try {
    const msg = JSON.parse(event.data);
    if (msg.type === "switch_and_capture") {
      const { sandbox_id, request_id } = msg;
      // Save current tab so we can restore after capture
      prevActiveTabRef.current = activeTabIdRef.current;
      // Switch to target tab
      setActiveTabId(sandbox_id);
      // Wait for React to render (2 animation frames + small delay for xterm)
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => resolve());
        });
      });
      await new Promise((r) => setTimeout(r, 50));
      // Tell daemon we're ready
      ws.send(JSON.stringify({ type: "tab_switched", request_id, sandbox_id }));
    } else if (msg.type === "capture_done") {
      // Daemon has captured, restore previous tab
      if (prevActiveTabRef.current) {
        setActiveTabId(prevActiveTabRef.current);
        prevActiveTabRef.current = null;
      }
    }
  } catch (err) {
    console.error("[screenshot-ws] parse error:", err);
  }
};
```

- [ ] **Step 4: Update terminal rendering to mount all tabs**

Replace the terminal rendering section (lines 183-205) to mount all tabs with CSS hiding:

```typescript
{/* Terminal Area */}
{tabs.length === 0 ? (
  <div className="empty-state">
    <div className="empty-state-icon">⌘</div>
    <div className="empty-state-text">No sandbox open</div>
    <div className="empty-state-hint">
      Run <code>sandbox start</code> in your terminal to get started
    </div>
  </div>
) : (
  <div className="terminal-area">
    {tabs.map((tab) => {
      const isActive = tab.id === activeTabId;
      const hiddenStyle: React.CSSProperties = isActive
        ? {}
        : {
            position: "absolute",
            left: "-9999px",
            top: "-9999px",
            width: "1200px",
            height: "800px",
            visibility: "hidden",
          };

      if (tab.kind === "app") {
        return (
          <div key={tab.id} className="terminal-container" style={hiddenStyle}>
            <AppPanel sandboxId={tab.id} />
          </div>
        );
      }

      if (!terminalRefs.current.has(tab.id)) {
        terminalRefs.current.set(tab.id, { current: null } as React.RefObject<SandboxTerminalHandle>);
      }
      const tabRef = terminalRefs.current.get(tab.id)!;

      return (
        <div key={tab.id} style={{ ...hiddenStyle, display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
          <SandboxTerminal ref={tabRef} sandboxId={tab.id} ptyPid={tab.sandbox.pty_pid!} />
        </div>
      );
    })}
  </div>
)}
```

- [ ] **Step 5: Update handleCloseTab to clean up refs**

In the `handleCloseTab` callback, add cleanup:

```typescript
terminalRefs.current.delete(id);
```

- [ ] **Step 6: Add `.terminal-area` CSS**

In `electron-app/src/renderer/styles.css`, ensure `.terminal-area` has proper flex layout:

```css
.terminal-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  min-height: 0;
}
```

- [ ] **Step 7: Verify TypeScript compilation**

Run: `cd electron-app && pnpm typecheck`
Expected: PASS

- [ ] **Step 8: Verify full build**

Run: `cd electron-app && pnpm build`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add electron-app/src/renderer/main.tsx electron-app/src/renderer/styles.css electron-app/src/renderer/components/Terminal.tsx
git commit -m "feat(renderer): add terminalRefs and screenshot WebSocket for --with-frame"
```

---

### Task 5: End-to-end verification

- [ ] **Step 1: Build everything**

```bash
cargo build -p sandbox-cli
cd electron-app && pnpm build && cd ..
```

- [ ] **Step 2: Run all tests**

```bash
cargo test -p sandbox-core
cd electron-app && pnpm test:unit && cd ..
```

- [ ] **Step 3: Manual test — default screenshot**

```bash
# Start a sandbox
./sandbox start zsh
# Take a default screenshot (ScreenCaptureKit, active tab)
./sandbox screenshot --id <id> -o default.png
# Verify: shows terminal content only (no macOS chrome)
```

- [ ] **Step 4: Manual test — with-frame screenshot**

```bash
# Take a --with-frame screenshot
./sandbox screenshot --id <id> --with-frame -o with-frame.png
# Verify: shows full macOS window (title bar, traffic lights, tab bar, status bar)
```

- [ ] **Step 5: Manual test — fallback when renderer unavailable**

```bash
# Close Electron app, keep daemon running
# Take --with-frame screenshot (should fall back to ScreenCaptureKit)
./sandbox screenshot --id <id> --with-frame -o fallback.png
```

- [ ] **Step 6: Commit and push**

```bash
git add -A
git commit -m "test: verify --with-frame screenshot end-to-end"
git push -u origin feature/screenshot-with-frame
```

- [ ] **Step 7: Create PR**

```bash
gh pr create --title "feat(screenshot): add --with-frame for full macOS window capture" --body "$(cat <<'EOF'
## Summary

- Add `--with-frame` flag to `sandbox screenshot` command
- When specified, briefly switches the Electron UI to the target tab, captures the full macOS window with ScreenCaptureKit, then switches back
- Switch happens in <16ms, invisible to the user
- Falls back to terminal-only capture if renderer is unavailable

## Architecture

```
CLI --with-frame → daemon HTTP → WebSocket to renderer → renderer switches tab
→ renderer confirms → daemon captures with ScreenCaptureKit → daemon tells renderer to switch back
```

## Test plan

- [ ] `cargo check --all-targets` passes
- [ ] `cargo test` passes
- [ ] `pnpm typecheck` passes
- [ ] Default screenshot: terminal content only
- [ ] `--with-frame` screenshot: full macOS window with title bar, traffic lights, tab bar, status bar
- [ ] Fallback: when renderer unavailable, falls back to ScreenCaptureKit

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
