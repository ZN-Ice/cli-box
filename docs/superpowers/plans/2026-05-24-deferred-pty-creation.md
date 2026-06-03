# Deferred PTY Creation — 延迟 PTY 创建以修复终端尺寸竞态

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 延迟 PTY 进程创建，直到前端 xterm.js 初始化完成并告知正确的终端尺寸，从根本上解决 opencode 等 TUI 应用的终端尺寸竞态问题。

**Architecture:** Tauri 启动时不再立即 spawn CLI 进程，而是将命令配置暂存在 AppState 中。前端 xterm.js 初始化后，通过 HTTP 获取待启动的命令配置，再以实际容器尺寸调用 `/cli/spawn` 创建 PTY。这样进程从第一行代码开始就使用正确的终端尺寸。

**Tech Stack:** Rust (axum, tokio), TypeScript (React, xterm.js)

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `src-tauri/src/main.rs` | 移除 auto-spawn，存储 pending CLI 配置 |
| `crates/sandbox-core/src/server/mod.rs` | 添加 `/sandbox/pending-cli` 端点，`/cli/spawn` 支持 cols/rows |
| `sandbox-web/src/api.ts` | 添加 `getPendingCli()` 函数 |
| `sandbox-web/src/main.tsx` | 启动时获取 pending CLI 并以正确尺寸 spawn |
| `crates/sandbox-cli/src/main.rs` | `serve` 命令立即 spawn（无前端场景） |

---

### Task 1: 后端 — /cli/spawn 支持 cols/rows 参数

**Files:**
- Modify: `crates/sandbox-core/src/server/mod.rs:93-98` (SpawnCliRequest)
- Modify: `crates/sandbox-core/src/server/mod.rs:221-231` (spawn_cli_handler)
- Test: `crates/sandbox-core/src/server/mod.rs` (tests section)

- [ ] **Step 1: 给 SpawnCliRequest 添加可选的 cols/rows 字段**

在 `crates/sandbox-core/src/server/mod.rs` 中，修改 `SpawnCliRequest` 结构体：

```rust
#[derive(Deserialize)]
struct SpawnCliRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}
```

- [ ] **Step 2: 修改 spawn_cli_handler 支持自定义尺寸**

修改 `spawn_cli_handler`，将 cols/rows 传给 `ProcessManager::spawn_cli_with_size`：

```rust
async fn spawn_cli_handler(
    Json(req): Json<SpawnCliRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let cmd = req.command.clone();
    let cols = req.cols.unwrap_or(80);
    let rows = req.rows.unwrap_or(24);
    let info = tokio::task::spawn_blocking(move || {
        ProcessManager::spawn_cli_with_size(&req.command, &req.args, cols, rows)
    })
    .await
    .map_err(|e| AppError::Process(format!("spawn_cli panicked: {e}")))??;
    tracing::info!("spawned cli: {cmd} ({cols}x{rows})");
    Ok(Json(info))
}
```

- [ ] **Step 3: 在 ProcessManager 中添加 spawn_cli_with_size 方法**

在 `crates/sandbox-core/src/process/mod.rs` 中，在 `spawn_cli` 方法之后添加：

