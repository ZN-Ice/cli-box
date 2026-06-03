# CLI Box — Release v${VERSION}

macOS 桌面自动化沙箱。通过 CLI 启动 Electron 沙箱窗口，内置 xterm.js 终端运行命令行工具（如 Claude Code），支持截图和输入模拟。

## 文件说明

```
release/
├── cli-box                     # CLI 工具（命令行入口）
├── cli-box-daemon              # 守护进程（CLI 自动管理）
├── CLI Box.app/    # Electron 沙箱 macOS 应用
└── README.md                   # 本文件
```

## 一、前置条件

| 依赖 | 版本要求 |
|------|---------|
| macOS | 14.0+ (Sonoma) |
| 芯片 | Apple Silicon (M1–M4)，Intel 也支持 |

### 必须授予的权限

> **没有这两个权限，cli-box 无法工作。**

1. **辅助功能 (Accessibility)**：用于 CGEvent 输入模拟 + AXUIElement UI 读取
2. **屏幕录制 (Screen Recording)**：用于 ScreenCaptureKit 截图

授予方式：\`系统设置 → 隐私与安全性 → 辅助功能 / 屏幕录制\`。

将 \`cli-box\` 和 \`CLI Box.app\` 添加进去并勾选。

## 二、使用方法

### 启动沙箱

\`\`\`bash
# 在沙箱中启动 Claude Code（交互模式）
./cli-box start claude

# 非交互式：直接向 Claude 提问（约 30 秒响应）
./cli-box start claude -- -p "你的问题"

# 启动交互式 Shell
./cli-box start zsh
./cli-box start bash

# 启动其他 CLI 工具
./cli-box start node
./cli-box start npm -- test
\`\`\`

> **注意**：命令与参数之间用 \`--\` 分隔，如 \`./cli-box start <command> -- <args>\`。

### 截图

\`\`\`bash
# 截取指定沙箱窗口
./cli-box screenshot --id <cli-box-id> -o screenshot.png
\`\`\`

### 其他命令

\`\`\`bash
# 列出所有沙箱
./cli-box list

# 查看沙箱详情
./cli-box inspect <cli-box-id>

# 关闭沙箱
./cli-box close <cli-box-id>
\`\`\`

### 示例工作流

\`\`\`bash
# 1. 启动 Claude Code（自动打开 Electron 窗口）
./cli-box start claude

# 2. 等待 Claude 启动（约 10 秒）
sleep 10

# 3. 截图查看状态
./cli-box screenshot --id <ID> -o screenshot.png

# 4. 启动另一个沙箱（自动创建新 Tab）
./cli-box start zsh

# 5. 列出所有沙箱
./cli-box list

# 6. 关闭指定沙箱
./cli-box close <ID>
\`\`\`

## 三、架构

\`\`\`
cli-box start claude
       │
       ▼
CLI (cli-box)
       │ 1. 启动 cli-box-daemon（如未运行）
       │ 2. 通过 HTTP 创建沙箱
       │ 3. 启动 Electron 窗口（如未运行）
       ▼
cli-box-daemon (HTTP :15801)
  - 管理 PTY 进程
  - 提供截图/输入 API
  - WebSocket PTY 终端
       │
       ▼
Electron 窗口 (Chromium)
  ┌────────────────────────────────────┐
  │  Tab: claude   Tab: zsh   Tab: ... │
  ├────────────────────────────────────┤
  │  xterm.js 终端                      │
  │  ← PTY WebSocket 连接              │
  │  标准 term.write() 渲染             │
  └────────────────────────────────────┘
\`\`\`

## 四、常见问题

**Q: 截图全黑？**
A: 检查「屏幕录制」权限是否已授予。

**Q: 点击/输入无效？**
A: 检查「辅助功能」权限是否已授予。

**Q: 无法启动沙箱？**
A: 确保 \`CLI Box.app\` 与 \`cli-box\` 在同一目录下。

**Q: 沙箱窗口内终端空白？**
A: 等待几秒让 CLI 工具启动，终端会自动连接 PTY 输出。

---

**版本**: v${VERSION} | **构建时间**: 2026-06-03 20:23
