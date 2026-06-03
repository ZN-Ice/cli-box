# Phase 7: Integration Tests, MCP Update & Docs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add MCP tools for sandbox management, write integration tests, run end-to-end smoke tests, and update documentation.

**Architecture:** MCP tools wrap the daemon HTTP client. Integration tests verify the daemon API end-to-end. Documentation updates reflect the Electron architecture.

**Tech Stack:** Rust, serde_json, tokio (test), axum (test server)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-cli/src/main.rs` | Modify | Add `McpServe` command with MCP tools |
| `crates/sandbox-core/tests/daemon_integration.rs` | Create | Daemon HTTP API integration tests |
| `CLAUDE.md` | Modify | Update architecture section for Electron |
| `README.md` | Modify | Update quick start for Electron |

---

### Task 1: MCP tools for sandbox management

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs`

- [ ] **Step 1: Add McpServe command**

Add to the `Commands` enum:

```rust
/// Start MCP stdio server for agent integration
McpServe,
```

- [ ] **Step 2: Implement MCP tool definitions**

Add a function that returns the MCP tool schema:

```rust
fn mcp_tools() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "list_sandboxes",
                "description": "List all active sandbox instances",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "start_sandbox",
                "description": "Start a new sandbox with a CLI command",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command to run (e.g., 'zsh', 'claude')" },
                        "args": { "type": "array", "items": { "type": "string" }, "description": "Command arguments" }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "close_sandbox",
                "description": "Close a sandbox by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            },
            {
                "name": "screenshot_sandbox",
                "description": "Take a screenshot of a sandbox",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            },
            {
                "name": "type_text",
                "description": "Type text into a sandbox PTY",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" },
                        "text": { "type": "string" }
                    },
                    "required": ["sandbox_id", "text"]
                }
            },
            {
                "name": "press_key",
                "description": "Press a key in a sandbox",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" },
                        "key": { "type": "string", "description": "Key name (Return, Tab, Escape, etc.)" },
                        "modifiers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["sandbox_id", "key"]
                }
            },
            {
                "name": "inspect_ui",
                "description": "Inspect the UI tree of a sandbox window",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sandbox_id": { "type": "string" }
                    },
                    "required": ["sandbox_id"]
                }
            }
        ]
    })
}
```

- [ ] **Step 3: Implement MCP stdio handler**

Add a basic JSON-RPC over stdio handler:

```rust
async fn run_mcp_server() -> anyhow::Result<()> {
    use std::io::{self, BufRead, Write};

    let client = SandboxClient::from_daemon_json().await
        .unwrap_or_else(|_| SandboxClient::from_port(15801));

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }

        let msg: serde_json::Value = serde_json::from_str(&line)?;
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(serde_json::json!({}));

        let result = match method {
            "initialize" => serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "sandbox-mcp", "version": "0.1.0" }
            }),
            "tools/list" => mcp_tools(),
            "tools/call" => {
                let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                handle_mcp_tool(&client, tool_name, &args).await
            }
            _ => serde_json::json!({ "error": { "code": -32601, "message": "Method not found" } }),
        };

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }
    Ok(())
}

async fn handle_mcp_tool(client: &SandboxClient, name: &str, args: &serde_json::Value) -> serde_json::Value {
    let result: anyhow::Result<serde_json::Value> = async {
        match name {
            "list_sandboxes" => {
                let list = sandbox_core::daemon::list_sandboxes_via_http(&client_url).await?;
                Ok(serde_json::to_value(list)?)
            }
            "start_sandbox" => {
                let cmd = args["command"].as_str().unwrap_or("zsh");
                let cmd_args: Vec<String> = args["args"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let result = daemon_create_sandbox("cli", cmd, &cmd_args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "close_sandbox" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                daemon_close(id).await?;
                Ok(serde_json::json!({ "closed": id }))
            }
            "type_text" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let text = args["text"].as_str().unwrap_or("");
                daemon_pty_write(id, text).await?;
                Ok(serde_json::json!({ "typed": text }))
            }
            "press_key" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let key = args["key"].as_str().unwrap_or("Return");
                let mods: Vec<String> = args["modifiers"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                daemon_key(id, key, &mods).await?;
                Ok(serde_json::json!({ "pressed": key }))
            }
            "inspect_ui" => {
                let id = args["sandbox_id"].as_str().unwrap_or("");
                let tree = daemon_inspect(id).await?;
                Ok(serde_json::to_value(tree)?)
            }
            _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {name}") })),
        }
    }.await;

    match result {
        Ok(value) => serde_json::json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }]
        }),
        Err(e) => serde_json::json!({
            "content": [{ "type": "text", "text": format!("Error: {e}") }],
            "isError": true
        }),
    }
}
```

