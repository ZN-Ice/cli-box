# Rebuild Plan: macOS Desktop Automation Sandbox v2

> **Branch**: `feat/rebuild-sandbox-v2` | **Date**: 2026-05-18 | **Status**: Phase 1 complete

## Overview

从 `feat/5-multi-instance` 重建项目，删除 Tauri/React 前端，简化核心库，分三个阶段逐步实现沙箱自动化功能。

## Architecture Decisions

1. **沙箱容器**: 使用原生 macOS Terminal.app (AppleScript)，不使用 Tauri/xterm.js/React
2. **Phase 1-2**: PTY-based CLI 交互（输入/输出通过 portable-pty）
3. **Phase 3**: CGEvent-based macOS 桌面自动化
4. **简化原则**: 删除 recorder/player/scenario/report/diff 高级模块，保留核心自动化能力

## New Project Structure

```
cli-box/
├── Cargo.toml                         # workspace: sandbox-core + sandbox-cli
├── crates/
│   ├── sandbox-core/                  # 6 个模块（从 11 个简化）
│   │   └── src/
│   │       ├── lib.rs                 # automation, capture, instance, process, sandbox, server
│   │       ├── automation/
│   │       │   ├── cg_event.rs        # CGEvent 输入模拟
│   │       │   ├── ax_ui.rs           # AXUIElement UI 检查
│   │       │   └── keycodes.rs        # CGKeyCode 映射
│   │       ├── capture/mod.rs         # ScreenCaptureKit 截图
│   │       ├── process/mod.rs         # PTY + .app 进程管理
│   │       ├── sandbox/mod.rs         # 沙箱窗口状态机
│   │       ├── instance/mod.rs        # 文件系统注册中心
│   │       └── server/mod.rs          # Axum HTTP API (17 routes)
│   └── sandbox-cli/                   # 全新 CLI
│       └── src/
│           ├── main.rs                # CLI 入口 (clap)
│           └── client.rs              # HTTP 客户端 (Phase 2)
└── docs/
    └── design/
        └── rebuild-plan.md            # This document
```

## Phase 1 (Complete): Claude in Sandbox + Screenshot

**目标**: 在 Terminal.app 窗口中启动 CLI 命令，截取窗口截图，列出所有窗口，关闭沙箱。

### 已实现的命令

```bash
cli-box start <command> [args...]      # 在 Terminal.app 中启动命令
cli-box screenshot [-o output] [--window-id ID]  # 截取沙箱窗口截图
cli-box windows                         # 列出所有可见窗口
cli-box shutdown                        # 关闭 Terminal 窗口
```

### 实现细节

- **start**: 使用 `osascript` 执行 AppleScript，打开 Terminal.app 并运行目标命令
- **screenshot**: 使用 `ScreenCapture::capture_window()` (ScreenCaptureKit)，自动发现 Terminal 窗口或手动指定 window_id
- **windows**: 使用 `ScreenCapture::list_windows()` 列出所有 SCWindow
- **shutdown**: 使用 AppleScript 关闭 Terminal 第一个窗口

### 关键文件

- `crates/sandbox-cli/src/main.rs` — CLI 实现
- `crates/sandbox-core/src/capture/mod.rs` — 截图引擎
- `crates/sandbox-core/src/server/mod.rs` — HTTP API（简化版 17 routes）

## Phase 2 (Planned): Input Operations

**目标**: 通过 PTY 向沙箱 Claude 发送文本和读取输出。

### 计划命令

```bash
sandbox serve [--port PORT] [--command CMD] [args...]  # 启动 HTTP 服务器 + PTY
sandbox type [--port PORT] <text>                       # 发送文本到 PTY
sandbox enter [--port PORT]                              # 发送 Enter 到 PTY
sandbox read [--port PORT]                               # 读取 PTY 输出
```

### 实现要点

- `serve` 命令启动 Axum HTTP 服务器 + 可选 PTY 会话
- `type/enter/read` 通过 HTTP 客户端与服务器通信
- PTY 通过 `ProcessManager::spawn_cli` 管理
- 需要创建 `crates/sandbox-cli/src/client.rs` HTTP 客户端

### 交互流程

```
Terminal 1: sandbox serve --command claude
Terminal 2: cli-box screenshot -o before.png
Terminal 2: sandbox type --text "Write a hello world in Rust"
Terminal 2: sandbox enter
Terminal 2: sleep 5
Terminal 2: sandbox read
Terminal 2: cli-box screenshot -o after.png
```

