# PTY WebSocket Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all HTTP-based PTY operations with a single bidirectional WebSocket connection per PTY session, enabling real-time terminal query responses (like `\x1b[14t`) so TUI apps like opencode render correctly.

**Architecture:** Each PTY session gets a WebSocket endpoint (`/pty/ws/{pid}`). The connection is fully bidirectional: PTY output is pushed to the client as text messages, user input and control commands (resize) are sent from the client as text/binary messages. Internally, a `tokio::sync::broadcast` channel fans out PTY output to all connected WebSocket clients. The old HTTP PTY endpoints (`/pty/write`, `/pty/output/{pid}`, `/pty/resize`) are removed.

**Tech Stack:** Axum 0.8 WebSocket (`ws` feature), `tokio::sync::broadcast`, browser-native `WebSocket` API, xterm.js v6.

---

## WebSocket Protocol

### 客户端 → 服务端 (用户发送)

| 消息类型 | 格式 | 说明 |
|---------|------|------|
| Text | 任意文本 | PTY 标准输入（用户键盘输入） |
| Binary | 原始字节 | PTY 原始字节输入 |
| Text | `{"type":"resize","cols":80,"rows":24}` | 调整终端大小 |
| Text | `{"type":"ping"}` | 心跳保活 |

### 服务端 → 客户端 (PTY 输出)

| 消息类型 | 格式 | 说明 |
|---------|------|------|
| Text | PTY 输出内容 | 终端输出（可能是 ANSI escape 序列） |
| Text | `{"type":"error","msg":"..."}` | 错误信息 |
| Text | `{"type":"pong"}` | 心跳响应 |
| Close | code + reason | 连接关闭 |

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` (workspace) | Modify | Add `ws` and `futures-util` dependencies |
| `crates/sandbox-core/Cargo.toml` | Modify | Add `futures-util` workspace dependency |
| `crates/sandbox-core/src/process/mod.rs` | Modify | Add `broadcast::Sender<String>` to `PtySession`, push from reader thread, add `subscribe_output()` method |
| `crates/sandbox-core/src/server/mod.rs` | Modify | Add `/pty/ws/{pid}` WebSocket handler, remove old HTTP PTY endpoints |
| `sandbox-web/src/api.ts` | Modify | Add `ptyConnectWs(pid)`, remove old `ptyWrite`/`ptyRead`/`ptyResize` |
| `sandbox-web/src/components/Terminal.tsx` | Modify | Replace polling with WebSocket, handle resize via WS |
| `sandbox-web/src/main.tsx` | Modify | Remove `handleTerminalInput` callback (input now handled by Terminal directly) |

---

### Task 1: Add Axum WebSocket and futures-util Dependencies

**Files:**
- Modify: `Cargo.toml:22` (workspace root)
- Modify: `crates/sandbox-core/Cargo.toml`

- [ ] **Step 1: Add `ws` feature to axum and `futures-util` in workspace Cargo.toml**

In `/Users/zn-ice/2026/cli-box/Cargo.toml`:

```toml
# Change axum (line 22):
axum = { version = "0.8", features = ["ws"] }

# Add after existing deps (e.g., after line 34):
futures-util = "0.3"
```

- [ ] **Step 2: Add futures-util to sandbox-core**

In `/Users/zn-ice/2026/cli-box/crates/sandbox-core/Cargo.toml`, add under `[dependencies]`:

```toml
futures-util.workspace = true
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/sandbox-core/Cargo.toml
git commit -m "feat(deps): add axum ws feature and futures-util for WebSocket support"
```

---

### Task 2: Add Broadcast Channel to PtySession

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs`

- [ ] **Step 1: Add broadcast import and field to PtySession**

At the top of `/Users/zn-ice/2026/cli-box/crates/sandbox-core/src/process/mod.rs`, add to imports:

```rust
use tokio::sync::broadcast;
```

Add field to `PtySession` (after `reader_thread` field, line ~44):