- [ ] **Step 4: Wire McpServe command**

```rust
Commands::McpServe => {
    run_mcp_server().await?;
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p sandbox-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-cli/src/main.rs
git commit -m "feat(cli): add MCP stdio server with sandbox management tools"
```

---

### Task 2: Daemon integration tests

**Files:**
- Create: `crates/sandbox-core/tests/daemon_integration.rs`

- [ ] **Step 1: Write health check test**

```rust
// crates/sandbox-core/tests/daemon_integration.rs
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_daemon_health_endpoint() {
    let state = Arc::new(Mutex::new(sandbox_core::daemon::DaemonState {
        port: 0,
        sandboxes: std::collections::HashMap::new(),
        started_at: std::time::Instant::now(),
    }));

    // Build router (same as run_daemon but without binding)
    let app = sandbox_core::daemon::build_daemon_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client.get(format!("http://{addr}/health"))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn test_daemon_list_empty() {
    let state = Arc::new(Mutex::new(sandbox_core::daemon::DaemonState {
        port: 0,
        sandboxes: std::collections::HashMap::new(),
        started_at: std::time::Instant::now(),
    }));

    let app = sandbox_core::daemon::build_daemon_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client.get(format!("http://{addr}/sandbox/list"))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.as_array().unwrap().is_empty());
}
```

- [ ] **Step 2: Verify tests compile (they need build_daemon_router exposed)**

This requires exposing `build_daemon_router` from the daemon module. Add to `daemon/mod.rs`:

```rust
/// Build the axum router (exposed for testing).
pub fn build_daemon_router(state: Arc<Mutex<DaemonState>>) -> Router {
    // Move the router building code from run_daemon into this function
    // and call it from run_daemon
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p sandbox-core --test daemon_integration`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/tests/daemon_integration.rs crates/sandbox-core/src/daemon/mod.rs
git commit -m "test(core): add daemon HTTP integration tests"
```

---

### Task 3: Update documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Update CLAUDE.md architecture section**

Replace the architecture diagram section to reflect Electron:

```markdown
## 架构总览

```
┌──────────────────────────────────────────────────────────────┐
│                  Agent / 用户 (CLI / MCP / HTTP)              │
│  cli-box start / list / screenshot / click / type / key      │
└───────────────────────────────┬───────────────────────────────┘
                                │ HTTP (localhost:15801)
                                ▼
┌──────────────────────────────────────────────────────────────┐
│              cli-box-daemon (Rust, 单实例)                     │
│  PTY Manager + App Manager + Automation Engine                │
│  Instance Registry (~/.sandbox/instances/)                    │
└───────────────────────────────┬───────────────────────────────┘
                                │ WebSocket (PTY 流)
                                ▼
┌──────────────────────────────────────────────────────────────┐
│              Electron App (单实例, Chromium)                   │
│  Tab 管理 + xterm.js + 控制面板 + 截图预览                     │
└──────────────────────────────────────────────────────────────┘
```
```

- [ ] **Step 2: Update README.md quick start**

Update the build and run instructions to use Electron:

```markdown
### 构建

```bash
# 构建 daemon + CLI
cargo build --release

# 构建 Electron 应用
cd electron-app && pnpm install && pnpm build && cd ..
```

### 运行

```bash
# 启动沙箱（自动启动 daemon + Electron）
./target/release/cli-box start claude
```
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md README.md
git commit -m "docs: update architecture for Electron + daemon model"
```
