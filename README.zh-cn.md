[English](README.md) | **简体中文**

---

> [!TIP]
> **一行命令。任意 CLI 工具。隔离沙箱。**
>
> ```bash
> cli-box start claude
> ```
> 就这样。Claude Code 在独立沙箱窗口中运行。截图。自动化。关闭。

<div align="center">

# cli-box

**macOS 桌面自动化沙箱 — AI Agent 专用**

[![GitHub Release](https://img.shields.io/github/v/release/ZN-Ice/cli-box?color=369eff&labelColor=black&logo=github&style=flat-square)](https://github.com/ZN-Ice/cli-box/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-white?labelColor=black&style=flat-square)](https://github.com/ZN-Ice/cli-box/blob/main/LICENSE)
[![macOS](https://img.shields.io/badge/platform-macOS%2014%2B-black?style=flat-square&logo=apple)](https://github.com/ZN-Ice/cli-box)
[![GitHub Stars](https://img.shields.io/github/stars/ZN-Ice/cli-box?color=ffcb47&labelColor=black&style=flat-square)](https://github.com/ZN-Ice/cli-box/stargazers)

[![cli-box 预览 — Claude Code 在沙箱中运行](./.github/assets/hero.png)](https://github.com/ZN-Ice/cli-box#cli-box)

</div>

## 安装

### TL;DR

| 你想 | 运行 | 效果 |
| :--- | :--- | :--- |
| **npm** | `npm install -g cli-box-skill` | 自动安装二进制 + skill |
| **Shell** | `bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)` | 下载到 `~/.cli-box/bin/`，安装 skill |
| **手动** | [GitHub Releases](https://github.com/ZN-Ice/cli-box/releases) | 手动下载解压 |

### 给人类用户

把这段命令粘贴到 **Claude Code** 里 — 它会自动下载、安装、配置权限、验证运行：

```
Install cli-box by following the instructions here:
https://raw.githubusercontent.com/ZN-Ice/cli-box/main/docs/guide/installation.md
```

> Agent 会处理 macOS 权限、PATH 配置和快速测试。只需在提示时确认即可。

### 给 LLM Agent

```bash
curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/docs/guide/installation.md
```

按指引逐步执行。

### 添加 PATH

```bash
echo 'export PATH="$HOME/.cli-box/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

## 演示

**启动 Claude Code 沙箱，交互，获取截图：**

```
$ cli-box start claude
Sandbox started: 0cd60ad4

$ cli-box screenshot --id 0cd60ad4 -o state.png
```

[![Claude Code 沙箱 — 信任对话框](./.github/assets/demo-claude-start.png)](https://github.com/ZN-Ice/cli-box#演示)

[![Claude Code 在沙箱中回复](./.github/assets/demo-claude-reply.png)](https://github.com/ZN-Ice/cli-box#演示)

**多 Tab — 同时运行 Claude Code、OpenCode、zsh：**

[![多 Tab 沙箱](./.github/assets/demo-multi-tab.png)](https://github.com/ZN-Ice/cli-box#演示)

**支持任意 CLI 工具：**

[![OpenCode 在沙箱中](./.github/assets/demo-opencode.png)](https://github.com/ZN-Ice/cli-box#演示)

```bash
cli-box start claude    # Claude Code
cli-box start opencode  # OpenCode
cli-box start zsh       # Shell
cli-box start node      # Node.js
```

## 特性

| | 功能 | |
|:---:|:---|:---:|
| 多实例 | 每个 CLI 工具独立沙箱 Tab | |
| 窗口截图 | ScreenCaptureKit 按窗口 ID 截图，无需前台 | |
| PTY 输入 | 直接终端输入，支持中文和所有按键组合 | |
| MCP 集成 | Claude Code / OpenCode 通过 MCP 调用 cli-box | |
| 零侵入 | 目标应用无需适配，OS 层面操作 | |

## 快速参考

```bash
# 沙箱生命周期
cli-box start [command]         # 启动沙箱（默认 zsh）
cli-box list                    # 列出活跃沙箱
cli-box close <id>              # 关闭沙箱

# 截图 + 输入
cli-box screenshot --id <id> -o shot.png
cli-box type --id <id> --pty "你好世界"
cli-box key --id <id> --pty Return
cli-box click --id <id> 100 200

# MCP 配置（添加到 .claude/settings.json）
# { "mcpServers": { "cli-box": { "command": "cli-box", "args": ["mcp-serve"] } } }
```

## macOS 权限

| 权限 | 用途 | 授权位置 |
|:---|:---|:---|
| **辅助功能** | 输入模拟 + UI 检查 | 系统设置 → 隐私与安全性 |
| **屏幕录制** | 窗口截图 | 系统设置 → 隐私与安全性 |

将 `cli-box` 和 `CLI Box.app` 添加到两个列表中。权限需手动授予。

## 技术栈

| 组件 | 技术 |
|:---|:---|
| 核心 | Rust (≥1.88), `cli-box-core` |
| CLI | Rust, `cli-box-cli` 二进制 |
| 桌面 | Electron + React 18 + TypeScript + Vite + xterm.js |
| macOS API | CoreGraphics (CGEvent), ApplicationServices (AXUIElement), ScreenCaptureKit |
| License | Apache 2.0 |

---

[English](README.md) · [GitHub Issues](https://github.com/ZN-Ice/cli-box/issues)
