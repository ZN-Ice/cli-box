# system-test-sandbox — macOS 桌面自动化沙箱

> **核心理念**：一个可复用的 macOS 桌面自动化沙箱，支持多实例管理——通过 CLI 命令启动独立沙箱窗口，在其中运行任意 CLI 或 macOS 应用，并通过模拟鼠标/键盘操作与截图反馈进行自动化控制。
>
> 对目标应用**零侵入**，所有操作在 OS 层面完成（CGEvent + AXUIElement + ScreenCaptureKit）。
>
> **多实例架构**：每个沙箱是一个独立的 Tauri 窗口进程，拥有唯一 ID、内嵌 HTTP API 服务器，通过文件系统注册中心（`~/.sandbox/instances/`）进行实例发现和管理。

## 一、架构总览

```
┌──────────────────────────────────────────────────────────────┐
│                  Agent / 用户 (CLI / MCP / HTTP)              │
│                                                              │
│  sandbox-cli start --cli "claude"        → 返回 sandbox-id   │
│  sandbox-cli list                        → 列出所有实例       │
│  sandbox-cli screenshot <id>             → 截取沙箱截图       │
│  sandbox-cli click <id> 100 200          → 模拟点击           │
│  sandbox-cli close <id>                  → 关闭沙箱           │
└──────────────────────┬───────────────────────────────────────┘
                       │ CLI (子进程启动) / MCP stdio / HTTP
                       ▼
┌──────────────────────────────────────────────────────────────┐
│              沙箱实例注册中心 (~/.sandbox/instances/)         │
│                                                              │
│  ┌─────────────────────┐  ┌─────────────────────┐            │
│  │ Sandbox Instance #1 │  │ Sandbox Instance #2 │  ...       │
│  │ id: abc123          │  │ id: def456          │            │
│  │ port: 15801         │  │ port: 15802         │            │
│  │ mode: cli (claude)  │  │ mode: app (cc-switch)│           │
│  │ status: Running     │  │ status: Running     │            │
│  └─────────┬───────────┘  └─────────┬───────────┘            │
│            │                         │                        │
│            │ HTTP :15801             │ HTTP :15802            │
└────────────┼─────────────────────────┼────────────────────────┘
             │                         │
             ▼                         ▼
   ┌──────────────────┐    ┌──────────────────┐
   │  Tauri Window #1  │    │  Tauri Window #2  │
   │  "System Test     │    │  "System Test     │
   │   Sandbox [abc]"  │    │   Sandbox [def]"  │
   │                  │    │                  │
   │  ┌────────────┐  │    │  ┌────────────┐  │
   │  │ xterm.js   │  │    │  │ App 关联   │  │
   │  │ (claude)   │  │    │  │ (cc-switch)│  │
   │  └────────────┘  │    │  └────────────┘  │
   │                  │    │                  │
   │  内嵌 HTTP API   │    │  内嵌 HTTP API   │
   │  + Automation    │    │  + Automation    │
   │  Engine          │    │  Engine          │
   └──────────────────┘    └──────────────────┘
             │                         │
             ▼                         ▼
      ┌─────────────┐          ┌─────────────────┐
      │  CLI 进程    │          │  macOS .app      │
      │  (PTY)      │          │  (NSWorkspace)   │
      └─────────────┘          └─────────────────┘
```

**设计原则**：
1. **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成
2. **多实例**：每个沙箱是独立的 Tauri 窗口进程，通过 CLI 管理生命周期
3. **窗口级截图**：ScreenCaptureKit 按窗口 ID 截图，不需要窗口在前台
4. **双协议**：MCP (Agent CLI 原生) + HTTP (通用调用)
5. **文件系统注册中心**：沙箱实例通过 `~/.sandbox/instances/<id>.json` 注册和发现

## 二、技术栈

| 项目属性 | 规范值 |
|---------|--------|
| 核心库 | Rust (Edition 2021, >=1.88), `sandbox-core` library crate |
| CLI | Rust, `sandbox-cli` binary crate |
| 桌面框架 | Tauri 2.x |
| 桌面前端 | React 18 + TS + Vite + TailwindCSS + xterm.js |
| 异步运行时 | tokio |
| macOS API | CoreGraphics (CGEvent), ApplicationServices (AXUIElement), ScreenCaptureKit |
| 包管理 | Cargo Workspace + pnpm |
| 测试 | cargo test (Rust) + vitest (TS) |
| 目标平台 | macOS (Apple Silicon 优先) |
| License | Apache 2.0 |

