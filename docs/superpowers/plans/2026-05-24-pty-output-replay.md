# PTY Output Replay — 修复 WebSocket 晚订阅者丢失早期输出

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 PTY 终端启动后空白的问题——WebSocket 连接时重放缓冲区中的早期输出，确保 shell prompt 等内容不丢失。

**Architecture:** 参照 WaveTerm 的两阶段加载模式：新 WebSocket 订阅者连接时，先从 buffer 重放已有输出，再切换到实时 broadcast 流。核心改动在 `handle_pty_ws` 中，连接建立后先 drain buffer 再进入 streaming 循环。

**Tech Stack:** Rust, tokio, tokio-tungstenite, axum WebSocket

---

## 问题根因

```
时间线：
t=0ms    PTY 创建，zsh 启动，输出 prompt
t=10ms   Reader 线程捕获 prompt → broadcast::send()
         ⚠️ 此时没有订阅者，消息丢失
t=500ms  前端加载完成，WebSocket 连接 → subscribe()
         ❌ 但 prompt 已经丢了
t=∞      zsh 等待输入，不再产生新输出 → 终端永久空白
```

`tokio::sync::broadcast` 不缓冲未订阅者的消息。一旦 send 时没有 receiver，消息即丢失。

## WaveTerm 的解决方案（参考）

WaveTerm 使用**两阶段加载**：
1. **持久化存储**：PTY 输出写入 2MB 循环 blockfile（SQLite），确保数据不丢
2. **HTTP 重放**：新连接先通过 HTTP 读取 blockfile 中的已有数据
3. **实时流**：重放完成后切换到 WPS 事件实时推送

我们的简化方案（不需要 SQLite）：
1. **Buffer 已有**：Reader 线程已将输出写入 `VecDeque<String>` buffer
2. **连接时重放**：WebSocket 连接后先 drain buffer 发送给客户端
3. **实时流**：重放完成后进入 broadcast 接收循环

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `crates/sandbox-core/src/process/mod.rs` | 添加 `drain_buffer` 方法，返回 buffer 中所有未读数据 |
| `crates/sandbox-core/src/server/mod.rs` | 修改 `handle_pty_ws`：连接后先重放 buffer 再 streaming |
| `crates/sandbox-core/tests/pty_replay_test.rs` | 新增测试：验证晚订阅者能收到早期输出 |

---

### Task 1: 添加 drain_buffer 方法

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs`

- [ ] **Step 1: 在 ProcessManager 中添加 drain_buffer 方法**

在 `subscribe_output` 方法附近添加：

```rust
/// Drain all buffered PTY output for a session.
/// Used by WebSocket handler to replay early output to late subscribers.
#[cfg(target_os = "macos")]
pub fn drain_buffer(pid: u32) -> Result<Vec<String>> {
    let sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;
    let mut buffer = session
        .buffer
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    Ok(buffer.drain(..).collect())
}