```rust
/// Launch a CLI process with PTY support, specifying exact terminal dimensions.
#[cfg(target_os = "macos")]
pub fn spawn_cli_with_size(
    command: &str,
    args: &[String],
    cols: u16,
    rows: u16,
) -> Result<ProcessInfo> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Process(format!("Failed to open PTY: {e}")))?;

    let mut cmd = CommandBuilder::new(command);
    cmd.args(args);
    cmd.env("TERM", "xterm-256color");
    if std::env::var("COLORTERM").is_err() {
        cmd.env("COLORTERM", "truecolor");
    }
    if std::env::var("LANG").is_err() {
        cmd.env("LANG", "en_US.UTF-8");
    }
    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Process(format!("Failed to spawn command: {e}")))?;

    let child_pid = child.process_id();
    drop(pty_pair.slave);

    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::Process(format!("Failed to clone PTY reader: {e}")))?;
    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| AppError::Process(format!("Failed to take PTY writer: {e}")))?;

    let tracked_id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let buffer: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let stop_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let (output_tx, _) = broadcast::channel::<String>(256);
    let thread_tx = output_tx.clone();
    let thread_buffer = Arc::clone(&buffer);
    let thread_stop = Arc::clone(&stop_flag);

    let reader_thread = std::thread::Builder::new()
        .name(format!("pty-reader-{tracked_id}"))
        .spawn(move || {
            let mut reader = reader;
            let mut read_buf = [0u8; 4096];
            loop {
                if thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                match std::io::Read::read(&mut reader, &mut read_buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&read_buf[..n]).to_string();
                        let _ = thread_tx.send(text.clone());
                        if let Ok(mut buf) = thread_buffer.lock() {
                            if buf.len() >= 1000 {
                                buf.pop_front();
                            }
                            buf.push_back(text);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| AppError::Process(format!("Failed to spawn reader thread: {e}")))?;

    let mut sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
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

    info!(
        "Spawned CLI: {} (tracked_id={}, os_pid={:?}, cols={}, rows={})",
        command, tracked_id, child_pid, cols, rows
    );

    Ok(ProcessInfo {
        pid: tracked_id,
        name: command.to_string(),
        path: None,
        is_running: true,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_cli_with_size(
    _command: &str,
    _args: &[String],
    _cols: u16,
    _rows: u16,
) -> Result<ProcessInfo> {
    Err(AppError::Process(
        "spawn_cli_with_size only supported on macOS".into(),
    ))
}
```

同时修改原来的 `spawn_cli` 方法让它调用 `spawn_cli_with_size`，避免代码重复：

```rust
#[cfg(target_os = "macos")]
pub fn spawn_cli(command: &str, args: &[String]) -> Result<ProcessInfo> {
    Self::spawn_cli_with_size(command, args, 80, 24)
}
```

- [ ] **Step 4: 编译验证**

Run: `cargo check -p sandbox-core`
Expected: 编译通过

- [ ] **Step 5: 运行测试**

Run: `cargo test -p sandbox-core`
Expected: 所有现有测试通过

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs crates/sandbox-core/src/server/mod.rs
git commit -m "feat(process): add spawn_cli_with_size with configurable terminal dimensions

Add cols/rows parameters to PTY creation so the frontend can specify
the correct terminal size at spawn time, avoiding the 80x24 default
that causes TUI apps like opencode to render with wrong dimensions."
```

---

### Task 2: 后端 — 添加 /sandbox/pending-cli 端点

**Files:**
- Modify: `src-tauri/src/main.rs` (AppState, setup handler)
- Modify: `crates/sandbox-core/src/server/mod.rs` (new endpoint)

- [ ] **Step 1: 在 Tauri AppState 中添加 pending_cli 字段**

在 `src-tauri/src/main.rs` 中修改 `AppState`：

```rust
#[allow(dead_code)]
struct AppState {
    sandbox: Mutex<Sandbox>,
    sandbox_id: Option<String>,
    port: Option<u16>,
    kind: Option<InstanceKind>,
    /// CLI config pending spawn — set during setup, consumed by frontend
    pending_cli: Mutex<Option<PendingCli>>,
}

#[derive(Clone, Debug)]
struct PendingCli {
    command: String,
    args: Vec<String>,
}
```

- [ ] **Step 2: 修改 Tauri setup，暂存 CLI 配置而不立即 spawn**

在 `src-tauri/src/main.rs` 的 setup handler 中，替换 auto-spawn 逻辑：

```rust
// 将 "Auto-spawn CLI if in CLI mode" 块替换为：
// Store pending CLI config — frontend will spawn with correct terminal size
if let Some(InstanceKind::Cli { command, args }) = &kind {
    let pending = PendingCli {
        command: command.clone(),
        args: args.clone(),
    };
    tracing::info!(
        "[setup] stored pending CLI: cmd={:?}, args={:?} (waiting for frontend)",
        pending.command,
        pending.args
    );
    if let Ok(mut state) = app_state_for_cli.lock() {
        state.pending_cli = Mutex::new(Some(pending));
    }
} else {
    tracing::info!("[setup] not CLI mode, no pending CLI. kind={:?}", kind);
}
```

注意：需要将 `AppState` 的引用传入 setup 闭包。当前 setup 闭包已经通过 `tauri::manage` 注册了 state，但 setup 闭包是在 manage 之前执行的。需要调整为在 manage 之后设置 pending_cli，或者将 pending_cli 存储在单独的 `Arc<Mutex<Option<PendingCli>>>` 中。

最简单的方式是在 setup 中创建一个 `Arc<Mutex<Option<PendingCli>>>`，传给 server state 和 Tauri manage：

```rust
let pending_cli = Arc::new(Mutex::new(if let Some(InstanceKind::Cli { command, args }) = &kind {
    Some(PendingCli {
        command: command.clone(),
        args: args.clone(),
    })
} else {
    None
}));