## 三、目录结构

```
system-test-sandbox/
├── Cargo.toml                    # Workspace 根
├── crates/
│   ├── sandbox-core/             # 🔑 自动化核心 (library)
│   │   └── src/
│   │       ├── lib.rs, error.rs
│   │       ├── automation/       # CGEvent 输入模拟 + AXUIElement UI 检查
│   │       │   ├── cg_event.rs
│   │       │   └── ax_ui.rs
│   │       ├── capture/          # ScreenCaptureKit 截图引擎
│   │       │   └── mod.rs
│   │       ├── process/          # PTY + NSWorkspace 进程管理
│   │       │   └── mod.rs
│   │       ├── sandbox/          # 沙箱窗口管理 (含多实例支持)
│   │       │   └── mod.rs
│   │       ├── instance/         # NEW: 沙箱实例注册中心
│   │       │   └── mod.rs
│   │       └── server/           # NEW: HTTP API 服务器 (library)
│   │           └── mod.rs
│   └── sandbox-cli/              # 🖥️ CLI (binary)
│       └── src/
│           ├── main.rs           # start/list/close + 所有子命令
│           ├── client.rs         # NEW: HTTP 客户端 (与沙箱实例通信)
│           └── mcp_server.rs     # MCP stdio 服务器
├── sandbox-web/                  # 🌐 沙箱窗口前端 (xterm.js + React)
│   └── src/
│       ├── main.tsx, api.ts
│       └── components/           # Terminal, ControlPanel, StatusBar, RecordControls
├── src-tauri/                    # 🖥️ macOS 宿主应用 (Tauri)
│   └── src/main.rs              # 多实例支持：CLI 参数解析 + 内嵌 HTTP API
├── docs/
│   ├── design/                   # 设计文档
│   └── task/                     # 任务管理 (README.md + phase-*.md + task_records.json)
├── tests/
│   └── fixtures/                 # 集成测试场景 (YAML/JSON)
└── config.example.json
```

## 四、核心接口

### CLI 命令 (sandbox-cli)

```bash
# 多实例管理
sandbox-cli start --cli "claude"              # 启动沙箱，运行 Claude Code，返回 sandbox-id
sandbox-cli start --cli "echo" -- "hello"     # 带参数启动 CLI
sandbox-cli start --app "/path/to/App.app"    # 启动沙箱，运行 macOS 应用
sandbox-cli list                              # 列出所有活跃沙箱及其状态
sandbox-cli close <sandbox-id>                # 关闭指定沙箱
sandbox-cli inspect <sandbox-id>              # 查看沙箱详情

# 沙箱作用域操作 (通过 --id 指定目标沙箱)
sandbox-cli screenshot --id <id>              # 截取沙箱截图
sandbox-cli screenshot --id <id> -o result.png  # 截图并指定输出路径
sandbox-cli click --id <id> 100 200           # 在沙箱内模拟点击
sandbox-cli type --id <id> "hello world"      # 在沙箱内模拟输入
sandbox-cli key --id <id> Return --modifiers cmd  # 在沙箱内模拟按键

# 进程管理 (沙箱内)
sandbox-cli windows --id <id>                 # 列出沙箱内窗口
sandbox-cli processes --id <id>               # 列出沙箱内进程
sandbox-cli spawn-cli --id <id> "npm" -- "test"  # 在沙箱内启动新的 CLI
sandbox-cli kill --id <id> <pid>              # 终止沙箱内进程

# 独立模式 (无多实例，向后兼容)
sandbox-cli serve --port 5801                 # 启动独立 HTTP + MCP 服务器
sandbox-cli mcp-serve                         # MCP stdio 模式
```

### 实例注册中心 (文件系统)

```
~/.sandbox/instances/
├── abc123.json    # {id, port, pid, kind, title, status, created_at, window_id}
├── def456.json
└── ...
```

### MCP Tools (Agent 调用)