```rust
/// Broadcast sender for streaming PTY output to WebSocket subscribers
output_tx: broadcast::Sender<String>,
```

- [ ] **Step 2: Initialize broadcast channel in spawn_cli**

In `spawn_cli`, where `buffer` and `stop_flag` are created (around line 170), add:

```rust
let (output_tx, _) = broadcast::channel::<String>(256);
let thread_tx = output_tx.clone();
```

- [ ] **Step 3: Push output from reader thread to broadcast channel**

In the reader thread, inside the `Ok(n) =>` match arm (around line 193), after the `buf.push_back(text)` line, add:

```rust
let _ = thread_tx.send(text.clone());
```

- [ ] **Step 4: Store output_tx in PtySession insert**

In `sessions.insert(...)` (around line 220), add `output_tx`:

```rust
sessions.insert(
    tracked_id,
    PtySession {
        writer,
        master: pty_pair.master,
        child_pid: child_pid.unwrap_or(0),
        command: command.to_string(),
        buffer,
        stop_flag,
        reader_thread: Some(reader_thread),
        output_tx,
    },
);
```

- [ ] **Step 5: Add subscribe_output method to ProcessManager**

Add after `read_output` (around line 438):

```rust
/// Subscribe to PTY output stream for WebSocket streaming.
#[cfg(target_os = "macos")]
pub fn subscribe_output(pid: u32) -> Result<broadcast::Receiver<String>> {
    let sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
    Ok(session.output_tx.subscribe())
}

#[cfg(not(target_os = "macos"))]
pub fn subscribe_output(_pid: u32) -> Result<broadcast::Receiver<String>> {
    Err(AppError::Process(
        "subscribe_output only supported on macOS".into(),
    ))
}
```

- [ ] **Step 6: Verify compilation and tests**

Run: `cargo check -p sandbox-core && cargo test -p sandbox-core`
Expected: Compiles and all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "feat(process): add broadcast channel for PTY output streaming"
```

---

### Task 3: Add WebSocket Handler and Remove Old HTTP PTY Endpoints

**Files:**
- Modify: `crates/sandbox-core/src/server/mod.rs`

- [ ] **Step 1: Add WebSocket imports**

In `/Users/zn-ice/2026/cli-box/crates/sandbox-core/src/server/mod.rs`, add imports:

```rust
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
```

- [ ] **Step 2: Replace HTTP PTY routes with WebSocket route**

In `build_router()` (around line 149), replace the three PTY HTTP routes:

```rust
// REMOVE these three routes:
// .route("/pty/write", post(pty_write_handler))
// .route("/pty/resize", post(pty_resize_handler))
// .route("/pty/output/{pid}", get(pty_output_handler))

// ADD this single WebSocket route:
.route("/pty/ws/{pid}", axum::routing::get(pty_ws_handler))
```

- [ ] **Step 3: Remove old HTTP PTY handlers**

Remove the `pty_write_handler`, `pty_resize_handler`, and `pty_output_handler` functions from the file.

Also remove the `PtyWriteRequest` and `PtyResizeRequest` structs if they are only used by these handlers.

- [ ] **Step 4: Implement WebSocket handler**

Add after the other handlers:

```rust
/// WebSocket endpoint for real-time bidirectional PTY streaming.
async fn pty_ws_handler(
    Path(pid): Path<u32>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_pty_ws(socket, pid))
}

