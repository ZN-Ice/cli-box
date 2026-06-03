# Terminal-First Approach Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify the sandbox to a pure terminal-first approach — spawn zsh by default, let users type `claude`/`opencode`/etc. themselves, and rely on the existing PTY+xterm.js infrastructure for rendering and screenshots.

**Architecture:** The existing codebase already has 90% of what's needed. The PTY layer (`portable_pty`), xterm.js frontend, HTTP PTY endpoints, and ScreenCaptureKit screenshots are all working. The changes are minimal: (1) default to zsh when no command is given, (2) add `--shell` convenience flag, (3) improve terminal resize propagation, (4) reduce PTY polling interval for snappier feel.

**Tech Stack:** Rust (sandbox-core, sandbox-cli), Tauri 2.x, React 18 + xterm.js + FitAddon, portable_pty

---

## Feasibility Analysis

**Why this works with minimal changes:**

| Capability | Current State | What Changes |
|-----------|--------------|-------------|
| PTY spawn | `spawn_cli("zsh")` works today | Default to zsh when no `--cmd` given |
| Keyboard input | xterm.js → HTTP `/pty/write` → PTY writer | No change needed |
| Terminal rendering | xterm.js polls `/pty/output/:pid` every 100ms | Reduce to 50ms for snappier feel |
| Screenshots | ScreenCaptureKit captures Tauri window | No change — captures xterm.js rendering |
| Resize | FitAddon adjusts frontend, PTY stays 80x24 | Add resize propagation to PTY |
| Claude Code/OpenCode | User types `claude` in zsh prompt | No change — just works |

**What we gain:**
- Simpler mental model: sandbox = terminal window
- Users control what runs (claude, opencode, htop, etc.)
- No need for `--cli "claude"` — just `sandbox start`
- Screenshots naturally capture whatever is on screen

**What we lose (intentionally):**
- Auto-spawn of specific CLI (user can still `sandbox start --cli "claude"`)
- Process lifecycle tied to specific command (now tied to zsh)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-cli/src/main.rs` | Modify | Add `--shell` flag, default to zsh, update help text |
| `src-tauri/src/main.rs` | Modify | Default to zsh mode when no cmd/mode given |
| `sandbox-web/src/components/Terminal.tsx` | Modify | Add resize propagation to PTY, reduce poll interval |
| `sandbox-web/src/api.ts` | Modify | Add `ptyResize` API function |
| `crates/sandbox-core/src/server/mod.rs` | Modify | Add `POST /pty/resize` endpoint |
| `crates/sandbox-core/src/process/mod.rs` | Modify | Add `resize_pty` function |

---

### Task 1: Default to zsh when no command specified

**Files:**
- Modify: `src-tauri/src/main.rs:117-133` (config construction)
- Modify: `src-tauri/src/main.rs:217-244` (auto-spawn logic)

- [ ] **Step 1: Modify config construction to default to zsh**

In `src-tauri/src/main.rs`, change the config construction block (around line 117) so that when no `mode` and no `cmd` are provided, it defaults to `mode=cli, cmd=zsh`:

```rust
// After line 115, before config construction:
let mode = launch_args.mode.clone().or_else(|| {
    if launch_args.cmd.is_some() {
        Some("cli".to_string())
    } else {
        Some("cli".to_string()) // default to cli mode
    }
});
let cmd = launch_args.cmd.clone().or_else(|| {
    if launch_args.cmd.is_none() {
        Some("zsh".to_string()) // default to zsh
    } else {
        None
    }
});

let config = SandboxConfig {
    id: launch_args.sandbox_id.clone(),
    port: launch_args.sandbox_port,
    mode: mode.clone(),
    command: cmd.clone(),
    args: launch_args.args.clone(),
    ..SandboxConfig::default()
};
```

- [ ] **Step 2: Run existing tests to verify no regression**

Run: `cargo test -p cli-box`
Expected: All existing tests pass (the parse tests don't depend on default behavior)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(sandbox): default to zsh shell when no command specified

When no --mode or --cmd is provided, the sandbox now defaults to
spawning a zsh shell. Users can type claude/opencode/etc. directly."
```

---

### Task 2: Add --shell convenience flag to CLI

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs:185-214` (cmd_start function)
- Modify: `crates/sandbox-cli/src/main.rs` (CLI argument definitions)

- [ ] **Step 1: Add --shell flag to CLI args**

Find the CLI argument definition for `start` and add a `--shell` flag. The exact location depends on how args are defined (likely using `clap`). Add:

```rust
/// Start with a shell (zsh) instead of a specific command
#[arg(long, default_value_t = false)]
shell: bool,
```

- [ ] **Step 2: Update cmd_start to handle --shell**

In `cmd_start`, when `--shell` is true, override the command to zsh:

```rust
fn cmd_start(command: &str, args: &[String], shell: bool) -> anyhow::Result<()> {
    let (actual_command, actual_args) = if shell {
        ("zsh".to_string(), vec![])
    } else {
        (command.to_string(), args.to_vec())
    };
    // ... rest of function uses actual_command and actual_args
}
```

- [ ] **Step 3: Update the match arm that calls cmd_start**

Find where `cmd_start` is called and pass the `shell` flag.

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p sandbox-cli`
Expected: Pass

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-cli/src/main.rs
git commit -m "feat(cli): add --shell flag for quick zsh sandbox launch

'sandbox start --shell' is shorthand for 'sandbox start --cli zsh'"
```

---

### Task 3: Add PTY resize propagation

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs` (add resize_pty function)
- Modify: `crates/sandbox-core/src/server/mod.rs` (add /pty/resize endpoint)
- Modify: `sandbox-web/src/api.ts` (add ptyResize function)
- Modify: `sandbox-web/src/components/Terminal.tsx` (call resize on fit)