```yaml
# 沙箱实例管理 (NEW)
list_sandboxes:       # 列出所有活跃沙箱
start_sandbox:        # 启动新沙箱 (--cli/--app)
close_sandbox:        # 关闭指定沙箱

# 窗口管理
list_windows:         # 列出沙箱内所有窗口
find_window:          # 按 app 名/标题查找
focus_window:         # 聚焦指定窗口

# 进程管理
spawn_app:            # 启动 .app (如 Hi Boss.app)
spawn_cli:            # 启动 CLI 进程 (如 hiboss)
kill_process:         # 终止进程
list_processes:       # 列出沙箱内进程

# 输入模拟
click:                # 鼠标点击 (x, y, button)
double_click:         # 双击
type_text:            # 输入文本
press_key:            # 按键 (Return, Tab, etc.)
scroll:               # 滚动
drag:                 # 拖拽

# 截图 (核心)
screenshot:           # 截取沙箱窗口 (base64 PNG)
screenshot_window:    # 截取沙箱内指定子窗口
screenshot_region:    # 截取沙箱内指定区域

# UI 检查 (高级)
inspect_ui:           # 读取 AX 树
find_element:         # 按 role/title 查找 UI 元素
```

### HTTP API (每实例独立端口 `:5801`/`:15802`/etc.)

```
GET  /health                     健康检查
GET  /sandbox/info               沙箱信息 (id, mode, running process)
POST /shutdown                   关闭沙箱
GET  /windows                    列出窗口
GET  /processes                  列出进程
POST /app/spawn                  启动 .app
POST /cli/spawn                  启动 CLI
POST /input/click                鼠标点击
POST /input/type                 键盘输入
POST /input/key                  按键
POST /input/scroll               滚动
POST /input/drag                 拖拽
GET  /screenshot                 截取沙箱窗口 (PNG)
GET  /screenshot/:window_id      截取指定窗口
GET  /screenshot/region          截取指定区域
GET  /ui/inspect/:window_id      读取 UI 树
POST /ui/find                    查找 UI 元素
POST /record/start               开始录制
POST /record/stop                停止录制
POST /playback/actions           回放操作
POST /scenario/run               运行测试场景
POST /diff                       截图差异对比
POST /pty/write                  写入 PTY
GET  /pty/output/:pid            读取 PTY 输出
```

### Rust API (sandbox-core)

```rust
// 实例管理
use sandbox_core::instance::{InstanceRegistry, SandboxInstance, generate_instance_id};
let registry = InstanceRegistry::default();
let instance = SandboxInstance::new(id, port, kind);
registry.register(&instance)?;
let all_instances = registry.list()?;
registry.unregister("abc123")?;

// 输入模拟
use sandbox_core::automation::cg_event::InputSimulator;
InputSimulator::click(100.0, 200.0, MouseButton::Left)?;
InputSimulator::type_text("Hello")?;
InputSimulator::press_key("Return", &["cmd"])?;

// 截图
use sandbox_core::capture::ScreenCapture;
let png_bytes = ScreenCapture::capture_window(window_id)?;

// UI 检查
use sandbox_core::automation::ax_ui::UiInspector;
let tree = UiInspector::inspect_window(window_id)?;

// 进程管理
use sandbox_core::process::ProcessManager;
ProcessManager::spawn_app("/path/to/App.app")?;
ProcessManager::spawn_cli("claude", &["--help".into()])?;

// 沙箱管理 (多实例)
use sandbox_core::sandbox::{Sandbox, SandboxConfig};
let config = SandboxConfig {
    id: Some("abc123".into()),
    port: Some(15801),
    mode: Some("cli".into()),
    command: Some("claude".into()),
    ..SandboxConfig::default()
};
let mut sandbox = Sandbox::new(config);
sandbox.init(window_id)?;
let screenshot = sandbox.screenshot()?;
```

## 五、macOS 权限要求

| 权限 | 用途 | 授予方式 |
|------|------|---------|
| **Accessibility** | CGEvent 输入模拟、AXUIElement 读取 | 系统设置 → 隐私与安全 → 辅助功能 |
| **Screen Recording** | ScreenCaptureKit 截图 | 系统设置 → 隐私与安全 → 屏幕录制 |

**注意**：这两个权限必须用户手动授予，无法通过代码自动获取。建议不经过 App Store，直接分发 .dmg。

## 六、Git 规范