async fn handle_pty_ws(socket: WebSocket, pid: u32) {
    use futures_util::StreamExt;

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to PTY output broadcast channel
    let mut output_rx = match ProcessManager::subscribe_output(pid) {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("WS PTY subscribe failed pid={pid}: {e}");
            let _ = ws_tx.send(Message::Close(Some(
                axum::extract::ws::CloseFrame {
                    code: axum::extract::ws::CloseCode::Error,
                    reason: std::borrow::Cow::Owned(e.to_string()),
                },
            ))).await;
            return;
        }
    };

    tracing::info!("WS PTY opened pid={pid}");

    // Task: read client messages → write to PTY
    let input_task = tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Check for control messages
                    if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(msg_type) = cmd.get("type").and_then(|v| v.as_str()) {
                            match msg_type {
                                "resize" => {
                                    let cols = cmd.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
                                    let rows = cmd.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;
                                    let _ = ProcessManager::resize_pty(pid, cols, rows);
                                    continue;
                                }
                                "ping" => continue,
                                _ => {}
                            }
                        }
                    }
                    // Regular text input → write to PTY
                    if let Err(e) = ProcessManager::send_input(pid, text.as_bytes()) {
                        tracing::warn!("WS PTY write failed pid={pid}: {e}");
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    if let Err(e) = ProcessManager::send_input(pid, &data) {
                        tracing::warn!("WS PTY write failed pid={pid}: {e}");
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                _ => {}
            }
        }
    });

    // Stream PTY output → client
    loop {
        tokio::select! {
            result = output_rx.recv() => {
                match result {
                    Ok(text) => {
                        if ws_tx.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WS PTY lagged pid={pid} by {n}");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = &mut input_task => break,
        }
    }

    tracing::info!("WS PTY closed pid={pid}");
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-core/src/server/mod.rs
git commit -m "feat(server): replace HTTP PTY endpoints with WebSocket streaming"
```

---

### Task 4: Update Frontend API — WebSocket Only

**Files:**
- Modify: `sandbox-web/src/api.ts`

- [ ] **Step 1: Remove old HTTP PTY functions**

In `/Users/zn-ice/2026/cli-box/sandbox-web/src/api.ts`, remove:

```typescript
// REMOVE these functions:
export async function ptyWrite(pid: number, data: string): Promise<void> { ... }
export async function ptyResize(pid: number, cols: number, rows: number): Promise<void> { ... }
export async function ptyRead(pid: number): Promise<{ output: string | null }> { ... }
```

- [ ] **Step 2: Add WebSocket PTY client**

Add after the process functions:

```typescript
// ── WebSocket PTY Streaming ──────────────────────

export interface PtyWsConnection {
  ws: WebSocket;
  onOutput: (cb: (data: string) => void) => () => void;
}

/**
 * Connect to PTY via WebSocket. Returns a bidirectional connection:
 * - ws.send(text) → PTY stdin
 * - ws.send(JSON.stringify({type:"resize", cols, rows})) → resize
 * - onOutput(cb) → subscribe to PTY stdout
 */
export function ptyConnectWs(pid: number): PtyWsConnection {
  const url = `ws://127.0.0.1:${getPort()}/pty/ws/${pid}`;
  const ws = new WebSocket(url);
  const listeners: ((data: string) => void)[] = [];

  ws.onmessage = (event) => {
    if (typeof event.data === "string") {
      for (const cb of listeners) cb(event.data);
    }
  };

  ws.onerror = (err) => console.error("[WS] PTY error:", err);
  ws.onclose = () => console.log("[WS] PTY closed pid=", pid);

  return {
    ws,
    onOutput: (cb) => {
      listeners.push(cb);
      return () => {
        const idx = listeners.indexOf(cb);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },
  };
}

/** Send resize command over PTY WebSocket */
export function ptyWsResize(ws: WebSocket, cols: number, rows: number) {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: "resize", cols, rows }));
  }
}
```

- [ ] **Step 3: Verify TypeScript compilation**

Run: `cd sandbox-web && pnpm typecheck`
Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
git add sandbox-web/src/api.ts
git commit -m "feat(api): replace HTTP PTY functions with WebSocket client"
```

---

### Task 5: Replace Polling with WebSocket in Terminal Component

**Files:**
- Modify: `sandbox-web/src/components/Terminal.tsx`
- Modify: `sandbox-web/src/main.tsx`

- [ ] **Step 1: Rewrite Terminal.tsx to use WebSocket**

Replace the entire content of `/Users/zn-ice/2026/cli-box/sandbox-web/src/components/Terminal.tsx`:

