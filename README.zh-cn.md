[English](README.md) | **简体中文**

---
# cli-box

macOS 桌面自动化沙箱 — 支持多实例管理，通过 CLI 命令启动独立沙箱窗口，在其中运行任意 CLI 或 macOS 应用，模拟人类操作并获取截图反馈。

## 特性

- **多实例管理**：`cli-box-cli start --cli "claude"` 一键启动沙箱，返回唯一 ID
- **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成
- **窗口级截图**：ScreenCaptureKit 按窗口 ID 截图，不需要窗口在前台
- **双协议**：MCP (Agent CLI 原生) + HTTP (通用调用)
- **可复用**：不限于特定项目，任何 macOS 应用/CLI 都能用

## 架构

```
cli-box start claude
    │
    ├─ 确保 daemon 运行 (cli-box-daemon, 端口 15801)
    ├─ POST /sandbox/create → 创建 PTY 沙箱
    ├─ 启动 Electron 应用 (如未运行)
    └─ 写入注册中心 ~/.cli-box/instances/<id>.json

cli-box screenshot --id <id>
    └─ GET http://127.0.0.1:15801/sandbox/<id>/screenshot → PNG

cli-box close <id>
    ├─ POST http://127.0.0.1:15801/sandbox/<id>/close
    └─ 清理注册信息, 终止关联进程
```

## 快速开始

### 安装

```bash
# 克隆项目
git clone https://github.com/ZN-Ice/cli-box.git
cd cli-box

# 构建 daemon + CLI
cargo build --release

# 构建 Electron 应用
cd electron-app && pnpm install && pnpm build && cd ..
```

### 启动沙箱

```bash
# 启动沙箱，运行 Claude Code
cli-box start claude
# → 打开 "CLI Box" 窗口
# → xterm.js 终端中运行 claude
# → 输出: Sandbox started: abc123

# 启动沙箱，运行 macOS 应用
cli-box start /Applications/cc-switch.app
# → 打开沙箱窗口，启动 cc-switch
# → 输出: Sandbox started: def456

# 启动沙箱，运行带参数的 CLI
cli-box start npm -- run test
```

### 管理沙箱

```bash
# 查看所有活跃沙箱
cli-box list
# → ID      TITLE              KIND  STATUS   PORT   CREATED
# → abc123  "claude"           CLI   Running  15801  2026-05-16 10:30
# → def456  "cc-switch"        APP   Running  15802  2026-05-16 10:31

# 查看沙箱详情
cli-box inspect abc123

# 截取沙箱截图
cli-box screenshot --id abc123 -o sandbox.png

# 列出沙箱内进程
cli-box processes --id abc123

# 关闭沙箱
cli-box close abc123
```

### 键盘与鼠标操作

> **CLI 沙箱（如 Claude Code、zsh）请始终使用 `--pty` 模式。** CGEvent 模式将键盘事件发送到 Tauri 窗口进程，而非 PTY 子进程，因此对 CLI 程序无效。

```bash
# ─── 输入文本 ──────────────────────────────────────────

# PTY 直写（推荐，CLI 沙箱专用）
cli-box type --id abc123 --pty "帮我写一个函数"

# CGEvent 模式（仅适用于 GUI 应用沙箱，对 CLI 沙箱无效）
cli-box type --id abc123 "帮我写一个函数"

# ─── 按键 ──────────────────────────────────────────────

# PTY 按键（推荐，CLI 沙箱专用）
cli-box key --id abc123 --pty Return
cli-box key --id abc123 --pty Tab
cli-box key --id abc123 --pty Escape
cli-box key --id abc123 --pty ctrl+c      # 发送 Ctrl+C
cli-box key --id abc123 --pty ctrl+l      # 清屏
cli-box key --id abc123 --pty up          # 上箭头
cli-box key --id abc123 --pty down        # 下箭头
cli-box key --id abc123 --pty left        # 左箭头
cli-box key --id abc123 --pty right       # 右箭头（接受补全）
cli-box key --id abc123 --pty home        # Home
cli-box key --id abc123 --pty end         # End
cli-box key --id abc123 --pty f1          # F1~F12

# PTY 带修饰符按键
cli-box key --id abc123 --pty c -m ctrl   # 等同 ctrl+c
cli-box key --id abc123 --pty up -m shift  # Shift+上（选择模式）
cli-box key --id abc123 --pty tab -m shift # Shift+Tab
cli-box key --id abc123 --pty a -m alt    # Alt+A（ESC 前缀）

# CGEvent 按键（仅适用于 GUI 应用沙箱）
cli-box key --id abc123 Return
cli-box key --id abc123 Return --modifiers cmd

# ─── 鼠标点击（仅 CGEvent，适用于所有沙箱）──────────

cli-box click --id abc123 100 200
cli-box click --id abc123 100 200 --button right
```