```
格式：<type>(<scope>): <description>

scope: sandbox | automation | capture | process | server | cli | ui

示例：
feat(sandbox): 实现沙箱窗口管理
feat(automation): CGEvent 鼠标点击模拟
feat(capture): ScreenCaptureKit 窗口截图
fix(server): 修复 HTTP API 端口冲突
```

## 七、核心工作流程

### 7.1 任务执行流程

```
[1]创建任务记录(待执行) → [2]创建特性分支 → [3]编写设计文档（写到docs/design）
   → [4]编写测试(RED) → [5]编码实现(GREEN) → [6]重构优化(REFACTOR)
   → [7]本地检查(fmt+clippy+check+typecheck) → [8]验证测试覆盖率
   → [9]更新任务状态(已完成,必须在push前) → [10]git commit → [11]git push
   → [12]创建 PR → [13]等待 CI 门禁通过
```

### 7.2 沙箱使用流程

```bash
# 1. 启动沙箱（运行 Claude Code 终端）
sandbox-cli start --cli "claude"
# → 自动打开 "System Test Sandbox" 窗口，xterm.js 中运行 claude
# → 输出: Sandbox started: abc123

# 2. 启动沙箱（运行 macOS 应用）
sandbox-cli start --app "/Applications/cc-switch.app"
# → 打开沙箱窗口，启动 cc-switch，关联其窗口
# → 输出: Sandbox started: def456

# 3. 查看所有沙箱
sandbox-cli list
# → ID      TITLE              KIND  STATUS   PORT   CREATED
# → abc123  "claude"           CLI   Running  15801  2026-05-16 10:30
# → def456  "cc-switch"        APP   Running  15802  2026-05-16 10:31

# 4. 操作指定沙箱
sandbox-cli screenshot --id abc123 -o sandbox.png  # 截图
sandbox-cli click --id abc123 100 200               # 点击
sandbox-cli type --id abc123 "帮我写一个函数"        # 输入文本
sandbox-cli key --id abc123 Return                  # 按键

# 5. 关闭沙箱
sandbox-cli close abc123
# → 关闭沙箱窗口，清理注册信息，终止关联进程
```

### 7.3 命令序列

```bash
# 本地检查
cargo fmt --all -- --check && cargo clippy --all-targets \
  && cargo check --all-targets && cargo test --all \
  && pnpm typecheck && pnpm format:check && pnpm test:unit

# 构建 Tauri 应用
cd sandbox-web && pnpm install && pnpm build && cd ..
cargo build --release -p system-test-sandbox

# 使用 CLI 启动沙箱
cargo run -p sandbox-cli -- start --cli "claude"

# 通过 HTTP 直接调用 (已知端口)
curl http://127.0.0.1:5801/screenshot -o screenshot.png
curl -X POST http://127.0.0.1:5801/input/click \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200, "button": "left"}'

# Agent 通过 MCP 调用 (Claude Code 配置)
# .claude/settings.json 中添加 MCP server 配置
```

## 八、安全约束

- ✅ 沙箱窗口是独立 NSWindow，截图范围可控
- ✅ ScreenCaptureKit 按窗口 ID 截图，不截全屏
- ✅ 目标应用不需要任何适配
- ✅ Accessibility 和 Screen Recording 权限需用户手动授权
- ✅ 每实例 HTTP API 仅监听 `127.0.0.1`，不暴露外部网络
- ✅ 实例注册中心仅存储在本地文件系统 `~/.sandbox/instances/`
- ✅ 沙箱关闭时自动清理注册信息并终止关联进程

## 目录速查

| 内容 | 路径 |
|------|------|
| 核心库 | `/crates/sandbox-core/src/` |
| 实例管理 | `/crates/sandbox-core/src/instance/` |
| HTTP 服务器 | `/crates/sandbox-core/src/server/` |
| CLI 入口 | `/crates/sandbox-cli/src/main.rs` |
| HTTP 客户端 | `/crates/sandbox-cli/src/client.rs` |
| Tauri 宿主 | `/src-tauri/src/main.rs` |
| 沙箱前端 | `/sandbox-web/src/` |
| 前端 API 层 | `/sandbox-web/src/api.ts` |
| 设计文档 | `/docs/design/` |
| 任务管理 | `/docs/task/` |
| 本文件 | `/CLAUDE.md` |

---

**版本**：v0.2.0 | **创建**：2026-05-13 | **更新**：2026-05-16 | **维护者**：system-test-sandbox 项目
