# 终端实现对比分析：cli-box vs waveterm

> 分析日期：2026-05-27
> 对比范围：PTY 管理、zsh 处理、子进程（claude/opencode）管理、前端渲染

---

## 一、架构对比总览

| 维度 | cli-box | waveterm |
|------|---------------------|----------|
| 桌面框架 | Tauri 2.x (Rust) | Electron (Node.js) |
| 后端语言 | Rust | Go |
| PTY 库 | `portable-pty` (Rust) | `creack/pty` (Go) |
| 前端渲染 | xterm.js (React) | xterm.js (React) |
| 进程间通信 | WebSocket (axum) | RPC (domain socket / SSH) |
| 数据持久化 | SQLite (PtyStore) | FileStore (文件系统) |
| 终端管理模型 | 单窗口单 PTY | 多 Block 多 PTY |
| Shell 集成 | 无（仅设置环境变量） | 深度集成（OSC 16162 + wsh） |

### 架构差异图

```
cli-box:
┌──────────────┐     WebSocket      ┌──────────────┐     PTY      ┌──────────┐
│  xterm.js    │ ◄───────────────► │  axum HTTP   │ ◄──────────► │  zsh     │
│  (React)     │  ws://.../pty/ws  │  server      │  portable-pty│  (child) │
└──────────────┘                    └──────────────┘              └──────────┘
                                           │
                                     ┌─────┴─────┐
                                     │  PtyStore  │  (SQLite)
                                     └───────────┘

waveterm:
┌──────────────┐     RPC (domain socket)  ┌──────────────┐     PTY      ┌──────────┐
│  xterm.js    │ ◄───────────────────────►│  Go backend  │ ◄──────────► │  zsh     │
│  (React)     │  ControllerInputCommand  │  wsh RPC     │  creack/pty  │  (child) │
│  + TermWrap  │                          │              │              │  + wsh   │
└──────────────┘                          └──────┬───────┘              └──────────┘
       ▲                                         │                          │
       │  PubSub (BlockFile)                     │                          │
       │                                         ▼                          ▼
       │                                   ┌──────────┐             ┌──────────┐
       └───────────────────────────────────│ FileStore│             │  wsh     │
                                           └──────────┘             │ (helper) │
                                                                    └──────────┘
```

---

## 二、PTY 创建与管理

### 2.1 PTY 创建流程对比

**cli-box**（`crates/sandbox-core/src/process/mod.rs`）：

```
1. portable_pty::native_pty_system().open(PtySize{rows, cols})
2. 设置环境变量: TERM=xterm-256color, COLORTERM=truecolor, LANG=en_US.UTF-8
3. pty_pair.slave.spawn_command(cmd) -- fork 子进程
4. master.try_clone_reader() / master.take_writer() -- 分离读写端
5. 创建 PtyStore (SQLite) + broadcast::channel(256)
6. 启动专用 reader thread (std::thread)
7. 注册到全局 SESSIONS HashMap
```

**waveterm**（`pkg/shellexec/shellexec.go`）：

```
1. 检测连接类型（Local/SSH/WSL）
2. 设置 ZDOTDIR 指向 ~/.waveterm/shell/zsh/
3. creack/pty.StartWithSize(cmd, &pty.Winsize{Rows, Cols})
4. 获取 stdin/stdout pipe
5. 启动 ShellController 的三个 goroutine:
   - PTY read goroutine → HandleAppendBlockFile()
   - Input goroutine → 从 ShellInputCh channel 读取写入 PTY
   - Wait goroutine → 等待进程退出
6. 注册到 BlockController
```

### 2.2 关键差异

| 方面 | cli-box | waveterm |
|------|---------------------|----------|
| PTY 库 | `portable-pty`（跨平台抽象） | `creack/pty`（Unix 原生） |
| 读取模型 | 专用 `std::thread` + broadcast channel | Go goroutine + FileStore append |
| 输出缓冲 | SQLite (PtyStore, 10MB 循环) | 文件系统 (FileStore, 2MB 循环) |
| 写入模型 | 直接 `writer.write_all()` | 通过 channel (ShellInputCh, cap=32) |
| 全局状态 | `LazyLock<Mutex<HashMap>>` | Block 级别的 ShellController |