#### PTY 支持的按键映射

| 按键 | PTY 字节序列 | 说明 |
|------|-------------|------|
| Return / Enter | `\r` | 提交输入 |
| Tab | `\t` | 自动补全 |
| Escape / Esc | `\x1b` | 取消 |
| Backspace / Delete | `\x7f` | 删除 |
| Up / Down / Left / Right | `\x1b[A/B/C/D` | 方向键 |
| Home / End | `\x1b[H` / `\x1b[F` | 行首/行尾 |
| PageUp / PageDown | `\x1b[5~` / `\x1b[6~` | 翻页 |
| F1~F12 | `\x1bOP`~`\x1b[24~` | 功能键 |
| Ctrl+A ~ Ctrl+Z | `\x01`~`\x1a` | 控制组合键 |
| Ctrl+C | `\x03` | 中断 |
| Ctrl+D | `\x04` | EOF |
| Ctrl+R | `\x12` | 历史搜索 |
| Ctrl+L | `\x0c` | 清屏 |
| Ctrl+W | `\x17` | 删除单词 |

#### 输入路径说明

CLI 沙箱有两种键盘输入路径：

| 路径 | 机制 | 适用场景 | 可靠性 |
|------|------|---------|--------|
| **PTY 直写** (`--pty`) | 写入 PTY master → 子进程 stdin | CLI 沙箱 | 可靠 |
| **CGEvent** (默认) | CGEvent → Tauri 窗口进程 | GUI 应用沙箱 | 依赖窗口焦点 |

CGEvent 模式将键盘事件发送到 Tauri 进程（`target_pid = std::process::id()`），而非 CLI 子进程。WKWebView 不一定能将合成 CGEvent 正确转换为 xterm.js 能处理的 DOM 键盘事件，因此对 CLI 沙箱不可靠。

### 完整场景示例

```bash
# 场景一：在沙箱中与 Claude Code 交互
cli-box start claude
# → 用 cli-box list 获取 ID
cli-box type --id <id> --pty "你是谁？"
cli-box key --id <id> --pty Return
# 等待回复后截图
cli-box screenshot --id <id> -o claude_response.png

# 场景二：在沙箱中执行 Shell 命令
cli-box start zsh
cli-box type --id <id> --pty 'echo "hello world"'
cli-box key --id <id> --pty Return
cli-box screenshot --id <id> -o shell_output.png

# 场景三：使用快捷键操作 Claude Code
cli-box key --id <id> --pty ctrl+c     # 中断当前操作
cli-box key --id <id> --pty up          # 查看上一条命令
cli-box key --id <id> --pty ctrl+l      # 清屏
cli-box key --id <id> --pty ctrl+r      # 搜索历史
```

### Agent 调用示例

**通过 HTTP API（每个沙箱独立端口）：**

```bash
# 获取沙箱信息
curl http://127.0.0.1:15801/health

# 截图
curl http://127.0.0.1:15801/screenshot -o sandbox.png

# 鼠标点击
curl -X POST http://127.0.0.1:15801/input/click \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200}'

# 键盘输入
curl -X POST http://127.0.0.1:15801/input/type \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello World"}'

# 列出窗口
curl http://127.0.0.1:15801/windows | jq

# 读取 UI 树
curl http://127.0.0.1:15801/ui/inspect/12345 | jq
```