```tsx
import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import * as api from "../api";
import { useTheme } from "../themes/ThemeContext";
import type { TerminalTheme } from "../themes/types";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  activePid?: number | null;
}

function buildTerminalTheme(t: TerminalTheme): Record<string, string> {
  return {
    background: t.background,
    foreground: t.foreground,
    cursor: t.cursor,
    cursorAccent: t.cursorAccent,
    selectionBackground: t.selectionBackground,
    selectionForeground: t.selectionForeground,
    black: t.black,
    red: t.red,
    green: t.green,
    yellow: t.yellow,
    blue: t.blue,
    magenta: t.magenta,
    cyan: t.cyan,
    white: t.white,
    brightBlack: t.brightBlack,
    brightRed: t.brightRed,
    brightGreen: t.brightGreen,
    brightYellow: t.brightYellow,
    brightBlue: t.brightBlue,
    brightMagenta: t.brightMagenta,
    brightCyan: t.brightCyan,
    brightWhite: t.brightWhite,
  };
}

export default function SandboxTerminal({ activePid = null }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const activePidRef = useRef(activePid);
  const wsRef = useRef<api.PtyWsConnection | null>(null);
  const { theme } = useTheme();

  useEffect(() => {
    activePidRef.current = activePid;
  }, [activePid]);

  // Initialize xterm.js once
  useEffect(() => {
    if (!terminalRef.current) return;
    if (xtermRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 14,
      lineHeight: 1.35,
      fontFamily:
        '"SF Mono", "Menlo", "Monaco", "Cascadia Code", "JetBrains Mono", monospace',
      fontWeight: "400",
      fontWeightBold: "600",
      letterSpacing: 0,
      scrollback: 10000,
      theme: buildTerminalTheme(theme.terminal),
      allowProposedApi: true,
      drawBoldTextInBrightColors: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();

    const handleResize = () => {
      fitAddon.fit();
      const pid = activePidRef.current;
      const ws = wsRef.current?.ws;
      if (pid && ws && ws.readyState === WebSocket.OPEN) {
        api.ptyWsResize(ws, term.cols, term.rows);
      }
    };
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Update terminal theme in-place
  useEffect(() => {
    if (!xtermRef.current) return;
    xtermRef.current.options.theme = buildTerminalTheme(theme.terminal);
  }, [theme.id]);

  // WebSocket PTY streaming — replaces HTTP polling
  useEffect(() => {
    // Clean up previous connection
    if (wsRef.current) {
      wsRef.current.ws.close();
      wsRef.current = null;
    }

    if (activePid === null || activePid === undefined) return;

    const term = xtermRef.current;
    if (!term) return;

    // Connect WebSocket
    const conn = api.ptyConnectWs(activePid);
    wsRef.current = conn;

    // PTY output → xterm.js
    const unsub = conn.onOutput((data) => {
      if (xtermRef.current) {
        xtermRef.current.write(data);
      }
    });

    // xterm.js input → PTY (via WebSocket)
    const inputDisposable = term.onData((data) => {
      if (conn.ws.readyState === WebSocket.OPEN) {
        conn.ws.send(data);
      }
    });

    // Send initial resize
    api.ptyWsResize(conn.ws, term.cols, term.rows);

    return () => {
      unsub();
      inputDisposable.dispose();
      conn.ws.close();
      wsRef.current = null;
    };
  }, [activePid]);

  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitAddonRef.current?.fit());
    }
  }, []);

  return (
    <div ref={containerRef} className="w-full h-full">
      <div ref={terminalRef} className="w-full h-full" />
    </div>
  );
}
```

- [ ] **Step 2: Update main.tsx to remove onTerminalInput**

In `/Users/zn-ice/2026/cli-box/sandbox-web/src/main.tsx`:

1. Remove the `handleTerminalInput` callback (lines 69-76):

```typescript
// REMOVE:
const handleTerminalInput = useCallback(
  (data: string) => {
    if (activePid !== null) {
      api.ptyWrite(activePid, data).catch(() => {});
    }
  },
  [activePid],
);
```