**设计哲学差异**：
- cli-box 使用 **SQLite 作为 PTY 输出持久层**，天然支持重连回放、offset 查询
- waveterm 使用 **文件系统 + PubSub**，通过 FileStore 追加写入 + 事件订阅实现类似功能

---

## 三、zsh 处理方案对比

### 3.1 cli-box 的 zsh 处理

**极简方案**——不做任何 shell 集成：

1. **环境变量设置**（`process/mod.rs` lines 153-162）：
   ```
   TERM=xterm-256color
   COLORTERM=truecolor
   LANG=en_US.UTF-8
   ```
   这是唯一与 shell 交互的地方。目的是确保 TUI 应用（vim, htop, claude, opencode）在 Tauri（GUI 进程）环境下也能正确渲染。

2. **直接启动 zsh**：`spawn_command(CommandBuilder::new("zsh"))`，不做任何 shell 配置注入。

3. **无 shell 集成**：不追踪命令、不追踪 CWD、不注入 hook。

### 3.2 waveterm 的 zsh 处理

**深度集成方案**——通过 ZDOTDIR 重写 + wsh helper + OSC 16162：

#### ZDOTDIR 重写机制

```
~/.waveterm/shell/zsh/
├── .zshenv     # 保存原始 ZDOTDIR，source 原始 ~/.zshenv
├── .zprofile   # source 原始 ~/.zprofile
├── .zshrc      # 核心集成文件：注入 wsh 到 PATH、设置 shell hook
└── .zlogin     # source 原始 ~/.zlogin，恢复 ZDOTDIR
```

启动时设置 `ZDOTDIR=~/.waveterm/shell/zsh/`，让 zsh 加载 waveterm 的集成脚本，这些脚本在加载完毕后会 source 用户原始的配置文件。

#### Shell Integration Hook

在 `.zshrc` 中注入以下 hook：

| Hook | 功能 | OSC 序列 |
|------|------|---------|
| `precmd` | 检测 shell 就绪，报告上一条命令退出码 | `OSC 16162;A` + `OSC 16162;D;{exitcode}` |
| `preexec` | 检测命令开始执行，报告命令内容 | `OSC 16162;C;{base64(cmd)}` |
| `chpwd` | 追踪 CWD 变化 | `OSC 7;file://localhost/{path}` |
| `zle-line-init` | 输入缓冲区状态 | `OSC 16162;I;{empty}` |

#### Token Swap 机制

每次 shell 启动时：
1. 创建一个 SwapToken（UUID + 5分钟有效期 + 环境变量）
2. `.zshrc` 中执行 `wsh token $WAVETERM_SWAPTOKEN zsh` 设置 RPC 上下文
3. wsh 在 shell 内提供 RPC 能力（执行命令、访问文件系统等）

### 3.3 差异总结

| 方面 | cli-box | waveterm |
|------|---------------------|----------|
| Shell 初始化 | 无特殊处理 | ZDOTDIR 重写 + 集成脚本 |
| 命令追踪 | 无 | precmd/preexec hook via OSC 16162 |
| CWD 追踪 | 无 | OSC 7 |
| Shell helper | 无 | wsh（RPC bridge） |
| 剪贴板集成 | 无 | OSC 52 |
| 环境变量注入 | TERM/COLORTERM/LANG | 完整环境（含 TERM_PROGRAM=waveterm 等） |
| 对用户配置影响 | 零（完全不修改） | 低（source 原始配置，但注入 hook） |

---

## 四、进入 claude/opencode 等子进程的处理

### 4.1 cli-box 的处理方式

**完全透明**——PTY 将子进程视为纯粹的 I/O 流：

1. **启动方式**：用户在 zsh 中手动输入 `claude` 或 `opencode`
2. **输入传递**：xterm.js `onData` → WebSocket → PTY master write → zsh → claude/opencode
3. **输出回传**：claude/opencode 的 stdout/stderr → PTY → reader thread → broadcast + SQLite → WebSocket → xterm.js
4. **无特殊处理**：不检测子进程类型，不做任何适配

**优点**：
- 实现简单，对所有 TUI 应用一视同仁
- 不依赖子进程的特殊行为

**缺点**：
- 无法知道当前运行的是什么命令
- 无法在自动化场景中精确检测 claude/opencode 的状态
- 如果需要针对特定应用做优化（如 claude 的截图反馈），需要在 PTY 外部实现