- [ ] **Step 1: Add resize_pty to ProcessManager**

In `crates/sandbox-core/src/process/mod.rs`, add a function to resize the PTY:

```rust
pub fn resize_pty(pid: u32, cols: u16, rows: u16) -> Result<(), ProcessError> {
    let mut sessions = SESSIONS.lock().map_err(|e| ProcessError::SessionError(e.to_string()))?;
    let session = sessions.get_mut(&pid).ok_or(ProcessError::SessionNotFound(pid))?;
    session
        .pty
        .resize(pty::WindowSize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| ProcessError::IoError(e))?;
    Ok(())
}
```

- [ ] **Step 2: Add POST /pty/resize endpoint**

In `crates/sandbox-core/src/server/mod.rs`, add a new route:

```rust
async fn handle_pty_resize(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pid = body.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let cols = body.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
    let rows = body.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;
    ProcessManager::resize_pty(pid, cols, rows)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true})))
}
```

Register the route in the router: `.route("/pty/resize", post(handle_pty_resize))`

- [ ] **Step 3: Add ptyResize to frontend API**

In `sandbox-web/src/api.ts`, add:

```typescript
export async function ptyResize(
  pid: number,
  cols: number,
  rows: number,
): Promise<void> {
  await fetch(`${baseUrl}/pty/resize`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pid, cols, rows }),
  });
}
```

- [ ] **Step 4: Wire resize in Terminal.tsx**

In `sandbox-web/src/components/Terminal.tsx`, after `fitAddon.fit()`, send the new dimensions to the PTY:

```typescript
const handleResize = () => {
  fitAddon.fit();
  if (activePid && xtermRef.current) {
    const dims = xtermRef.current.rows; // or use fitAddon.proposeDimensions()
    const cols = xtermRef.current.cols;
    const rows = xtermRef.current.rows;
    api.ptyResize(activePid, cols, rows).catch(() => {});
  }
};
```

- [ ] **Step 5: Run tests**

Run: `cargo test --all && cd sandbox-web && pnpm typecheck`
Expected: Pass

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs crates/sandbox-core/src/server/mod.rs sandbox-web/src/api.ts sandbox-web/src/components/Terminal.tsx
git commit -m "feat: propagate terminal resize to PTY backend

When the terminal container resizes, the new dimensions are sent
to the PTY so the shell process knows the correct window size."
```

---

### Task 4: Reduce PTY polling interval for snappier feel

**Files:**
- Modify: `sandbox-web/src/components/Terminal.tsx:115-127` (poll interval)

- [ ] **Step 1: Reduce poll interval from 100ms to 50ms**

In `sandbox-web/src/components/Terminal.tsx`, change the polling interval:

```typescript
// Change line 127 from:
}, 100);
// To:
}, 50);
```

- [ ] **Step 2: Verify no performance issues**

The 50ms interval means ~20 requests/second when idle (returning empty). This is acceptable for a single-instance sandbox. If needed, we can add a "has data" flag later.

- [ ] **Step 3: Commit**

```bash
git add sandbox-web/src/components/Terminal.tsx
git commit -m "perf: reduce PTY polling interval to 50ms for snappier terminal"
```

---

### Task 5: Update help text and docs

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs` (help text / description)
- Modify: `CLAUDE.md` (update CLI examples)

- [ ] **Step 1: Update CLI help text**

Update the `start` command description to mention the default zsh behavior:

```rust
/// Start a sandbox with a shell (default: zsh)
///
/// Without arguments, opens a zsh shell where you can run any command.
/// Use --cli to start a specific command directly.
#[command(name = "start")]
```

- [ ] **Step 2: Update CLAUDE.md examples**

Update the CLI examples section to show the simplified usage:

```bash
# Quick start — opens zsh shell
sandbox start

# Start with specific command
sandbox start --cli "claude"

# Start macOS app
sandbox start --app "/Applications/cc-switch.app"
```

- [ ] **Step 3: Commit**

```bash
git add crates/sandbox-cli/src/main.rs CLAUDE.md
git commit -m "docs: update help text and examples for terminal-first approach"
```

---

### Task 6: Create branch and PR

- [ ] **Step 1: Create feature branch**

```bash
git checkout -b feat/terminal-first-approach
```

- [ ] **Step 2: Push branch**

```bash
git push -u origin feat/terminal-first-approach
```

- [ ] **Step 3: Create PR (do not merge)**

```bash
gh pr create --title "feat: terminal-first sandbox approach" --body "$(cat <<'EOF'
## Summary
- Default to zsh shell when no command specified (`sandbox start` just works)
- Add `--shell` convenience flag as shorthand for `--cli zsh`
- Propagate terminal resize to PTY backend for proper shell rendering
- Reduce PTY polling interval from 100ms to 50ms for snappier feel

## Motivation
Simplify the sandbox to a pure terminal-first approach. Instead of requiring users to specify `--cli "claude"`, they can just run `sandbox start` to get a zsh shell, then type `claude`/`opencode`/whatever themselves. Screenshots and rendering naturally capture the terminal output.

## Test plan
- [ ] `cargo test --all` passes
- [ ] `pnpm typecheck` passes in sandbox-web
- [ ] Manual: `cargo run -p sandbox-cli -- start` opens zsh shell
- [ ] Manual: Typing `claude` in the shell starts Claude Code
- [ ] Manual: Terminal resize propagates correctly
- [ ] Manual: Screenshot captures terminal content

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Verify PR exists**

Run: `gh pr list`
Expected: PR visible in list, status "Open"