// 在 manage 中加入 pending_cli
tauri::Builder::default()
    .manage(AppState {
        sandbox: Mutex::new(Sandbox::new(config)),
        sandbox_id: sandbox_id.clone(),
        port: sandbox_port,
        kind: kind.clone(),
        pending_cli: Mutex::new(None), // 不再需要，已移到 Arc
    })
```

实际上更好的做法是：把 `pending_cli` 作为 `Arc<Mutex<Option<PendingCli>>>` 同时传给 server state 和前端。在 `server::AppState` 中添加一个字段：

```rust
pub struct AppState {
    pub sandbox_id: Option<String>,
    pub start_time: Instant,
    pub window_id: Option<u32>,
    pub target_pid: Option<u32>,
    pub pending_cli: Option<Arc<Mutex<Option<PendingCli>>>>,
}
```

不对，这样 `PendingCli` 定义在 `src-tauri` 中，server crate 无法引用。需要把 `PendingCli` 移到 `sandbox-core` 中。

**简化方案**：不修改 server::AppState，而是在 server 中添加一个独立的 static 或通过 handler 的 State 传入。最简单的做法：

在 `sandbox-core/src/server/mod.rs` 中不添加字段，而是在 Tauri 的 `main.rs` 中添加一个额外的 HTTP 路由处理 pending-cli。但这太复杂了。

**最终方案**：在 `sandbox-core` 中定义 `PendingCli` 类型，在 `server::AppState` 中添加可选字段。

- [ ] **Step 3: 在 sandbox-core 中定义 PendingCli 类型**

在 `crates/sandbox-core/src/server/mod.rs` 中，`AppState` 之前添加：

```rust
/// CLI configuration pending spawn — stored until frontend requests it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCli {
    pub command: String,
    pub args: Vec<String>,
}
```

修改 `AppState`：

```rust
pub struct AppState {
    pub sandbox_id: Option<String>,
    pub start_time: Instant,
    pub window_id: Option<u32>,
    pub target_pid: Option<u32>,
    /// CLI config pending spawn — consumed by frontend after xterm.js init
    pub pending_cli: Option<Arc<Mutex<Option<PendingCli>>>>,
}
```

- [ ] **Step 4: 添加 /sandbox/pending-cli GET 端点**

在 `build_router` 中添加路由：

```rust
.route("/sandbox/pending-cli", get(pending_cli_handler))
```

添加 handler：

```rust
async fn pending_cli_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state.lock().await;
    match &s.pending_cli {
        Some(pending) => {
            let cli = pending.lock().await;
            match cli.as_ref() {
                Some(config) => Ok(Json(serde_json::json!({
                    "command": config.command,
                    "args": config.args,
                }))),
                None => Ok(Json(serde_json::json!({ "command": null }))),
            }
        }
        None => Ok(Json(serde_json::json!({ "command": null }))),
    }
}
```

添加消费接口（spawn 后清空 pending）：

```rust
// 在 server::AppState 中添加方法或直接在 handler 中处理
```

在 `spawn_cli_handler` 中，spawn 成功后清空 pending_cli：

```rust
async fn spawn_cli_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<SpawnCliRequest>,
) -> Result<Json<crate::process::ProcessInfo>, AppError> {
    let cmd = req.command.clone();
    let cols = req.cols.unwrap_or(80);
    let rows = req.rows.unwrap_or(24);
    let info = tokio::task::spawn_blocking(move || {
        ProcessManager::spawn_cli_with_size(&req.command, &req.args, cols, rows)
    })
    .await
    .map_err(|e| AppError::Process(format!("spawn_cli panicked: {e}")))??;

    // Clear pending CLI after successful spawn
    let s = state.lock().await;
    if let Some(pending) = &s.pending_cli {
        let mut cli = pending.lock().await;
        *cli = None;
    }

    tracing::info!("spawned cli: {cmd} ({cols}x{rows})");
    Ok(Json(info))
}
```

- [ ] **Step 5: 更新 Tauri main.rs 使用新的 AppState**

修改 `src-tauri/src/main.rs` 中的 state 构造：

```rust
let pending_cli_arc = Arc::new(Mutex::new(
    if let Some(InstanceKind::Cli { command, args }) = &kind {
        Some(sandbox_core::server::PendingCli {
            command: command.clone(),
            args: args.clone(),
        })
    } else {
        None
    }
));