### 4.2 waveterm 的处理方式

**主动检测 + 状态感知**——通过 shell integration 感知子进程：

1. **命令检测**：`preexec` hook 在用户按回车时捕获命令，base64 编码后通过 OSC 16162 发送给前端
2. **Claude Code 特殊检测**（`shellblocking.ts`）：
   ```typescript
   const ClaudeCodeRegex = /^claude\b/;
   ```
   检测到 claude 后：设置 `claudeCodeActiveAtom`，显示 Claude 图标，记录遥测
3. **Shell Blocking 机制**：当检测到 TUI 应用运行时，标记 shell 为 "blocking" 状态：
   - **总是阻塞的命令**：tmux, screen, vim, nvim, emacs, htop, top, less, more, fzf 等
   - **裸 REPL**（仅无参数时阻塞）：python, node, ruby, irb 等
   - **包装器**（剥离后检查）：sudo, env, time, nohup 等
   - **SSH/Docker 交互检测**：ssh -t, docker attach/exec -it
4. **Alternate Screen 检测**：任何在 alternate screen buffer 中运行的命令都被视为阻塞
5. **状态图标**：UI 显示当前 shell 状态（ready/running/blocking），用户可直观看到

**优点**：
- 知道当前运行的是什么命令
- 可以针对特定应用做 UI 适配（如 claude 图标）
- shell 状态感知让用户和自动化程序都能理解终端状态

**缺点**：
- 依赖 shell integration hook，仅在支持的 shell 中有效
- 如果子进程改变了 shell 类型（如进入 tmux），hook 可能失效

### 4.3 差异总结

| 方面 | cli-box | waveterm |
|------|---------------------|----------|
| 子进程检测 | 无 | preexec hook + 命令解析 |
| Claude 识别 | 无 | 正则匹配 + 专用 UI |
| TUI 状态感知 | 无 | Shell Blocking 机制 |
| Alternate Screen | 由 xterm.js 自动处理 | xterm.js + 阻塞状态标记 |
| 自动化支持 | 依赖截图 + OCR | 知道精确的命令和状态 |

---

## 五、数据流详细对比

### 5.1 输出流（PTY → 前端）

**cli-box**：
```
child stdout → PTY master FD
  → reader thread (std::thread, 4KB buffer)
    → PtyStore.append() [SQLite, 持久化]
    → broadcast::Sender::send() [实时推送]
      → WebSocket send_task
        → ws message
          → xterm.js write()
```

**waveterm**：
```
child stdout → PTY
  → goroutine read loop
    → HandleAppendBlockFile() [FileStore append]
      → wps.Broker.Publish() [PubSub 事件]
        → mainFileSubject.subscribe() [前端订阅]
          → doTerminalWrite()
            → xterm.js write()
```

### 5.2 输入流（前端 → PTY）

**cli-box**：
```
xterm.js onData → ws.send(text)
  → WebSocket recv_task → ProcessManager::send_input()
    → PtySession.writer.write_all() → PTY master → child stdin
```

**waveterm**：
```
xterm.js onData → handleTermData() → base64 encode
  → RPC ControllerInputCommand → ShellInputCh channel (cap=32)
    → input goroutine → shellProc.Cmd.Write() → PTY → child stdin
```

### 5.3 Resize 流

**cli-box**：
```
window resize → FitAddon.fit() → ws.send({"type":"resize","cols","rows"})
  → WebSocket → ProcessManager::resize_pty() → pty_master.resize()
```

**waveterm**：
```
ResizeObserver → fitAddon.fit() → RPC ControllerInputCommand{TermSize}
  → ShellInputCh → input goroutine → pty.Setsize()
```

---

## 六、重连与回放机制对比

### cli-box

WebSocket 重连时，`handle_pty_ws` 的 Phase 1：
1. 从 PtyStore 读取全部历史输出（`store.read_all()`）
2. 逐条发送给新的 WebSocket 连接
3. 然后订阅 broadcast channel 接收实时数据

**特点**：SQLite 支持精确的 offset 查询，回放效率高。

### waveterm

前端 TermWrap 的 `loadInitialTerminalData()`：
1. 先加载 `cache:term:full`（xterm.js 序列化的完整终端状态）
2. 计算与 FileStore 最新数据的 offset 差
3. 追加缺失的新数据

