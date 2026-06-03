# Phase 5: Multi-Instance Management — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the multi-sandbox management system — stale instance cleanup, window_id propagation from Electron to daemon, instance status lifecycle, and concurrent safety.

**Architecture:** The daemon already manages multiple sandboxes via `DaemonState.sandboxes`. This phase adds: (1) periodic cleanup of dead instances, (2) a `POST /sandbox/{id}/window` endpoint for Electron to report window IDs, (3) proper status transitions (Starting → Running → Stopped).

**Tech Stack:** Rust, tokio, axum

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-core/src/daemon/mod.rs` | Modify | Add cleanup task, window_id endpoint, status transitions |
| `crates/sandbox-cli/src/main.rs` | Modify | Report window_id after Electron connects |
| `crates/sandbox-cli/src/client.rs` | Modify | Add `set_window_id()` client method |

---

### Task 1: Stale instance cleanup

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs`

- [ ] **Step 1: Add cleanup function**

Add after the existing `DaemonState` impl:

```rust
impl DaemonState {
    /// Remove sandboxes whose PTY process is no longer running.
    pub fn cleanup_dead_sandboxes(&mut self) -> Vec<String> {
        let mut removed = Vec::new();
        self.sandboxes.retain(|id, sb| {
            if let Some(pty_pid) = sb.pty_pid {
                // Check if process is still alive
                let alive = unsafe { libc::kill(pty_pid as i32, 0) == 0 };
                if !alive {
                    tracing::info!("Cleaning up dead sandbox {id} (pty_pid={pty_pid})");
                    removed.push(id.clone());
                    return false;
                }
            }
            true
        });
        removed
    }
}
```

- [ ] **Step 2: Spawn cleanup task in run_daemon**

Add inside `run_daemon()` after the server starts:

```rust
// Spawn periodic cleanup task
let cleanup_state = state.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        let mut s = cleanup_state.lock().await;
        s.cleanup_dead_sandboxes();
    }
});
```

- [ ] **Step 3: Add libc dependency if missing**

Check if `Cargo.toml` has `libc`. If not, add to workspace dependencies:

```toml
libc = "0.2"
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs
git commit -m "feat(daemon): add periodic stale sandbox cleanup"
```

---

### Task 2: Window ID propagation endpoint

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs`
- Modify: `crates/sandbox-cli/src/client.rs`

- [ ] **Step 1: Add request type**

```rust
#[derive(Deserialize)]
pub struct SetWindowIdRequest {
    pub window_id: u32,
}
```

- [ ] **Step 2: Add route**

```rust
.route("/sandbox/{id}/window", post(set_window_id_handler))
```

- [ ] **Step 3: Implement handler**

```rust
async fn set_window_id_handler(
    State(state): State<Arc<Mutex<DaemonState>>>,
    Path(id): Path<String>,
    Json(req): Json<SetWindowIdRequest>,
) -> Result<StatusCode, AppError> {
    let mut state = state.lock().await;
    let sandbox = state.sandboxes.get_mut(&id)
        .ok_or_else(|| AppError::BadRequest(format!("Sandbox not found: {id}")))?;
    sandbox.window_id = Some(req.window_id);
    tracing::info!("Set window_id={} for sandbox {}", req.window_id, id);
    // Update instance registry file
    let registry = InstanceRegistry::default();
    if let Ok(mut instance) = registry.get(&id) {
        instance.window_id = Some(req.window_id);
        let _ = registry.update(&instance);
    }
    Ok(StatusCode::OK)
}
```

- [ ] **Step 4: Add client method**

Add to `crates/sandbox-cli/src/client.rs`, following the `daemon_*` pattern:

```rust
pub async fn daemon_set_window_id(sandbox_id: &str, window_id: u32) -> Result<()> {
    let base = daemon_base_url()?;
    let url = format!("{base}/sandbox/{sandbox_id}/window");
    reqwest::Client::new().post(&url)
        .json(&serde_json::json!({ "window_id": window_id }))
        .send().await?
        .error_for_status()?;
    Ok(())
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p sandbox-core && cargo check -p sandbox-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs crates/sandbox-cli/src/client.rs
git commit -m "feat(daemon): add window_id propagation endpoint"
```

---

### Task 3: Instance status lifecycle

**Files:**
- Modify: `crates/sandbox-core/src/daemon/mod.rs`

- [ ] **Step 1: Update status on sandbox creation**

In `create_sandbox_handler`, after PTY is spawned, set status to `Running`:

```rust
sandbox.status = InstanceStatus::Running;
```

- [ ] **Step 2: Update status on cli-box close**

In `close_sandbox_handler`, before removing, set status to `Stopped`:

```rust
if let Some(sb) = state.sandboxes.get_mut(&id) {
    sb.status = InstanceStatus::Stopped;
}
```

- [ ] **Step 3: Update status on PTY exit**

In the cleanup function, when a dead sandbox is found, write `Stopped` to the instance registry before removing:

```rust
let registry = InstanceRegistry::default();
if let Ok(mut instance) = registry.get(id) {
    instance.status = InstanceStatus::Stopped;
    let _ = registry.update(&instance);
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-core/src/daemon/mod.rs
git commit -m "feat(daemon): proper instance status lifecycle transitions"
```

---

### Task 4: Manual verification

- [ ] **Step 1: Build**

Run: `cargo build -p cli-box-daemon && cargo build -p sandbox-cli`

- [ ] **Step 2: Start two sandboxes**

Run: `cargo run -p sandbox-cli -- start zsh && cargo run -p sandbox-cli -- start claude`

- [ ] **Step 3: List sandboxes**

Run: `cargo run -p sandbox-cli -- list`
Expected: Two sandboxes listed with Running status

- [ ] **Step 4: Close one**

Run: `cargo run -p sandbox-cli -- close <id>`
Expected: Sandbox removed from list

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(phase5): multi-instance hardening complete"
```
