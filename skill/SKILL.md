---
name: cli-box
description: macOS desktop automation sandbox — run CLI tools and macOS apps in isolated sandbox windows with screenshot feedback and input simulation
---

# cli-box

macOS desktop automation sandbox. Launch isolated sandbox windows from the CLI, run any CLI tool (Claude Code, OpenCode, zsh, etc.) inside them, and automate via screenshot + keyboard/mouse simulation.

## Prerequisites

- macOS 14.0+ (Sonoma), Apple Silicon or Intel
- **Accessibility** permission (System Settings → Privacy & Security → Accessibility)
- **Screen Recording** permission (System Settings → Privacy & Security → Screen Recording)

Both permissions must be granted manually. Add `cli-box` and `CLI Box.app` to both lists.

## Quick Start

```bash
# Start a sandbox running Claude Code
cli-box start claude

# Start a sandbox running zsh
cli-box start zsh

# List all active sandboxes
cli-box list

# Take a screenshot of a sandbox
cli-box screenshot --id <sandbox-id> -o screenshot.png

# Type text into a sandbox (PTY mode for CLI tools)
cli-box type --id <sandbox-id> --pty "hello world"

# Press Enter to send
cli-box key --id <sandbox-id> --pty Return

# Close a sandbox
cli-box close <sandbox-id>
```

## Commands

### Sandbox Management

| Command | Description |
|---------|-------------|
| `cli-box start [command]` | Start sandbox (default: zsh). Supports `claude`, `opencode`, `zsh`, `bash`, or any CLI |
| `cli-box start /path/to/App.app` | Start sandbox with a macOS application |
| `cli-box start claude -- -p "question"` | Start sandbox with arguments |
| `cli-box list` | List all active sandboxes with ID, title, status, port |
| `cli-box close <id>` | Close a sandbox and clean up |
| `cli-box inspect <id>` | Show sandbox details |

### Input Simulation

| Command | Description |
|---------|-------------|
| `cli-box type --id <id> --pty "text"` | Type text via PTY (recommended for CLI tools) |
| `cli-box key --id <id> --pty Return` | Press a key via PTY |
| `cli-box key --id <id> --pty ctrl+c` | Send Ctrl+C |
| `cli-box key --id <id> --pty up` | Arrow keys |
| `cli-box click --id <id> 100 200` | Mouse click at coordinates (CGEvent) |

### Screenshots

| Command | Description |
|---------|-------------|
| `cli-box screenshot --id <id>` | Screenshot to stdout (base64) |
| `cli-box screenshot --id <id> -o file.png` | Screenshot to file |

### MCP Integration

Add to `.claude/settings.json` or `.opencode/config.json`:

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

Then use tools: `start_sandbox`, `screenshot`, `click`, `type_text`, `press_key`, `close_sandbox`, `list_sandboxes`.

## Typical Workflow

```bash
# 1. Start sandbox
cli-box start claude
# → Returns: Sandbox started: abc123

# 2. Wait for tool to initialize
sleep 10

# 3. Screenshot to see current state
cli-box screenshot --id abc123 -o state.png

# 4. Interact
cli-box type --id abc123 --pty "Write a hello world function"
cli-box key --id abc123 --pty Return

# 5. Wait and screenshot again
sleep 15
cli-box screenshot --id abc123 -o result.png

# 6. Clean up
cli-box close abc123
```

## Notes

- CLI tools (claude, opencode, zsh) should always use `--pty` mode for input
- CGEvent mode (no `--pty`) is for GUI app sandboxes only
- Each sandbox gets its own Electron tab and HTTP port
- The daemon auto-starts on first `cli-box start` and manages all sandboxes
