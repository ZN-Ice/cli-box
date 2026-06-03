> [!TIP]
> **One command. Any CLI tool. Isolated sandbox.**
>
> ```bash
> cli-box start claude
> ```
> That's it. Claude Code runs in its own sandbox window. Screenshot it. Automate it. Close it.

<div align="center">

# cli-box

**macOS desktop automation sandbox for AI agents**

[![GitHub Release](https://img.shields.io/github/v/release/ZN-Ice/cli-box?color=369eff&labelColor=black&logo=github&style=flat-square)](https://github.com/ZN-Ice/cli-box/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-white?labelColor=black&style=flat-square)](https://github.com/ZN-Ice/cli-box/blob/main/LICENSE)
[![macOS](https://img.shields.io/badge/platform-macOS%2014%2B-black?style=flat-square&logo=apple)](https://github.com/ZN-Ice/cli-box)
[![GitHub Stars](https://img.shields.io/github/stars/ZN-Ice/cli-box?color=ffcb47&labelColor=black&style=flat-square)](https://github.com/ZN-Ice/cli-box/stargazers)

[![cli-box preview — Claude Code running in sandbox](./.github/assets/hero.png)](https://github.com/ZN-Ice/cli-box#cli-box)

*Claude Code running inside a cli-box sandbox. Multi-tab, screenshot feedback, zero invasion.*

</div>

## What is this?

You're building AI agents that need to interact with CLI tools. They need to see the screen, type commands, and read results.

cli-box gives them an isolated sandbox window per tool. Screenshot to see. Type to interact. Close when done. No API integration needed — it works at the OS level.

```bash
cli-box start claude    # → sandbox window with Claude Code
cli-box start opencode  # → sandbox window with OpenCode
cli-box start zsh       # → sandbox window with zsh
cli-box list            # → see all active sandboxes
cli-box screenshot --id abc123 -o state.png  # → see what's on screen
cli-box close abc123    # → clean up
```

## Installation

### TL;DR

| You want | Run | What happens |
| :--- | :--- | :--- |
| **Quick install** | `bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)` | Downloads binaries to `~/.cli-box/bin/`, installs skill to `.claude/skills/cli-box/` |
| **npm** | `npx cli-box-skill install` | Same as above, via npm |
| **Manual** | [GitHub Releases](https://github.com/ZN-Ice/cli-box/releases) | Download `cli-box-skill.tar.gz`, extract, copy binaries |

### For Humans

Paste this into your terminal:

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)
```

Then grant macOS permissions when prompted:
1. **System Settings → Privacy & Security → Accessibility** → add `cli-box`
2. **System Settings → Privacy & Security → Screen Recording** → add `cli-box`

### For LLM Agents

Paste this into Claude Code, OpenCode, Cursor, or any agent:

```
Install cli-box by following the instructions here:
https://raw.githubusercontent.com/ZN-Ice/cli-box/main/docs/guide/installation.md
```

The agent will download, install, verify, and configure everything automatically.

### Add to PATH

```bash
echo 'export PATH="$HOME/.cli-box/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

## How it works

```
cli-box start claude
       │
       ▼
cli-box CLI
       │ 1. Start cli-box-daemon (if not running)
       │ 2. Create sandbox via HTTP API
       │ 3. Launch Electron window (if not running)
       ▼
cli-box-daemon (HTTP :15801)
  - Manages PTY processes
  - Screenshot + input APIs
  - WebSocket PTY terminal
       │
       ▼
Electron Window (Chromium)
  ┌────────────────────────────────────┐
  │  Tab: claude   Tab: zsh   Tab: ... │
  ├────────────────────────────────────┤
  │  xterm.js terminal                 │
  │  ← PTY WebSocket connection        │
  │  Standard term.write() rendering   │
  └────────────────────────────────────┘
```

**Zero invasion.** The target app needs no adaptation. All operations happen at the OS level (CGEvent + AXUIElement + ScreenCaptureKit).

## Features

| | Feature | What it does |
| :---: | :--- | :--- |
| 🖥️ | **Multi-instance sandboxes** | Run Claude Code, OpenCode, zsh, any CLI — each in its own sandbox tab |
| 📸 | **Window-level screenshots** | ScreenCaptureKit captures by window ID, no need to be in foreground |
| ⌨️ | **PTY keyboard input** | Direct PTY write for reliable CLI input (supports Chinese, all key combos) |
| 🖱️ | **Mouse simulation** | CGEvent click/drag/scroll for GUI app sandboxes |
| 🔌 | **MCP integration** | Claude Code and OpenCode can call cli-box as an MCP tool |
| 🏗️ | **Single daemon** | One daemon manages all sandboxes, auto-starts on first use |
| ♻️ | **Electron reuse** | Second `cli-box start` reuses existing window, adds a new tab |
| 🎯 | **Zero invasion** | Target apps need no adaptation — works at OS level |

## CLI Commands

```bash
# Sandbox lifecycle
cli-box start [command]       # Start sandbox (default: zsh)
cli-box start claude          # Start Claude Code sandbox
cli-box start opencode        # Start OpenCode sandbox
cli-box start /path/App.app   # Start macOS app sandbox
cli-box list                  # List active sandboxes
cli-box close <id>            # Close sandbox
cli-box inspect <id>          # Show sandbox details

# Screenshot
cli-box screenshot --id <id>              # Screenshot to stdout (base64)
cli-box screenshot --id <id> -o shot.png  # Screenshot to file

# Input (PTY mode — for CLI tools)
cli-box type --id <id> --pty "text"       # Type text
cli-box key --id <id> --pty Return        # Press key
cli-box key --id <id> --pty ctrl+c        # Ctrl+C
cli-box key --id <id> --pty up            # Arrow keys

# Input (CGEvent mode — for GUI apps)
cli-box click --id <id> 100 200           # Mouse click
cli-box type --id <id> "text"             # Type via CGEvent
```

## MCP Integration

Add to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "cli-box": {
      "command": "cli-box",
      "args": ["mcp-serve"]
    }
  }
}
```

Available MCP tools: `start_sandbox`, `screenshot`, `click`, `type_text`, `press_key`, `close_sandbox`, `list_sandboxes`.

## macOS Permissions

| Permission | Purpose | Grant in |
|:---|:---|:---|
| **Accessibility** | CGEvent input simulation + AXUIElement UI inspection | System Settings → Privacy & Security → Accessibility |
| **Screen Recording** | ScreenCaptureKit screenshots | System Settings → Privacy & Security → Screen Recording |

Both permissions must be granted manually. Add `cli-box` and `CLI Box.app` to both lists.

## Tech Stack

| Component | Technology |
|:---|:---|
| Core library | Rust (Edition 2021, ≥1.88), `cli-box-core` |
| CLI | Rust, `cli-box-cli` binary |
| Desktop framework | Electron (Chromium) |
| Desktop frontend | React 18 + TypeScript + Vite + xterm.js |
| macOS APIs | CoreGraphics (CGEvent), ApplicationServices (AXUIElement), ScreenCaptureKit |
| Package management | Cargo Workspace + pnpm |
| Testing | cargo test (Rust) + vitest (TypeScript) |
| Target platform | macOS 14+ (Apple Silicon preferred) |
| License | Apache 2.0 |

## License

Apache 2.0

---

[中文文档](README.zh-cn.md) · [GitHub Issues](https://github.com/ZN-Ice/cli-box/issues)