2. Remove the `onTerminalInput` prop from `<Dashboard>` (line 117):

```tsx
// Change:
<Dashboard
  command={command}
  connected={connected}
  activePid={activePid}
  onTerminalInput={handleTerminalInput}  // REMOVE this line
  onScreenshot={handleScreenshot}
>

// To:
<Dashboard
  command={command}
  connected={connected}
  activePid={activePid}
  onScreenshot={handleScreenshot}
>
```

3. Remove the `api.ptyWrite` import if no longer used.

- [ ] **Step 3: Update Dashboard component if it passes onTerminalInput to Terminal**

Check `sandbox-web/src/components/Dashboard.tsx` — if it passes `onInput` or `onTerminalInput` to `<SandboxTerminal>`, remove that prop:

```tsx
// If Dashboard has:
<SandboxTerminal onInput={onTerminalInput} activePid={activePid} />

// Change to:
<SandboxTerminal activePid={activePid} />
```

- [ ] **Step 4: Verify TypeScript compilation**

Run: `cd sandbox-web && pnpm typecheck`
Expected: No type errors.

- [ ] **Step 5: Commit**

```bash
git add sandbox-web/src/components/Terminal.tsx sandbox-web/src/main.tsx sandbox-web/src/components/Dashboard.tsx
git commit -m "feat(terminal): replace HTTP polling with WebSocket bidirectional streaming"
```

---

### Task 6: End-to-End Test with opencode

**Files:**
- Test: manual verification

- [ ] **Step 1: Build and start sandbox with opencode**

```bash
cd /Users/zn-ice/2026/cli-box
./release.sh
./target/release/sandbox start opencode
```

- [ ] **Step 2: Verify WebSocket connection**

Open browser DevTools → Network tab → WS filter:
- Connection to `ws://127.0.0.1:5801/pty/ws/1000` should appear
- Status: `101 Switching Protocols`
- Messages tab should show streaming PTY output

- [ ] **Step 3: Verify TUI rendering**

- opencode should render its TUI interface (NOT a blank dark screen)
- The terminal area should show opencode's startup UI with text, borders, and colors

- [ ] **Step 4: Verify interactive input**

- Click in terminal, type "你是谁？", press Enter
- opencode should respond with text
- Response should be near-instant (no 50ms polling delay)

- [ ] **Step 5: Take screenshots**

```bash
curl -s -o release_test/ws_opencode_render.png http://127.0.0.1:5801/screenshot
```

- [ ] **Step 6: Verify zsh and vim still work**

```bash
./target/release/sandbox start zsh    # verify prompt renders
./target/release/sandbox start vim    # verify ~ characters render
```

---

## Summary

| Task | Files Changed | Lines Changed | Risk |
|------|--------------|---------------|------|
| 1. Dependencies | 2 | ~3 | Low |
| 2. Broadcast channel | 1 | ~30 | Low |
| 3. WebSocket handler | 1 | ~90 | Medium |
| 4. Frontend API | 1 | ~50 | Low |
| 5. Terminal + main | 3 | ~60 | Medium |
| 6. E2E test | 0 | 0 | None |

**Total: ~230 lines changed across 8 files**

## What Changes for Each Stakeholder

| 使用场景 | 之前 | 之后 |
|---------|------|------|
| Tauri 前端 (xterm.js) | HTTP 50ms 轮询 | WebSocket 实时推送 |
| CLI 工具 (`sandbox screenshot/click`) | HTTP API | HTTP API（不变） |
| MCP Agent | HTTP/MCP | HTTP/MCP（不变） |
| curl 调试 PTY 输出 | `curl /pty/output/1000` | 需要 WebSocket 客户端 |
| curl 发送 PTY 输入 | `curl -X POST /pty/write` | 需要 WebSocket 客户端 |

HTTP 管理端点（health、screenshot、input 模拟等）完全不变。只有 PTY 实时通信从 HTTP 迁移到 WebSocket。