// ... 在 setup 中 ...
let state = Arc::new(tokio::sync::Mutex::new(sandbox_core::server::AppState {
    sandbox_id: Some(id.clone()),
    start_time: Instant::now(),
    window_id: None,
    target_pid: Some(std::process::id()),
    pending_cli: Some(pending_cli_arc.clone()),
}));
```

移除原来的 auto-spawn 块（sleep(500ms) + spawn_cli）。

- [ ] **Step 6: 编译验证**

Run: `cargo check --all`
Expected: 编译通过

- [ ] **Step 7: 运行测试**

Run: `cargo test -p sandbox-core`
Expected: 所有测试通过

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/main.rs crates/sandbox-core/src/server/mod.rs
git commit -m "feat(server): add /sandbox/pending-cli endpoint for deferred PTY spawn

Store CLI config in server state during Tauri setup instead of
auto-spawning. Frontend can query pending CLI and spawn with correct
terminal dimensions after xterm.js initialization."
```

---

### Task 3: 前端 — 启动时获取 pending CLI 并以正确尺寸 spawn

**Files:**
- Modify: `sandbox-web/src/api.ts` (添加 getPendingCli)
- Modify: `sandbox-web/src/main.tsx` (启动时 spawn 逻辑)
- Modify: `sandbox-web/src/__tests__/api.test.ts` (测试)

- [ ] **Step 1: 在 api.ts 中添加 getPendingCli 函数**

```typescript
// ── Pending CLI ──────────────────────────────────────

export interface PendingCli {
  command: string | null;
  args?: string[];
}

export async function getPendingCli(): Promise<PendingCli> {
  const res = await fetch(`${BASE()}/sandbox/pending-cli`);
  if (!res.ok) return { command: null };
  return res.json();
}
```

- [ ] **Step 2: 修改 main.tsx，在 xterm.js 初始化后 spawn CLI**

替换 main.tsx 中的 "Auto-connect to spawned processes" effect：

```typescript
// Auto-spawn pending CLI with correct terminal size, then connect
const spawnAndConnect = useCallback(async (cols: number, rows: number) => {
  try {
    // Check if there's a pending CLI to spawn
    const pending = await api.getPendingCli();
    if (pending.command) {
      console.log(`[App] spawning pending CLI: ${pending.command} (${cols}x${rows})`);
      await api.spawnCli(pending.command, pending.args || [], cols, rows);
    }
  } catch (err) {
    console.error("[App] failed to spawn pending CLI:", err);
  }
}, []);

useEffect(() => {
  const pollProcesses = async () => {
    try {
      const list = await api.listProcesses();
      if (list.length > 0) {
        setConnected(true);
        if (activePid === null && !hasConnectedRef.current) {
          const running = list.find((p) => p.is_running);
          if (running) {
            setActivePid(running.pid);
            hasConnectedRef.current = true;
          }
        }
      } else {
        setConnected(false);
      }
    } catch {
      setConnected(false);
    }
  };

  pollProcesses();
  const interval = setInterval(pollProcesses, 2000);
  return () => clearInterval(interval);
}, [activePid]);
```

将 `spawnAndConnect` 通过 props 传给 Dashboard/Terminal，在 xterm.js fit 后调用。

更简单的做法：Terminal 组件在 fit 后通过回调通知父组件。修改 Terminal.tsx 添加 `onReady?: (cols: number, rows: number) => void` prop。

- [ ] **Step 3: 修改 Terminal.tsx，添加 onReady 回调**

```typescript
interface TerminalProps {
  activePid?: number | null;
  onReady?: (cols: number, rows: number) => void;
}
```

在 xterm.js 初始化并 fit 后触发：

```typescript
// 在 fitAddon.fit() 之后
term.onReady(() => {
  props.onReady?.(term.cols, term.rows);
});
```

