# system-test-sandbox — macOS 桌面自动化沙箱

> **核心理念**：一个可复用的 macOS 桌面自动化沙箱，让 Agent CLI 工具可以模拟人类操作任意 macOS 应用和 CLI，并获取截图反馈。
>
> 对目标应用**零侵入**，所有操作在 OS 层面完成（CGEvent + AXUIElement + ScreenCaptureKit）。

## 一、架构总览

```
┌──────────────────────────────────────────────────────┐
│              Agent CLI (Claude Code / OpenCode)       │
│                                                      │
│  Agent 调用 MCP tools / HTTP API:                     │
│    → screenshot()     → 返回 base64 PNG               │
│    → click(x, y)      → 模拟点击                      │
│    → type_text(text)  → 模拟输入                      │
│    → spawn_cli(cmd)   → 启动 CLI 进程                 │
└──────────────────────┬───────────────────────────────┘
                       │ MCP stdio / HTTP (:5801)
┌──────────────────────┴───────────────────────────────┐
│              Sandbox Host App (Tauri 2)               │
│                                                      │
│  ┌─────────────────────────────────────────────────┐ │
│  │              Sandbox Window (NSWindow)            │ │
│  │                                                  │ │
│  │  固定尺寸 (1280x800)                              │ │
│  │  包含:                                            │ │
│  │    - xterm.js 终端 (CLI 运行区)                   │ │
│  │    - 内嵌视图 (macOS App 渲染区)                  │ │
│  │    - 状态栏 (进程、截图按钮)                       │ │
│  │                                                  │ │
│  │  SCContentFilter targeting this window ID         │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ┌─────────────────────────────────────────────────┐ │
│  │           Automation Engine (Rust)               │ │
│  │                                                  │ │
│  │  CGEvent       ← 鼠标/键盘模拟                    │ │
│  │  AXUIElement   ← UI 元素树读取                    │ │
│  │  ScreenCaptureKit ← 窗口级截图                    │ │
│  │  PTY           ← CLI 进程管理                     │ │
│  │  NSWorkspace   ← .app 启动管理                   │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ┌─────────────────────────────────────────────────┐ │
│  │         Server Layer                            │ │
│  │  MCP Server (stdio) ← Claude Code / OpenCode    │ │
│  │  HTTP Server (:5801) ← curl / Python / 脚本     │ │
│  └─────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
         │                           │
         ▼                           ▼
  ┌─────────────┐           ┌─────────────────┐
  │  任意 CLI    │           │  任意 .app       │
  │  进程        │           │  (Tauri/SwiftUI) │
  └─────────────┘           └─────────────────┘
```

**设计原则**：
1. **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成
2. **窗口级截图**：ScreenCaptureKit 只截取沙箱窗口，不需要窗口在前台
3. **双协议**：MCP (Agent CLI 原生) + HTTP (通用调用)
4. **可复用**：不限于特定项目，任何 macOS 应用/CLI 都能用

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
│   │       └── sandbox/          # 沙箱窗口管理
│   │           └── mod.rs
│   └── sandbox-cli/              # 🖥️ CLI (binary)
│       └── src/main.rs
├── sandbox-web/                  # 🌐 沙箱窗口前端 (xterm.js + React)
│   └── src/
├── src-tauri/                    # 🖥️ macOS 宿主应用 (Tauri)
│   └── src/main.rs
├── docs/
│   ├── design/                   # 设计文档
│   └── task/                     # 任务管理 (README.md + phase-*.md + task_records.json)
├── tests/
│   └── fixtures/                 # 集成测试场景 (YAML/JSON)
└── config.example.json
```

## 四、核心接口

### MCP Tools (Agent 调用)

```yaml
# 窗口管理
list_windows:        # 列出沙箱内所有窗口
find_window:         # 按 app 名/标题查找
focus_window:        # 聚焦指定窗口

# 进程管理
spawn_app:           # 启动 .app (如 Hi Boss.app)
spawn_cli:           # 启动 CLI 进程 (如 hiboss)
kill_process:        # 终止进程
list_processes:      # 列出沙箱内进程

# 输入模拟
click:               # 鼠标点击 (x, y, button)
double_click:        # 双击
type_text:           # 输入文本
press_key:           # 按键 (Return, Tab, etc.)
scroll:              # 滚动
drag:                # 拖拽

# 截图 (核心)
screenshot:          # 截取沙箱窗口 (base64 PNG)
screenshot_window:   # 截取沙箱内指定子窗口
screenshot_region:   # 截取沙箱内指定区域

# UI 检查 (高级)
inspect_ui:          # 读取 AX 树
find_element:        # 按 role/title 查找 UI 元素
```

### HTTP API (`:5801`)

```
GET  /health                     健康检查
GET  /windows                    列出窗口
GET  /processes                  列出进程
POST /app/spawn                  启动 .app
POST /cli/spawn                  启动 CLI
POST /input/click                鼠标点击
POST /input/type                 键盘输入
POST /input/key                  按键
GET  /screenshot                 截取沙箱窗口 (PNG)
GET  /screenshot/:window_id      截取指定窗口
GET  /ui/inspect/:window_id      读取 UI 树
```

### Rust API (sandbox-core)

```rust
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
ProcessManager::spawn_cli("hiboss", &["start".into()])?;

// 沙箱管理
use sandbox_core::sandbox::{Sandbox, SandboxConfig};
let mut sandbox = Sandbox::new(SandboxConfig::default());
sandbox.init()?;
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
[1]创建任务记录(待执行) → [2]创建特性分支 → [3]编写设计文档
   → [4]编写测试(RED) → [5]编码实现(GREEN) → [6]重构优化(REFACTOR)
   → [7]本地检查(fmt+clippy+check+typecheck) → [8]验证测试覆盖率
   → [9]更新任务状态(已完成,必须在push前) → [10]git commit → [11]git push
   → [12]创建 PR → [13]等待 CI 门禁通过
```

### 7.2 命令序列

```bash
# 本地检查
cargo fmt --all -- --check && cargo clippy --all-targets \
  && cargo check --all-targets && cargo test --all \
  && pnpm typecheck && pnpm format:check && pnpm test:unit

# 启动沙箱
cargo run -p sandbox-cli -- serve --port 5801

# Agent 通过 MCP 调用 (Claude Code 配置)
# .claude/settings.json 中添加 MCP server 配置

# Agent 通过 HTTP 调用
curl http://127.0.0.1:5801/screenshot | base64 > screenshot.png
curl -X POST http://127.0.0.1:5801/input/click \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200, "button": "left"}'
```

## 八、安全约束

- ✅ 沙箱窗口是独立 NSWindow，截图范围可控
- ✅ ScreenCaptureKit 按窗口 ID 截图，不截全屏
- ✅ 目标应用不需要任何适配
- ✅ Accessibility 和 Screen Recording 权限需用户手动授权
- ✅ HTTP API 仅监听 `127.0.0.1`，不暴露外部网络

## 目录速查

| 内容 | 路径 |
|------|------|
| 核心库 | `/crates/sandbox-core/src/` |
| CLI 入口 | `/crates/sandbox-cli/src/main.rs` |
| Tauri 宿主 | `/src-tauri/src/main.rs` |
| 沙箱前端 | `/sandbox-web/src/` |
| 设计文档 | `/docs/design/` |
| 任务管理 | `/docs/task/` |
| 本文件 | `/CLAUDE.md` |

---

**版本**：v0.1.0 | **创建**：2026-05-13 | **维护者**：system-test-sandbox 项目
