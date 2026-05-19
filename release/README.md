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

授予方式：\`系统设置 → 隐私与安全性 → 辅助功能 / 屏幕录制\`。

将 \`sandbox\` 和 \`System Test Sandbox.app\` 添加进去并勾选。

## 二、使用方法

### 启动沙箱

\`\`\`bash
# 在沙箱中启动 Claude Code（交互模式）
./sandbox start claude

# 非交互式：直接向 Claude 提问（约 30 秒响应）
./sandbox start claude -- -p "你的问题"

# 启动交互式 Shell
./sandbox start zsh
./sandbox start bash

# 启动其他 CLI 工具
./sandbox start node
./sandbox start npm -- test
\`\`\`

> **注意**：命令与参数之间用 \`--\` 分隔，如 \`./sandbox start <command> -- <args>\`。

### 截图

\`\`\`bash
# 自动发现沙箱窗口并截图（保存为 PNG）
./sandbox screenshot -o screenshot.png

# 指定窗口 ID 截图
./sandbox screenshot --window-id 12345 -o screenshot.png
\`\`\`

### 其他命令

\`\`\`bash
# 列出所有可见窗口
./sandbox windows

# 关闭沙箱
./sandbox shutdown
\`\`\`

### 示例工作流

\`\`\`bash
# 1. 启动 Claude Code
./sandbox start claude

# 2. 等待 Claude 启动（约 10 秒）
sleep 10

# 3. 截图查看状态
./sandbox screenshot -o screenshot.png

# 4. 关闭沙箱
./sandbox shutdown
\`\`\`

\`\`\`bash
# 非交互式快速提问
./sandbox start claude -- -p "用 Python 写一个 hello world"
sleep 30
./sandbox screenshot -o claude_response.png
./sandbox shutdown
\`\`\`

## 三、架构

\`\`\`
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
  - /screenshot, /input/click, /pty/write, ...
\`\`\`

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: 无法启动沙箱？**
A: 确保 \`System Test Sandbox.app\` 与 \`sandbox\` 在同一目录下。

**Q: 沙箱窗口内终端空白？**
A: 等待几秒让 Claude 启动，终端会自动连接 PTY 输出。

---

**版本**: v${VERSION} | **构建时间**: __BUILD_DATE__
