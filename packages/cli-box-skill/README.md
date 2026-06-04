# cli-box-skill

macOS desktop automation sandbox for AI agents.

## Install

```bash
npm install -g cli-box-skill
```

## What is cli-box?

A macOS sandbox that lets AI agents (Claude Code, OpenCode, etc.) run CLI tools in isolated windows with screenshot feedback and input simulation.

## Quick start

```bash
cli-box start claude    # Start Claude Code sandbox
cli-box start zsh       # Start zsh sandbox
cli-box list            # List active sandboxes
cli-box screenshot --id <id> -o shot.png  # Screenshot
cli-box close <id>      # Close sandbox
```

## No npm?

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)
```

## Links

- [GitHub](https://github.com/ZN-Ice/cli-box)
- [Full README](https://github.com/ZN-Ice/cli-box#readme)
- [Installation Guide](https://github.com/ZN-Ice/cli-box/blob/main/docs/guide/installation.md)

## License

Apache 2.0
