# system-test-sandbox

macOS 桌面自动化沙箱 — 支持多实例管理，通过 CLI 命令启动独立沙箱窗口，在其中运行任意 CLI 或 macOS 应用，模拟人类操作并获取截图反馈。

## 特性

- **多实例管理**：`sandbox-cli start --cli "claude"` 一键启动沙箱，返回唯一 ID
- **零侵入**：目标应用不需要任何适配，所有操作在 OS 层面完成
- **窗口级截图**：ScreenCaptureKit 按窗口 ID 截图，不需要窗口在前台
- **双协议**：MCP (Agent CLI 原生) + HTTP (通用调用)
- **可复用**：不限于特定项目，任何 macOS 应用/CLI 都能用

## 架构

```
sandbox-cli start --cli "claude"
    │
    ├─ 生成沙箱 ID, 分配端口
    ├─ 启动 Tauri 窗口进程 (含内嵌 HTTP API)
    ├─ 在 xterm.js 终端中运行 claude (PTY)
    └─ 写入注册中心 ~/.sandbox/instances/<id>.json

sandbox-cli screenshot <id>
    ├─ 读取注册中心, 获取端口
    └─ GET http://127.0.0.1:<port>/screenshot → PNG

sandbox-cli close <id>
    ├─ 读取注册中心
    ├─ POST http://127.0.0.1:<port>/shutdown
    └─ 清理注册信息, 终止关联进程
```

## 快速开始

### 安装

```bash
# 克隆项目
git clone https://github.com/your-org/system-test-sandbox.git
cd system-test-sandbox

# 构建 Tauri 应用 + CLI
cd sandbox-web && pnpm install && pnpm build && cd ..
cargo build --release
```

### 启动沙箱

```bash
# 启动沙箱，运行 Claude Code
sandbox-cli start --cli "claude"
# → 打开 "System Test Sandbox" 窗口
# → xterm.js 终端中运行 claude
# → 输出: Sandbox started: abc123

# 启动沙箱，运行 macOS 应用
sandbox-cli start --app "/Applications/cc-switch.app"
# → 打开沙箱窗口，启动 cc-switch
# → 输出: Sandbox started: def456

# 启动沙箱，运行带参数的 CLI
sandbox-cli start --cli "npm" --args "run" "test"
```

### 管理沙箱

```bash
# 查看所有活跃沙箱
sandbox-cli list
# → ID      TITLE              KIND  STATUS   PORT   CREATED
# → abc123  "claude"           CLI   Running  15801  2026-05-16 10:30
# → def456  "cc-switch"        APP   Running  15802  2026-05-16 10:31

# 截取沙箱截图
sandbox-cli screenshot abc123 -o sandbox.png

# 在沙箱内模拟操作
sandbox-cli click abc123 100 200          # 鼠标点击
sandbox-cli type abc123 "帮我写一个函数"    # 输入文本
sandbox-cli key abc123 Return --modifiers cmd  # 按键

# 关闭沙箱
sandbox-cli close abc123
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
      "command": "sandbox-cli",
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

## 技术栈

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

## 项目结构

```
system-test-sandbox/
├── Cargo.toml                    # Workspace 根
├── crates/
│   ├── sandbox-core/             # 自动化核心 (library)
│   │   └── src/
│   │       ├── automation/       # CGEvent + AXUIElement
│   │       ├── capture/          # ScreenCaptureKit 截图
│   │       ├── process/          # PTY + NSWorkspace 进程管理
│   │       ├── sandbox/          # 沙箱窗口管理 (多实例)
│   │       ├── instance/         # 实例注册中心
│   │       └── server/           # HTTP API 服务器
│   └── sandbox-cli/              # CLI 工具
│       └── src/
│           ├── main.rs           # start/list/close + 子命令
│           ├── client.rs         # HTTP 客户端
│           └── mcp_server.rs     # MCP 服务器
├── sandbox-web/                  # 沙箱窗口前端
│   └── src/
│       ├── main.tsx, api.ts
│       └── components/
├── src-tauri/                    # macOS 宿主应用 (Tauri)
└── docs/                         # 设计文档 + 任务管理
```

## License

Apache 2.0
