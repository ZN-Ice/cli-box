# Auto Input Routing Design

## Problem

The `key` and `type` CLI commands require `--pty` flag for CLI/TUI apps (like Claude Code, zsh). Without it, they default to CGEvent mode, which doesn't reliably reach PTY-based processes. Users must remember to add `--pty` for every CLI/TUI sandbox interaction.

## Root Cause

The input routing decision is entirely on the CLI side. The daemon already knows the sandbox type via `InstanceKind` (`Cli` or `App`), but the CLI doesn't use this information to auto-select the input method.

## Solution

Auto-detect the sandbox type by querying the daemon's `/box/list` endpoint, and route input accordingly:
- `InstanceKind::Cli` → PTY write (direct stdin)
- `InstanceKind::App` → CGEvent (macOS-level key events)

Remove the `--pty` flag entirely — the routing is deterministic based on sandbox type, no override needed.

## Design

### File: `crates/cli-box-cli/src/main.rs`

#### 1. Add helper function to query sandbox kind

```rust
async fn resolve_sandbox_kind(id: &str) -> anyhow::Result<cli_box_core::instance::InstanceKind> {
    let sandboxes = client::daemon_list_sandboxes().await?;
    sandboxes
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.kind.clone())
        .ok_or_else(|| anyhow::anyhow!("Sandbox '{}' not found", id))
}
```

#### 2. Modify `cmd_type_daemon`

Remove `pty: bool` parameter. Query sandbox kind and route automatically:

```rust
async fn cmd_type_daemon(text: &str, id: &str) -> anyhow::Result<()> {
    let use_pty = matches!(resolve_sandbox_kind(id).await?, InstanceKind::Cli { .. });
    if use_pty {
        client::daemon_pty_write(id, text).await?;
        println!("Typed (PTY): {:?} -> sandbox {}", text, id);
    } else {
        client::daemon_type(id, text).await?;
        println!("Typed: {:?} -> sandbox {}", text, id);
    }
    Ok(())
}
```

#### 3. Modify `cmd_key_daemon`

Same pattern — remove `pty: bool`, auto-detect via `resolve_sandbox_kind`.

#### 4. Remove `--pty` CLI argument

Remove the `--pty` flag from both `TypeText` and `Key` command definitions.

### File: `crates/cli-box-cli/src/client.rs`

No changes needed. `daemon_list_sandboxes()` already returns `Vec<DaemonSandbox>` with `kind: InstanceKind`.

### File: `crates/cli-box-core/src/daemon/mod.rs`

No changes needed. `/box/list` already returns `ManagedSandbox` with `kind` field.

## Behavior Matrix

| Sandbox Kind | Input Method |
|-------------|-------------|
| `Cli` | PTY write (auto) |
| `App` | CGEvent (auto) |

## Testing

- `cli-box type --id <cli-sandbox> "hello"` → PTY write (auto-detected)
- `cli-box key --id <cli-sandbox> return` → PTY write (auto-detected)
- `cli-box type --id <app-sandbox> "hello"` → CGEvent (auto-detected)
- `cli-box key --id <app-sandbox> return` → CGEvent (auto-detected)
