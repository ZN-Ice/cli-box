# Auto Input Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `--pty` flag and auto-detect input routing based on `InstanceKind` (Cli → PTY write, App → CGEvent).

**Architecture:** CLI queries sandbox `kind` from daemon's `/box/list` endpoint. `InstanceKind::Cli` routes to PTY write, `InstanceKind::App` routes to CGEvent. No daemon changes needed.

**Tech Stack:** Rust (clap, reqwest)

---

### Task 1: Add `resolve_sandbox_kind` helper and refactor `cmd_type_daemon`

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:50-62` (remove `--pty` from TypeText)
- Modify: `crates/cli-box-cli/src/main.rs:237-238` (update call site)
- Modify: `crates/cli-box-cli/src/main.rs:609-625` (refactor cmd_type_daemon)

- [ ] **Step 1: Remove `--pty` from TypeText command definition**

In `crates/cli-box-cli/src/main.rs`, replace lines 50-62:

```rust
    #[command(name = "type")]
    TypeText {
        /// Text to type
        text: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Use PTY write instead of CGEvent (more reliable for CLI processes)
        #[arg(long)]
        pty: bool,
    },
```

With:

```rust
    #[command(name = "type")]
    TypeText {
        /// Text to type
        text: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,
    },
```

- [ ] **Step 2: Update TypeText call site**

Replace line 237-238:

```rust
        Commands::TypeText { text, id, pty } => {
            cmd_type_daemon(&text, &id, pty).await?;
        }
```

With:

```rust
        Commands::TypeText { text, id } => {
            cmd_type_daemon(&text, &id).await?;
        }
```

- [ ] **Step 3: Add `resolve_sandbox_kind` helper function**

Add this function before `cmd_type_daemon` (around line 608):

```rust
/// Query the daemon to determine a sandbox's InstanceKind.
async fn resolve_sandbox_kind(id: &str) -> anyhow::Result<cli_box_core::instance::InstanceKind> {
    let sandboxes = client::daemon_list_sandboxes().await?;
    sandboxes
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.kind.clone())
        .ok_or_else(|| anyhow::anyhow!("Sandbox '{}' not found", id))
}
```

- [ ] **Step 4: Refactor `cmd_type_daemon` to auto-detect**

Replace the entire `cmd_type_daemon` function (lines 609-625):

```rust
/// Type text in a sandbox via the daemon API.
async fn cmd_type_daemon(text: &str, id: &str) -> anyhow::Result<()> {
    let use_pty = matches!(resolve_sandbox_kind(id).await?, cli_box_core::instance::InstanceKind::Cli { .. });
    tracing::info!(
        "[cli] type: text_len={}, id={}, use_pty={}",
        text.len(),
        id,
        use_pty
    );
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

- [ ] **Step 5: Verify TypeScript compiles**

Run: `cargo check -p cli-box-cli`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): auto-detect PTY vs CGEvent for type command"
```

---

### Task 2: Refactor `cmd_key_daemon` to auto-detect

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:64-80` (remove `--pty` from Key)
- Modify: `crates/cli-box-cli/src/main.rs:240-246` (update call site)
- Modify: `crates/cli-box-cli/src/main.rs:627-679` (refactor cmd_key_daemon)

- [ ] **Step 1: Remove `--pty` from Key command definition**

Replace lines 64-80:

```rust
    /// Press a key in a sandbox
    Key {
        /// Key name (e.g., Return, Tab, Escape, space, a-z)
        key: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Modifier keys (e.g., cmd, shift, ctrl, alt)
        #[arg(short, long, num_args = 0..)]
        modifiers: Vec<String>,

        /// Use PTY write instead of CGEvent
        #[arg(long)]
        pty: bool,
    },
```

With:

```rust
    /// Press a key in a sandbox
    Key {
        /// Key name (e.g., Return, Tab, Escape, space, a-z)
        key: String,

        /// Sandbox instance ID
        #[arg(long)]
        id: String,

        /// Modifier keys (e.g., cmd, shift, ctrl, alt)
        #[arg(short, long, num_args = 0..)]
        modifiers: Vec<String>,
    },
```

- [ ] **Step 2: Update Key call site**

Replace lines 240-246:

```rust
        Commands::Key {
            key,
            id,
            modifiers,
            pty,
        } => {
            cmd_key_daemon(&key, &id, &modifiers, pty).await?;
        }
```

With:

```rust
        Commands::Key {
            key,
            id,
            modifiers,
        } => {
            cmd_key_daemon(&key, &id, &modifiers).await?;
        }
```

- [ ] **Step 3: Refactor `cmd_key_daemon` to auto-detect**

Replace the entire `cmd_key_daemon` function (lines 627-679):

```rust
/// Press a key in a sandbox via the daemon API.
async fn cmd_key_daemon(
    key: &str,
    id: &str,
    modifiers: &[String],
) -> anyhow::Result<()> {
    let use_pty = matches!(resolve_sandbox_kind(id).await?, cli_box_core::instance::InstanceKind::Cli { .. });
    tracing::info!(
        "[cli] key: key={}, modifiers={:?}, id={}, use_pty={}",
        key,
        modifiers,
        id,
        use_pty
    );
    if use_pty {
        let data = client::key_to_pty_bytes_with_modifiers(key, modifiers);
        if data.is_empty() {
            let plain = client::key_to_pty_bytes(key);
            if plain.is_empty() {
                anyhow::bail!(
                    "Key '{}' with modifiers {:?} cannot be mapped to PTY bytes.",
                    key, modifiers
                );
            }
            client::daemon_pty_write(id, &plain).await?;
        } else {
            client::daemon_pty_write(id, &data).await?;
        }
        println!(
            "Pressed (PTY): {}{} -> sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    } else {
        client::daemon_key(id, key, modifiers).await?;
        println!(
            "Pressed: {}{} -> sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    }
    Ok(())
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p cli-box-cli`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): auto-detect PTY vs CGEvent for key command"
```

---

### Task 3: Update legacy functions and verify tests

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:892-912` (legacy cmd_type)
- Modify: `crates/cli-box-cli/src/main.rs:914-960` (legacy cmd_key)

- [ ] **Step 1: Update legacy `cmd_type` function**

Replace the legacy `cmd_type` function (lines 892-912):

```rust
/// Type text into a sandbox (legacy).
#[allow(dead_code)]
async fn cmd_type(text: &str, id: &str) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    let use_pty = matches!(resolve_sandbox_kind(id).await?, cli_box_core::instance::InstanceKind::Cli { .. });
    tracing::info!(
        "[cli] type: text_len={}, id={}, use_pty={}",
        text.len(),
        id,
        use_pty
    );

    if use_pty {
        client.pty_write_auto(text).await?;
        println!("Typed (PTY): {:?} → sandbox {}", text, id);
    } else {
        client.type_text(text).await?;
        println!("Typed (CGEvent): {:?} → sandbox {}", text, id);
    }
    Ok(())
}
```

- [ ] **Step 2: Update legacy `cmd_key` function**

Replace the legacy `cmd_key` function signature and body (lines 914-960):

```rust
/// Press a key in a sandbox (legacy).
#[allow(dead_code)]
async fn cmd_key(key: &str, id: &str, modifiers: &[String]) -> anyhow::Result<()> {
    let client = client::SandboxClient::from_instance_id(id)?;
    let use_pty = matches!(resolve_sandbox_kind(id).await?, cli_box_core::instance::InstanceKind::Cli { .. });
    tracing::info!(
        "[cli] key: key={}, modifiers={:?}, id={}, use_pty={}",
        key,
        modifiers,
        id,
        use_pty
    );

    if use_pty {
        let data = client::key_to_pty_bytes_with_modifiers(key, modifiers);
        if data.is_empty() {
            let plain = client::key_to_pty_bytes(key);
            if plain.is_empty() {
                anyhow::bail!(
                    "Key '{}' with modifiers {:?} cannot be mapped to PTY bytes.",
                    key, modifiers
                );
            }
            client.pty_write_auto(&plain).await?;
        } else {
            client.pty_write_auto(&data).await?;
        }
        println!(
            "Pressed (PTY): {} {} → sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    } else {
        client.press_key(key, modifiers).await?;
        println!(
            "Pressed (CGEvent): {} {} → sandbox {}",
            if modifiers.is_empty() {
                String::new()
            } else {
                format!("{:?}+", modifiers)
            },
            key,
            id
        );
    }
    Ok(())
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p cli-box-cli -p cli-box-core`
Expected: All tests pass

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p cli-box-cli -- -D warnings`
Expected: No warnings

- [ ] **Step 5: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "refactor(cli): update legacy type/key functions to use auto-detection"
```