**特点**：利用 xterm.js 的 SerializeAddon 缓存渲染状态，恢复更快（不需要重新解析所有 ANSI 序列）。

---

## 七、前端 xterm.js 使用对比

| 方面 | cli-box | waveterm |
|------|---------------------|----------|
| xterm.js 版本 | `@xterm/xterm` (v5+) | `@xterm/xterm` |
| Addons | FitAddon | FitAddon, SearchAddon, SerializeAddon, WebLinksAddon, WebglAddon |
| 渲染加速 | 无（默认 Canvas） | WebGL（可选，fallback DOM） |
| 终端缓存 | 无（每次重连重放全量） | SerializeAddon 序列化 + 增量追加 |
| 主题管理 | React Context | Jotai atom + 自定义主题系统 |
| 搜索功能 | 无 | SearchAddon |
| 链接检测 | 无 | WebLinksAddon |
| 同步输出 (mode 2026) | xterm.js 内置支持 | xterm.js 内置 + 应用层状态追踪 |

---

## 八、对我们项目的启示

### 8.1 可以借鉴的地方

1. **Shell Integration（OSC 16162）**：waveterm 的 precmd/preexec hook 机制让我们可以精确知道终端中正在运行的命令，这对自动化场景（检测 claude 是否已启动、opencode 是否已就绪）非常有价值。

2. **Claude Code 检测**：`shellblocking.ts` 中的命令解析逻辑可以直接复用，用于自动化控制时判断子进程状态。

3. **SerializeAddon**：xterm.js 的 SerializeAddon 可以大幅加速重连恢复，比从 SQLite 重放全量 ANSI 数据更高效。

4. **WebGL 渲染**：对于 claude/opencode 这类输出量大的 TUI 应用，WebGL 渲染可以显著提升性能。

### 8.2 当前方案的优势

1. **零侵入**：不修改用户 shell 配置，不需要额外安装 helper 工具
2. **SQLite 持久化**：PtyStore 的 offset 查询比 FileStore 更精确，适合自动化回放
3. **WebSocket 直连**：比 RPC over domain socket 更简单，更适合 HTTP API 场景
4. **延迟启动**：等待 xterm.js 报告实际尺寸后再 spawn PTY，避免尺寸不匹配问题

### 8.3 改进建议

1. **添加 Shell Integration**：参考 waveterm 的 OSC 16162 方案，添加 precmd/preexec hook，实现命令追踪和状态感知。这对自动化场景至关重要。

2. **添加 SerializeAddon**：缓存 xterm.js 的终端状态，加速 WebSocket 重连时的恢复。

3. **考虑 WebGL 渲染**：对于 claude/opencode 等 TUI 应用，WebGL 可以显著减少 CPU 占用。

4. **TUI 检测机制**：参考 waveterm 的 shellblocking，检测 alternate screen buffer 状态，在自动化 API 中暴露当前运行的命令信息。

---

## 附录：关键文件索引

### cli-box

| 文件 | 功能 |
|------|------|
| `crates/sandbox-core/src/process/mod.rs` | PTY 会话管理、spawn/kill/input/output/resize |
| `crates/sandbox-core/src/pty_store.rs` | SQLite 循环缓冲区 |
| `crates/sandbox-core/src/server/mod.rs` | HTTP/WS 端点 |
| `sandbox-web/src/components/Terminal.tsx` | xterm.js React 组件 |
| `sandbox-web/src/api.ts` | WebSocket 客户端 |
| `src-tauri/src/main.rs` | Tauri 入口 + HTTP 服务器 |

### waveterm

| 文件 | 功能 |
|------|------|
| `pkg/shellexec/shellexec.go` | PTY 创建和 shell 进程管理 |
| `pkg/blockcontroller/shellcontroller.go` | Shell 控制器（三个 goroutine） |
| `pkg/util/shellutil/shellintegration/zsh_zshrc.sh` | zsh 集成脚本 |
| `frontend/app/view/term/termwrap.ts` | TermWrap (xterm.js 封装) |
| `frontend/app/view/term/osc-handlers.ts` | OSC 7/52/16162 处理 |
| `frontend/app/view/term/shellblocking.ts` | TUI 应用检测 |
