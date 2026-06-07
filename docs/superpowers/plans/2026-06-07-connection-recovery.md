# Connection Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `cli-box start` hanging when Electron renderer fails to connect to daemon WebSocket.

**Architecture:** CLI-side stale Electron detection and respawn + Electron-side daemon port change detection on reconnect.

**Tech Stack:** Rust (CLI), TypeScript/React (Electron renderer)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/cli-box-cli/src/main.rs` | Modify | Add `kill_stale_electron()` helper, modify `cmd_start_daemon()` retry logic |
| `electron-app/src/renderer/main.tsx` | Modify | Add port change detection in WebSocket `onclose` handler |

---

### Task 1: Add `kill_stale_electron()` Helper Function

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs`

- [ ] **Step 1: Write the `kill_stale_electron()` function**

Add this function right after the existing `find_running_electron()` function (after line 1741):

```rust
/// Kill a stale Electron process that is alive but not responding.
///
/// Reads `~/.cli-box/electron.json` to get the PID, kills the process,
/// and cleans up the file. Returns `true` if a stale process was found and killed.
fn kill_stale_electron() -> bool {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(home)
        .join(".cli-box")
        .join("electron.json");

    if !path.exists() {
        return false;
    }

    let json = match std::fs::read_to_string(&path) {
        Ok(j) => j,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return false;
        }
    };

    let info: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return false;
        }
    };

    let pid = match info["pid"].as_u64() {
        Some(p) => p as i32,
        None => {
            let _ = std::fs::remove_file(&path);
            return false;
        }
    };

    // Check if process is alive
    let alive = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if alive {
        tracing::info!("[start] Killing stale Electron process (pid={pid})");
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
        // Give it a moment to exit
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let _ = std::fs::remove_file(&path);
    alive
}
```

- [ ] **Step 2: Write unit tests for `kill_stale_electron()`**

Add these tests inside the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn kill_stale_electron_returns_false_when_no_file() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(&home)
        .join(".cli-box")
        .join("electron.json");
    let backup = std::fs::read_to_string(&path).ok();
    let _ = std::fs::remove_file(&path);

    let result = kill_stale_electron();
    assert!(
        !result,
        "Should return false when electron.json doesn't exist"
    );

    if let Some(content) = backup {
        let _ = std::fs::write(&path, content);
    }
}

#[test]
fn kill_stale_electron_returns_false_for_dead_pid() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let dir = std::path::PathBuf::from(&home).join(".cli-box");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("electron.json");
    let backup = std::fs::read_to_string(&path).ok();

    // Write a PID that is very unlikely to exist
    let _ = std::fs::write(
        &path,
        serde_json::json!({"pid": 4000000, "port": 15801}).to_string(),
    );

    let result = kill_stale_electron();
    assert!(
        !result,
        "Should return false when PID is not alive"
    );

    if let Some(content) = backup {
        let _ = std::fs::write(&path, content);
    } else {
        let _ = std::fs::remove_file(&path);
    }
}
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p cli-box-cli kill_stale_electron -- --nocapture`
Expected: Both tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): add kill_stale_electron() helper for connection recovery"
```

---

### Task 2: Modify `cmd_start_daemon()` to Retry on Renderer Timeout

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs:470-529`

- [ ] **Step 1: Modify the renderer wait logic to support retry**

Replace the Electron spawn and renderer wait section (lines 470-529) with this new version:

```rust
    // Spawn Electron only if not already running.
    // If already running, the renderer polls /box/list and will pick up the new sandbox.
    let electron_newly_spawned = if find_running_electron() {
        tracing::info!("[start] Electron already running, skipping spawn");
        false
    } else if let Some(electron_bin) = find_electron_binary() {
        tracing::info!("[start] spawning Electron: {}", electron_bin.display());
        let _child = Command::new(&electron_bin)
            .spawn()
            .context("Failed to launch Electron app")?;
        tracing::info!("[start] Electron launched");
        true
    } else {
        tracing::warn!("[start] Electron app not found, running in headless daemon mode");
        false
    };

    use std::io::Write;

    // Phase 1: Wait for renderer WebSocket
    // If Electron was already running (not freshly spawned by us), we may need to
    // kill a stale process and retry if the renderer doesn't connect.
    let mut renderer_connected = false;
    let mut retried = false;

    loop {
        print!("Waiting for renderer");
        let _ = std::io::stdout().flush();

        let timeout = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_secs(1);
        let mut dot_count: u8 = 0;

        let mut connected = false;
        loop {
            if start.elapsed() > timeout {
                println!();
                break;
            }

            match client::daemon_readiness().await {
                Ok(resp) if resp.renderer_connected => {
                    println!(" done");
                    connected = true;
                    break;
                }
                Err(e) => {
                    tracing::trace!("[start] readyz check failed (will retry): {e}");
                }
                _ => {}
            }

            dot_count = (dot_count % 3) + 1;
            print!(
                "\rWaiting for renderer{:<3}",
                ".".repeat(dot_count as usize)
            );
            let _ = std::io::stdout().flush();

            tokio::time::sleep(poll_interval).await;
        }

        if connected {
            renderer_connected = true;
            break;
        }

        // Renderer didn't connect. If Electron was already running (not spawned by us)
        // and we haven't retried yet, kill the stale Electron and respawn.
        if !electron_newly_spawned && !retried {
            retried = true;
            if kill_stale_electron() {
                tracing::info!("[start] Stale Electron killed, respawning...");
                if let Some(electron_bin) = find_electron_binary() {
                    let _child = Command::new(&electron_bin)
                        .spawn()
                        .context("Failed to re-launch Electron app")?;
                    tracing::info!("[start] Electron re-launched");
                    continue; // Retry the wait loop
                }
            }
        }

        tracing::warn!(
            "[start] Renderer WebSocket did not connect within {}s, continuing anyway",
            timeout.as_secs()
        );
        break;
    }