#[cfg(not(target_os = "macos"))]
pub fn drain_buffer(_pid: u32) -> Result<Vec<String>> {
    Err(AppError::Process(
        "drain_buffer only supported on macOS".into(),
    ))
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "feat(process): add drain_buffer method for PTY output replay"
```

---

### Task 2: 修改 WebSocket handler — 连接后先重放 buffer

**Files:**
- Modify: `crates/sandbox-core/src/server/mod.rs:395-469` (`handle_pty_ws`)

- [ ] **Step 1: 修改 handle_pty_ws 函数**

在 `send_task` 启动前，先重放缓冲区内容：

```rust
async fn handle_pty_ws(pid: u32, socket: WebSocket) {
    let mut rx = match ProcessManager::subscribe_output(pid) {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!("[pty_ws] pid={pid}: subscribe failed: {e}");
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Phase 1: Replay buffered output to late subscriber
    match ProcessManager::drain_buffer(pid) {
        Ok(chunks) => {
            for chunk in chunks {
                if ws_tx.send(Message::Text(chunk.into())).await.is_err() {
                    tracing::debug!("[pty_ws] pid={pid}: client disconnected during replay");
                    return;
                }
            }
            tracing::debug!("[pty_ws] pid={pid}: replayed {} chunks", chunks.len());
        }
        Err(e) => {
            tracing::warn!("[pty_ws] pid={pid}: drain_buffer failed: {e}");
        }
    }

    // Phase 2: Real-time streaming via broadcast
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // ... recv_task 保持不变 ...
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 3: 运行现有测试**

Run: `cargo test -p sandbox-core`
Expected: 所有现有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/server/mod.rs
git commit -m "fix(server): replay buffered PTY output to late WebSocket subscribers

When a WebSocket connects after PTY output has already been produced
(e.g., shell prompt), the broadcast channel has lost those messages.
Fix by draining the buffer and sending it to the client before entering
the real-time streaming loop.

This is the same two-phase load pattern used by WaveTerm:
1. Replay existing data from buffer
2. Stream real-time updates via broadcast"
```

---

### Task 3: 添加集成测试 — 验证晚订阅者收到早期输出

**Files:**
- Create: `crates/sandbox-core/tests/pty_replay_test.rs`

- [ ] **Step 1: 编写测试**

```rust
#![cfg(target_os = "macos")]

use sandbox_core::process::ProcessManager;

#[test]
fn test_pty_replay_late_subscriber() {
    // Spawn a process that produces output immediately
    let info = ProcessManager::spawn_cli("echo", &["hello from replay"])
        .expect("Failed to spawn echo");
    eprintln!("Spawned echo: pid={}", info.pid);

    // Wait for output to be produced and buffered
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Simulate late subscriber: drain buffer (like WebSocket handler does)
    let chunks = ProcessManager::drain_buffer(info.pid)
        .expect("Failed to drain buffer");
    let total: String = chunks.join("");
    eprintln!("Drained buffer: {} chars: {:?}", total.len(), &total);

    // Cleanup
    ProcessManager::kill_process(info.pid).expect("Failed to kill");

    assert!(
        total.contains("hello from replay"),
        "Expected buffer to contain early output, got: {:?}",
        total
    );
}

#[test]
fn test_pty_replay_empty_buffer_is_ok() {
    // Spawn a long-running process (cat) that doesn't produce output
    let info = ProcessManager::spawn_cli("cat", &[]).expect("Failed to spawn cat");

    // Drain immediately — buffer should be empty but not error
    let chunks = ProcessManager::drain_buffer(info.pid)
        .expect("Failed to drain empty buffer");
    eprintln!("Drained empty buffer: {} chunks", chunks.len());

    // Cleanup
    ProcessManager::kill_process(info.pid).expect("Failed to kill");

    assert!(chunks.is_empty(), "Expected empty buffer for cat with no input");
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p sandbox-core --test pty_replay_test`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/sandbox-core/tests/pty_replay_test.rs
git commit -m "test(process): add PTY replay tests for late WebSocket subscribers"
```

---

### Task 4: 端到端验证 — release 构建 + 手动测试

**Files:** 无代码修改，纯验证

- [ ] **Step 1: 构建 release**

Run: `./release.sh`
Expected: 构建成功

- [ ] **Step 2: 测试 zsh 启动后立即可见**

```bash
./release/cli-box start zsh
sleep 5
./release/cli-box list  # 获取 ID
./release/cli-box screenshot --id <id> -o test_zsh_replay.png
```

Expected: 截图中立即显示 zsh prompt，无需按 Enter

- [ ] **Step 3: 测试 claude 启动后立即可见**

```bash
./release/cli-box start claude
sleep 8
./release/cli-box list
./release/cli-box screenshot --id <id> -o test_claude_replay.png
```

Expected: 截图中显示 claude 的 "Welcome back!" 界面

- [ ] **Step 4: 运行全量测试**

Run: `cargo test --all && pnpm test:unit && pnpm typecheck`
Expected: 全部通过

- [ ] **Step 5: 完成 — 无额外 commit**
