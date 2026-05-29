# CLI PTY WebSocket 写入 — 修复 CLI --pty 模式

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 CLI 的 PTY 写入从已删除的 HTTP `POST /pty/write` 端点切换到 WebSocket `/pty/ws/{pid}` 端点，修复 `sandbox type --pty` 和 `sandbox key --pty` 命令。

**Architecture:** CLI 客户端 `SandboxClient` 添加 `tokio-tungstenite` WebSocket 连接能力。`pty_write` 方法改为：连接 WebSocket → 发送文本消息 → 关闭连接。`pty_write_auto` 保持不变（自动发现 PID 后调用 `pty_write`）。

**Tech Stack:** Rust, tokio, tokio-tungstenite

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `Cargo.toml` (workspace) | 添加 `tokio-tungstenite` workspace 依赖 |
| `crates/sandbox-cli/Cargo.toml` | 添加 `tokio-tungstenite` 依赖 |
| `crates/sandbox-cli/src/client.rs` | 修改 `pty_write` 使用 WebSocket |
| `crates/sandbox-cli/src/client.rs` (tests) | 添加 WebSocket pty_write 测试 |

---

### Task 1: 添加 tokio-tungstenite 依赖

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/sandbox-cli/Cargo.toml`

- [ ] **Step 1: 在 workspace Cargo.toml 中添加 tokio-tungstenite**

在 `Cargo.toml` 的 `[workspace.dependencies]` 中添加：

```toml
tokio-tungstenite = { version = "0.29", features = ["connect"] }
```

- [ ] **Step 2: 在 sandbox-cli Cargo.toml 中引用 workspace 依赖**

在 `crates/sandbox-cli/Cargo.toml` 的 `[dependencies]` 中添加：

```toml
tokio-tungstenite.workspace = true
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p sandbox-cli`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/sandbox-cli/Cargo.toml
git commit -m "chore(cli): add tokio-tungstenite dependency for WebSocket PTY"
```

---

### Task 2: 修改 SandboxClient — 存储 port 并重写 pty_write

**Files:**
- Modify: `crates/sandbox-cli/src/client.rs` (SandboxClient struct + pty_write method)

- [ ] **Step 1: 在 SandboxClient 中添加 port 字段**

修改 `SandboxClient` 结构体，添加 `port` 字段：

```rust
pub struct SandboxClient {
    base_url: String,
    port: u16,
    client: reqwest::Client,
}
```

修改 `from_port` 方法以存储 port：

```rust
pub fn from_port(port: u16) -> Self {
    let client = reqwest::ClientBuilder::new()
        .no_proxy()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    Self {
        base_url: format!("http://127.0.0.1:{port}"),
        port,
        client,
    }
}
```

- [ ] **Step 2: 重写 pty_write 使用 WebSocket**

替换当前的 `pty_write` 方法（HTTP POST）为 WebSocket 实现：

```rust
pub async fn pty_write(&self, pid: u32, data: &str) -> Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;

    let url = format!("ws://127.0.0.1:{}/pty/ws/{}", self.port, pid);
    let (mut ws_stream, _) = connect_async(&url)
        .await
        .with_context(|| format!("Failed to connect to PTY WebSocket for pid={pid}"))?;

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(data.to_string()))
        .await
        .with_context(|| "Failed to send data to PTY WebSocket")?;

    // Close the connection after sending
    ws_stream.close(None).await.ok();

    Ok(())
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p sandbox-cli`
Expected: 编译通过

- [ ] **Step 4: 运行现有测试**

Run: `cargo test -p sandbox-cli`
Expected: 所有现有测试通过

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-cli/src/client.rs
git commit -m "fix(cli): switch pty_write from HTTP POST to WebSocket

The /pty/write HTTP endpoint was replaced by /pty/ws/{pid} WebSocket
in commit cecd18e. Update the CLI client to use WebSocket for PTY
input, restoring 'sandbox type --pty' and 'sandbox key --pty' functionality."
```

---

### Task 3: 验证端到端 — 手动测试

**Files:** 无代码修改，纯验证

- [ ] **Step 1: 构建 release CLI**

Run: `cargo build --release -p sandbox-cli`
Expected: 构建成功

- [ ] **Step 2: 启动沙箱并测试 PTY 输入**

```bash
# 启动沙箱
./target/release/sandbox start zsh
sleep 5

# 获取 sandbox ID
./target/release/sandbox list

# 通过 PTY 发送文本（替换 <id> 为实际 ID）
./target/release/sandbox type --id <id> --pty "hello from PTY"

# 通过 PTY 按键
./target/release/sandbox key --id <id> Return --pty

# 截图验证
./target/release/sandbox screenshot --id <id> -o test_pty_ws.png

# 清理
./target/release/sandbox close <id>
```

Expected: 文本成功发送到 PTY，截图显示输入内容

- [ ] **Step 3: 测试中文输入**

```bash
./target/release/sandbox type --id <id> --pty "你好世界"
./target/release/sandbox key --id <id> Return --pty
./target/release/sandbox screenshot --id <id> -o test_chinese_pty.png
```

Expected: 中文文本成功发送

- [ ] **Step 4: 运行全量测试**

Run: `cargo test --all && pnpm test:unit && pnpm typecheck`
Expected: 全部通过

- [ ] **Step 5: 完成 — 无额外 commit**