```

- [ ] **Step 2: Run Rust tests to verify compilation**

Run: `cargo test -p cli-box-cli --no-run`
Expected: Compilation succeeds with no errors

- [ ] **Step 3: Run all CLI tests**

Run: `cargo test -p cli-box-cli`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): retry renderer connection when stale Electron detected"
```

---

### Task 3: Add Port Change Detection in Electron Renderer

**Files:**
- Modify: `electron-app/src/renderer/main.tsx:215-225`

- [ ] **Step 1: Modify the WebSocket `onclose` handler**

Replace the current `onclose` handler (lines 215-225):

```typescript
      ws.onclose = () => {
        console.log("[screenshot-ws] disconnected");
        if ((ws as any)._readyInterval) clearInterval((ws as any)._readyInterval);
        if (!unmounted) {
          console.log(`[screenshot-ws] reconnecting in ${reconnectDelay}ms...`);
          reconnectTimeout = setTimeout(() => {
            reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
            connect();
          }, reconnectDelay);
        }
      };
```

With this new version that checks for port changes:

```typescript
      ws.onclose = () => {
        console.log("[screenshot-ws] disconnected");
        if ((ws as any)._readyInterval) clearInterval((ws as any)._readyInterval);
        if (!unmounted) {
          console.log(`[screenshot-ws] reconnecting in ${reconnectDelay}ms...`);
          reconnectTimeout = setTimeout(async () => {
            // Check if daemon port changed (e.g., daemon restarted)
            try {
              const newPort = await window.sandbox.getDaemonPort();
              if (newPort && newPort !== port) {
                console.log(`[screenshot-ws] daemon port changed: ${port} → ${newPort}`);
                setDaemonPort(newPort);
                port = newPort;
              }
            } catch {
              // IPC failed, keep current port
            }
            reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
            connect();
          }, reconnectDelay);
        }
      };
```

- [ ] **Step 2: Run TypeScript type check**

Run: `pnpm typecheck`
Expected: No type errors

- [ ] **Step 3: Run frontend tests**

Run: `pnpm vitest run`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add electron-app/src/renderer/main.tsx
git commit -m "feat(ui): detect daemon port changes on WebSocket reconnect"
```

---

### Task 4: Build and Manual Verification

**Files:**
- No file changes

- [ ] **Step 1: Run full test suite**

Run: `sh test.sh`
Expected: All tests PASS

- [ ] **Step 2: Build release**

Run: `sh release.sh`
Expected: Build succeeds, artifacts in `release/`

- [ ] **Step 3: Manual test — fresh start**

```bash
# Ensure no stale processes
pkill -f "CLI Box" 2>/dev/null
rm -f ~/.cli-box/electron.json

# Start a sandbox
release/cli-box start zsh
```

Expected: Sandbox starts successfully, renderer connects within a few seconds.

- [ ] **Step 4: Manual test — stale Electron recovery**

```bash
# Start a sandbox
release/cli-box start zsh

# Kill only the renderer (simulate stale state)
pkill -f "CLI Box Helper (Renderer)"

# Try starting another sandbox
release/cli-box start zsh
```

Expected: CLI detects stale Electron, kills it, respawns, and the new sandbox works.

- [ ] **Step 5: Commit any fixes**

If manual testing reveals issues, fix and commit.