## Phase 3 (Planned): macOS Desktop Automation

**目标**: 启动 .app 应用，截取任意窗口截图，通过 CGEvent 模拟鼠标键盘操作。

### 计划命令

```bash
sandbox spawn-app <path>                               # 启动 macOS .app
sandbox click <x> <y> [--button left|right|middle]      # 鼠标点击
sandbox type-text <text> [--target-pid PID]             # CGEvent 文本输入
sandbox press-key <key> [--modifiers cmd,shift...]       # 按键
cli-box screenshot-window <window_id> [-o output]        # 截取指定窗口
cli-box screenshot-region <x> <y> <w> <h> [-o output]   # 截取屏幕区域
```

### 实现要点

- `spawn-app` 使用 `ProcessManager::spawn_app_with_window()`
- 输入模拟使用 `InputSimulator` (CGEvent)，支持 `target_pid` 定向发送
- 截图使用 `ScreenCapture::capture_window()` / `capture_region()`

### 使用示例

```bash
sandbox spawn-app /System/Applications/Calculator.app
cli-box windows | grep -i calculator
cli-box screenshot-window <ID> -o calc.png
sandbox click 300 400
sandbox type-text "123+456"
sandbox press-key Return
cli-box screenshot-region 0 0 500 500 -o top_left.png
```

## Server HTTP API (17 routes)

```
GET  /health                    # 健康检查
GET  /sandbox/info              # 沙箱信息
POST /shutdown                  # 关闭服务器
GET  /windows                   # 列出所有窗口
GET  /processes                 # 列出进程
POST /app/spawn                 # 启动 macOS .app
POST /cli/spawn                 # 在 PTY 中启动 CLI
POST /process/kill              # 终止进程
POST /input/click               # 鼠标点击
POST /input/type                # 文本输入
POST /input/key                 # 按键
POST /input/scroll              # 滚动
POST /input/drag                # 拖拽
GET  /screenshot                # 截取沙箱窗口
GET  /screenshot/region         # 截取区域
POST /pty/write                 # PTY 写入
GET  /pty/output/:pid           # PTY 读取
GET  /ui/inspect/:window_id     # UI 树检查
POST /ui/find                   # 查找 UI 元素
GET  /ui/value                  # 获取元素值
```

## Removed Modules

| Module | Reason |
|--------|--------|
| `diff.rs` | 图片差异对比 — Phase 2+ 可能重新添加 |
| `player.rs` | 操作回放引擎 — 复杂功能，暂不需要 |
| `recorder.rs` | 操作录制 — 复杂功能，暂不需要 |
| `report.rs` | 测试报告生成 — 依赖 scenario |
| `scenario.rs` | YAML 场景运行器 — 依赖 player/recorder |

## Removed Directories

| Directory | Reason |
|-----------|--------|
| `sandbox-web/` | React + Vite + xterm.js 前端 |
| `src-tauri/` | Tauri v2 桌面应用 |
| `crates/sandbox-cli/` (旧的) | 复杂的 22 子命令 CLI |

## Verification

### Pre-Work
- [x] `cargo check -p sandbox-core` passes
- [x] `cargo test -p sandbox-core` passes (94 tests)
- [x] `cargo fmt --all -- --check` passes
- [x] `cargo clippy --all-targets` passes

### Phase 1
- [x] `cargo build -p sandbox-cli` succeeds
- [ ] `cli-box start claude` opens Terminal with Claude Code (requires macOS)
- [ ] `cli-box screenshot -o test.png` saves a valid PNG (requires macOS + permissions)
- [ ] `cli-box windows` lists system windows (requires macOS + permissions)
- [ ] `cli-box shutdown` closes the Terminal (requires macOS)

### Phase 2 (to be verified)
- [ ] `sandbox serve --command claude` starts HTTP server + PTY
- [ ] `sandbox type --text "hello"` sends text to PTY
- [ ] `sandbox enter` sends newline to PTY
- [ ] `sandbox read` returns PTY output

### Phase 3 (to be verified)
- [ ] `sandbox spawn-app /System/Applications/Calculator.app` launches Calculator
- [ ] `cli-box screenshot-window <ID>` captures Calculator
- [ ] `sandbox click 300 400` clicks in Calculator
- [ ] `sandbox type-text "123+456"` types via CGEvent
- [ ] `sandbox press-key Return` presses Enter
- [ ] `cli-box screenshot-region 0 0 500 500` captures desktop region