**通过 MCP（Claude Code）：**

在 `.claude/settings.json` 中配置：

```json
{
  "mcpServers": {
    "mac-sandbox": {
      "command": "cli-box-cli",
      "args": ["mcp-serve"]
    }
  }
}
```

然后 Agent 可以直接调用：

```
start_sandbox(cli="claude") → 启动沙箱，返回 ID
screenshot(sandbox_id="abc123") → 获取沙箱截图
click(100, 200, sandbox_id="abc123") → 点击指定坐标
type_text("hello", sandbox_id="abc123") → 输入文本
close_sandbox("abc123") → 关闭沙箱
```

## macOS 权限

首次使用需要在 **系统设置 → 隐私与安全** 中授权：

1. **辅助功能** (Accessibility) → 输入模拟 + UI 检查
2. **屏幕录制** (Screen Recording) → 窗口截图

## 日志与调试

CLI 和 Tauri 沙箱均使用 `tracing` 输出结构化日志。设置 `RUST_LOG` 环境变量控制日志级别：

```bash
# 查看详细输入管线日志
RUST_LOG=info cli-box type --id <id> --pty "hello"
# → [cli] type: text_len=5, id=abc123, pty=true
# → [pty] write: pid=1001, len=5, preview="hello"
# → [pty] send_input: written and flushed to pid=1001

# 不使用 --pty 时会看到警告
RUST_LOG=info cli-box type --id <id> "hello"
# → [cli] type: using CGEvent path... Consider using --pty for CLI sandboxes.
# → [input] type_text: len=5, target_pid=9999
# → [cg_event] press_key: key=h, target_pid=Some(9999)

# 更详细的 CGEvent 日志
RUST_LOG=trace cli-box key --id <id> "a"

# 查看 Tauri 沙箱进程的日志（在沙箱启动的终端中可见）
RUST_LOG=info ./CLI\ Box.app/Contents/MacOS/cli-box --mode=cli --cmd=claude
```

关键日志前缀：
- `[cli]` — CLI 命令入口，显示参数和路径选择
- `[input]` — HTTP API 输入处理，显示 target_pid
- `[cg_event]` — CGEvent 层，显示按键码和目标 PID
- `[pty]` — PTY 层，显示写入数据和可用 PID 列表
- `[setup]` — Tauri 启动初始化，显示沙箱配置和窗口发现

## 技术栈

| 项目属性 | 规范值 |
|---------|--------|
| 核心库 | Rust (Edition 2021, >=1.88), `cli-box-core` library crate |
| CLI | Rust, `cli-box-cli` binary crate |
| 桌面框架 | Electron (Chromium) |
| 桌面前端 | React 18 + TS + Vite + xterm.js |
| 异步运行时 | tokio |
| macOS API | CoreGraphics (CGEvent), ApplicationServices (AXUIElement), ScreenCaptureKit |
| 包管理 | Cargo Workspace + pnpm |
| 测试 | cargo test (Rust) + vitest (TS) |
| 目标平台 | macOS (Apple Silicon 优先) |
| License | Apache 2.0 |

## 项目结构

```
cli-box/
├── Cargo.toml                    # Workspace 根
├── crates/
│   ├── cli-box-core/             # 自动化核心 (library)
│   │   └── src/
│   │       ├── automation/       # CGEvent + AXUIElement
│   │       ├── capture/          # ScreenCaptureKit 截图
│   │       ├── process/          # PTY + NSWorkspace 进程管理
│   │       ├── daemon/           # HTTP daemon (单实例管理所有沙箱)
│   │       ├── instance/         # 实例注册中心
│   │       └── server/           # HTTP API 服务器
│   └── cli-box-cli/              # CLI 工具
│       └── src/
│           ├── main.rs           # start/list/close + MCP stdio
│           └── client.rs         # HTTP 客户端
├── electron-app/                 # Electron GUI 前端
│   └── src/renderer/
│       ├── main.tsx, api.ts
│       └── components/           # Terminal, AppPanel
└── docs/                         # 设计文档 + 任务管理
```

## License

Apache 2.0