不对，xterm.js 没有 `onReady` 事件。应该在 `fitAddon.fit()` 之后直接调用：

```typescript
const fitAddon = new FitAddon();
term.loadAddon(fitAddon);
term.open(terminalRef.current);
fitAddon.fit();

// Notify parent of initial terminal size
onReadyRef.current?.(term.cols, term.rows);
```

需要 `onReadyRef`：

```typescript
const onReadyRef = useRef(props.onReady);
onReadyRef.current = props.onReady;
```

- [ ] **Step 4: 修改 spawnCli API 支持 cols/rows**

```typescript
export async function spawnCli(
  command: string,
  args: string[],
  cols?: number,
  rows?: number,
): Promise<ProcessInfo> {
  const body: Record<string, unknown> = { command, args };
  if (cols !== undefined) body.cols = cols;
  if (rows !== undefined) body.rows = rows;
  const res = await fetch(`${BASE()}/cli/spawn`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`spawnCli failed: ${body}`);
  }
  return res.json();
}
```

- [ ] **Step 5: 修改 Dashboard 传递 onReady**

Dashboard.tsx 添加 `onSpawnReady?: (cols: number, rows: number) => void` prop，传给 SandboxTerminal：

```typescript
<SandboxTerminal activePid={activePid} onReady={onSpawnReady} />
```

main.tsx 传入：

```typescript
<Dashboard
  ...
  onSpawnReady={spawnAndConnect}
>
```

- [ ] **Step 6: TypeScript 编译验证**

Run: `cd sandbox-web && npx tsc --noEmit`
Expected: 编译通过

- [ ] **Step 7: 运行前端测试**

Run: `pnpm test:unit`
Expected: 测试通过

- [ ] **Step 8: Commit**

```bash
git add sandbox-web/src/api.ts sandbox-web/src/main.tsx sandbox-web/src/components/Terminal.tsx sandbox-web/src/components/Dashboard.tsx sandbox-web/src/__tests__/*
git commit -m "feat(frontend): spawn CLI with correct terminal size on init

Replace process polling with: getPendingCli → fit xterm → spawnCli(cols, rows).
The PTY is now created with the actual xterm.js container dimensions,
so TUI apps like opencode render correctly from the start."
```

---

### Task 4: CLI 二进制 — serve 命令立即 spawn（无前端场景）

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs` (cmd_serve 函数)

- [ ] **Step 1: 在 serve 命令中立即 spawn CLI**

在 `crates/sandbox-cli/src/main.rs` 的 `cmd_serve` 函数中，启动 HTTP 服务器后立即 spawn CLI 进程（使用默认 80x24，因为没有前端来提供正确尺寸）：

```rust
// 在 HTTP server 启动后添加：
if let Some(command) = command {
    let args = args.unwrap_or_default();
    match ProcessManager::spawn_cli(&command, &args) {
        Ok(info) => println!("Spawned CLI: {} (pid={})", command, info.pid),
        Err(e) => eprintln!("Failed to spawn CLI: {e}"),
    }
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p sandbox-cli`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add crates/sandbox-cli/src/main.rs
git commit -m "fix(cli): spawn CLI immediately in serve mode (no frontend)

When using standalone serve mode without Tauri, spawn the CLI process
immediately with default 80x24 dimensions since there's no frontend
to provide the correct terminal size."
```

---

### Task 5: 端到端验证

**Files:** 无代码修改，纯验证

- [ ] **Step 1: 构建 release**

```bash
pnpm build && cargo build --release -p sandbox-cli && cargo tauri build
```

- [ ] **Step 2: 启动 opencode 并截图**

```bash
./target/release/cli-box start opencode
sleep 8
./target/release/cli-box screenshot --id <id> -o release_test/ws_deferred_pty.png
```

预期：opencode TUI 填满整个终端区域，无右侧/底部空白。

- [ ] **Step 3: 启动 zsh 验证回归**

```bash
./target/release/cli-box start zsh
sleep 5
./target/release/cli-box screenshot --id <id> -o release_test/ws_deferred_zsh.png
```

预期：zsh prompt 正常显示。

- [ ] **Step 4: 运行全量测试**

```bash
cargo test --all && pnpm test:unit && pnpm typecheck
```

预期：全部通过。

- [ ] **Step 5: 完成 — 无额外 commit**
