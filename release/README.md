# System Test Sandbox — Release v${VERSION}

macOS 桌面自动化沙箱。通过 CLI 启动 Tauri 沙箱窗口，内置 xterm.js 终端运行命令行工具（如 Claude Code），支持截图和输入模拟。

## 文件说明

```
release/
├── sandbox                     # CLI 工具（命令行入口）
├── System Test Sandbox.app/    # Tauri 沙箱 macOS 应用
└── README.md                   # 本文件
```

## 一、前置条件

| 依赖 | 版本要求 |
|------|---------|
| macOS | 14.0+ (Sonoma) |
| 芯片 | Apple Silicon (M1–M4)，Intel 也支持 |

### 必须授予的权限

> **没有这两个权限，sandbox 无法工作。**

1. **辅助功能 (Accessibility)**：用于 CGEvent 输入模拟 + AXUIElement UI 读取
2. **屏幕录制 (Screen Recording)**：用于 ScreenCaptureKit 截图

授予方式：`系统设置 → 隐私与安全性 → 辅助功能 / 屏幕录制`。

将 `sandbox` 和 `System Test Sandbox.app` 添加进去并勾选。

## 二、使用方法

### 启动沙箱

```bash
# 在沙箱中启动 Claude Code（交互模式）
./sandbox start claude

# 启动交互式 Shell
./sandbox start zsh
./sandbox start bash

# 启动其他 CLI 工具
./sandbox start node
./sandbox start npm -- test
```

> **注意**：命令与参数之间用 `--` 分隔，如 `./sandbox start <command> -- <args>`。

### 输入操作

```bash
# 获取沙箱 ID
./sandbox list

# PTY 输入文本（推荐用于 CLI 沙箱）
./sandbox type --id <id> --pty "你好世界"

# PTY 按键
./sandbox key --id <id> --pty Return       # 回车
./sandbox key --id <id> --pty ctrl+c       # Ctrl+C
./sandbox key --id <id> --pty ctrl+l       # 清屏

# 截图
./sandbox screenshot --id <id> -o screenshot.png
```

### 管理沙箱

```bash
# 查看所有活跃沙箱
./sandbox list

# 查看沙箱详情
./sandbox inspect <id>

# 关闭沙箱
./sandbox close <id>
```

### 示例工作流

```bash
# 1. 启动 Claude Code
./sandbox start claude

# 2. 等待 Claude 启动（约 10 秒）
sleep 10

# 3. 截图查看状态
./sandbox screenshot --id <id> -o screenshot.png

# 4. 通过 PTY 与 Claude 交互
./sandbox type --id <id> --pty "你好"
./sandbox key --id <id> --pty Return
sleep 5
./sandbox screenshot --id <id> -o claude_response.png

# 5. 关闭沙箱
./sandbox close <id>
```

## 三、架构

```
sandbox start claude
       │
       ▼
CLI (sandbox)
       │ spawn System Test Sandbox.app --mode=cli --cmd=claude
       ▼
Tauri 沙箱窗口
  ┌────────────────────────────────────────────┐
  │  终端面板 (xterm.js)    │  Screenshot Preview │
  │  ← Claude 运行在这里     │                     │
  ├────────────────────────────────────────────┤
  │  Control Panel: Screenshot / Spawn / Click  │
  ├────────────────────────────────────────────┤
  │  Status: Server :5801 | Processes: X | ...  │
  └────────────────────────────────────────────┘
       │ HTTP :5801
       ▼
  内嵌 HTTP API (axum)
  - /screenshot, /input/click, /pty/ws/{pid}, ...
```

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: 无法启动沙箱？**
A: 确保 `System Test Sandbox.app` 与 `sandbox` 在同一目录下。

**Q: 沙箱窗口内终端空白？**
A: 等待几秒让应用启动，终端会自动连接 PTY 输出。

**Q: CLI `type` / `key` 命令无效？**
A: CLI 沙箱请始终使用 `--pty` 模式。不带 `--pty` 时使用 CGEvent，对 CLI 进程无效。

---

**版本**: v${VERSION} | **构建时间**: 2026-05-24 20:43
