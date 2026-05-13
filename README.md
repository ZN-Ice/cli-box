# system-test-sandbox

macOS 桌面自动化沙箱 — 让 Agent CLI 工具模拟人类操作任意 macOS 应用和 CLI，并获取截图反馈。

## 特性

- **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成
- **窗口级截图**：ScreenCaptureKit 只截取沙箱窗口，不需要窗口在前台
- **双协议**：MCP (Agent CLI 原生) + HTTP (通用调用)
- **可复用**：不限于特定项目，任何 macOS 应用/CLI 都能用

## 架构

```
Agent CLI (Claude Code / OpenCode)
       │ MCP / HTTP
       ▼
┌──────────────────────────┐
│   Sandbox Host (Tauri)   │
│                          │
│  ┌────────────────────┐  │
│  │  Sandbox Window    │  │
│  │  ┌──────┐ ┌──────┐│  │
│  │  │ CLI  │ │ App  ││  │
│  │  │(PTY) │ │(嵌入)││  │
│  │  └──────┘ └──────┘│  │
│  └────────────────────┘  │
│                          │
│  CGEvent · AXUIElement   │
│  ScreenCaptureKit        │
│  PTY · NSWorkspace       │
└──────────────────────────┘
```

## 快速开始

### 安装

```bash
# 克隆项目
git clone https://github.com/your-org/system-test-sandbox.git
cd system-test-sandbox

# 构建
cargo build --release

# 前端依赖
cd sandbox-web && pnpm install && cd ..
```

### 启动沙箱

```bash
# CLI 模式
cargo run -p sandbox-cli -- serve --port 5801

# Tauri 桌面模式
cd sandbox-web && pnpm dev
cargo run -p system-test-sandbox
```

### Agent 调用示例

**通过 HTTP API：**

```bash
# 截图
curl http://127.0.0.1:5801/screenshot -o sandbox.png

# 启动 CLI
curl -X POST http://127.0.0.1:5801/cli/spawn \
  -H "Content-Type: application/json" \
  -d '{"command": "hiboss", "args": ["start"]}'

# 鼠标点击
curl -X POST http://127.0.0.1:5801/input/click \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200}'

# 键盘输入
curl -X POST http://127.0.0.1:5801/input/type \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello World"}'

# 列出窗口
curl http://127.0.0.1:5801/windows | jq

# 读取 UI 树
curl http://127.0.0.1:5801/ui/inspect/12345 | jq
```

**通过 MCP（Claude Code）：**

在 `.claude/settings.json` 中配置：

```json
{
  "mcpServers": {
    "mac-sandbox": {
      "command": "sandbox",
      "args": ["serve", "--mcp"]
    }
  }
}
```

然后 Agent 可以直接调用：

```
screenshot() → 获取沙箱截图
click(100, 200) → 点击指定坐标
type_text("hello") → 输入文本
spawn_cli("hiboss start") → 启动 CLI
```

## macOS 权限

首次使用需要在 **系统设置 → 隐私与安全** 中授权：

1. **辅助功能** (Accessibility) → 输入模拟 + UI 检查
2. **屏幕录制** (Screen Recording) → 窗口截图

## 技术栈

- Rust + Tauri 2 (桌面框架)
- React 18 + TypeScript + Vite + TailwindCSS (前端)
- xterm.js (终端模拟)
- CoreGraphics (CGEvent 输入模拟)
- ApplicationServices (AXUIElement UI 检查)
- ScreenCaptureKit (窗口级截图)

## License

Apache 2.0
