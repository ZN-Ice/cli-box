# PTY Dedicated Reader Thread Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the blocking take/read/put-back PTY reader pattern with a dedicated background reader thread + shared buffer, fixing TUI apps (opencode, vim, htop) that produce burst output followed by idle periods.

**Architecture:** Each PTY session spawns a background `std::thread` that continuously reads from the PTY master in a blocking loop and appends data to a shared `Arc<Mutex<VecDeque<String>>>` buffer. The HTTP handler (`read_output`) drains from this buffer non-blocking. A stop flag (`Arc<AtomicBool>`) signals the thread to exit when the session is killed.

**Tech Stack:** Rust, `portable_pty 0.9`, `std::thread`, `std::sync::{Mutex, Arc, AtomicBool}`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sandbox-core/src/process/mod.rs` | Modify | Add reader thread, buffer, stop flag to PtySession; rewrite `read_output` to drain buffer |

**Only one file changes.** The server handler (`server/mod.rs`) and frontend (`Terminal.tsx`) are unaffected — `read_output` keeps the same signature.

---

### Task 1: Add buffer, thread handle, and stop flag to PtySession

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs:1-13` (imports)
- Modify: `crates/sandbox-core/src/process/mod.rs:31-38` (PtySession struct)

- [ ] **Step 1: Add imports**

Add `VecDeque`, `AtomicBool`, and `Arc` to the existing imports at the top of the file:

```rust
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, trace, warn};
```

- [ ] **Step 2: Add buffer and thread fields to PtySession**

Replace the PtySession struct (lines 31-38) with:

```rust
struct PtySession {
    reader: Option<Box<dyn std::io::Read + Send>>,
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn MasterPty>,
    #[allow(dead_code)]
    child_pid: u32,
    command: String,
    /// Buffer for output from the dedicated reader thread
    buffer: Arc<Mutex<VecDeque<String>>>,
    /// Flag to signal the reader thread to stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the reader thread (for join on cleanup)
    reader_thread: Option<std::thread::JoinHandle<()>>,
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: compiles (struct fields are added but not yet used in constructors)

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "refactor(process): add buffer and thread fields to PtySession

Prepares PtySession for dedicated reader thread by adding shared
buffer (VecDeque), stop flag (AtomicBool), and thread handle."
```

---

### Task 2: Spawn dedicated reader thread in spawn_cli

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs:119-178` (spawn_cli function)

- [ ] **Step 1: Create reader thread and buffer in spawn_cli**

In `spawn_cli` (the `#[cfg(target_os = "macos")]` version), after cloning the reader and before inserting into `SESSIONS`, create the buffer, stop flag, and reader thread. Replace the `sessions.insert(...)` block (lines 153-165) with:

```rust
let buffer: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
let stop_flag = Arc::new(AtomicBool::new(false));

// Clone reader for the dedicated thread (original goes into the thread)
let mut reader_for_thread = {
    // We need to take ownership of the reader — re-clone before moving
    // The try_clone_reader() already gave us one reader; move it to the thread
    reader
};

let thread_buffer = buffer.clone();
let thread_stop = stop_flag.clone();
let thread_pid = tracked_id;

let reader_thread = std::thread::Builder::new()
    .name(format!("pty-reader-{tracked_id}"))
    .spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            if thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                debug!("PTY reader thread pid={thread_pid}: stop flag set, exiting");
                break;
            }
            match reader_for_thread.read(&mut buf) {
                Ok(0) => {
                    debug!("PTY reader thread pid={thread_pid}: EOF (process exited)");
                    break;
                }
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    trace!("PTY reader thread pid={thread_pid}: read {n} bytes");
                    if let Ok(mut buffer) = thread_buffer.lock() {
                        // Cap buffer at 1000 entries (~4MB) to prevent memory growth
                        while buffer.len() >= 1000 {
                            buffer.pop_front();
                        }
                        buffer.push_back(text);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    warn!("PTY reader thread pid={thread_pid}: read error: {e}");
                    break;
                }
            }
        }
        debug!("PTY reader thread pid={thread_pid}: thread exiting");
    })
    .map_err(|e| AppError::Process(format!("Failed to spawn reader thread: {e}")))?;

let mut sessions = SESSIONS
    .lock()
    .map_err(|e| AppError::Process(e.to_string()))?;
sessions.insert(
    tracked_id,
    PtySession {
        reader: None, // reader moved to thread
        writer,
        master: pty_pair.master,
        child_pid: child_pid.unwrap_or(0),
        command: command.to_string(),
        buffer,
        stop_flag,
        reader_thread: Some(reader_thread),
    },
);
```

Note: The original `reader` variable (from `try_clone_reader`) is moved into the thread. `PtySession.reader` is set to `None` because the reader thread owns it now.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: compiles. There may be warnings about unused `reader` field — that's fine, it will be used by `read_output` in the next task.

- [ ] **Step 3: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "feat(process): spawn dedicated reader thread per PTY session

A background thread continuously reads PTY output into a shared
VecDeque buffer. This eliminates the blocking read / reader-busy
race condition that prevented TUI apps from rendering."
```

---

### Task 3: Rewrite read_output to drain from buffer

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs:320-369` (read_output function, macOS version)

- [ ] **Step 1: Replace read_output implementation**

Replace the entire `#[cfg(target_os = "macos")] pub fn read_output` function (lines 319-369) with:

```rust
/// Read output from a PTY process.
///
/// Drains from the shared buffer filled by the dedicated reader thread.
/// Non-blocking: returns immediately with whatever data is available.
#[cfg(target_os = "macos")]
pub fn read_output(pid: u32) -> Result<Option<String>> {
    let sessions = SESSIONS
        .lock()
        .map_err(|e| AppError::Process(e.to_string()))?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| AppError::Process(format!("Process {pid} not found")))?;

    let mut buffer = session
        .buffer
        .lock()
        .map_err(|e| AppError::Process(format!("Buffer lock poisoned: {e}")))?;

    if buffer.is_empty() {
        return Ok(None);
    }

    // Drain all available data, concatenating into a single string
    let mut output = String::new();
    while let Some(chunk) = buffer.pop_front() {
        output.push_str(&chunk);
    }

    debug!("PTY pid={pid}: drained {} chars from buffer", output.len());
    Ok(Some(output))
}
```

- [ ] **Step 2: Remove the `use std::io::Read;` inside read_output**

The old `read_output` had `use std::io::Read;` at line 321 inside the function body. This is no longer needed — remove it if it's still present.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: compiles cleanly

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p sandbox-core`
Expected: all existing tests pass (read_output signature unchanged)

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "feat(process): rewrite read_output to drain from shared buffer

Replaces blocking take/read/put-back with non-blocking buffer drain.
The dedicated reader thread continuously fills the buffer, and
read_output drains all available data in one call.
```

---

### Task 4: Signal stop flag and join thread in kill_process

**Files:**
- Modify: `crates/sandbox-core/src/process/mod.rs:204-229` (kill_process function, macOS version)

- [ ] **Step 1: Signal stop flag and join reader thread in kill_process**

In `kill_process` (the `#[cfg(target_os = "macos")]` version), after removing the session from `SESSIONS` and before dropping it, signal the stop flag and join the thread. Replace the function body (lines 206-229) with:

```rust
/// Kill a process by tracked PID
#[cfg(target_os = "macos")]
pub fn kill_process(pid: u32) -> Result<()> {
    let session = {
        let mut sessions = SESSIONS
            .lock()
            .map_err(|e| AppError::Process(e.to_string()))?;
        sessions
            .remove(&pid)
            .ok_or_else(|| AppError::Process(format!("Process {pid} not found in sandbox")))?
    };

    // Signal the reader thread to stop
    session
        .stop_flag
        .store(true, std::sync::atomic::Ordering::Relaxed);

    // Kill the actual OS child process
    let os_pid = session.child_pid;
    if os_pid > 0 {
        kill(Pid::from_raw(os_pid as i32), Signal::SIGTERM).map_err(|e| {
            AppError::Process(format!("Failed to kill process {os_pid}: {e}"))
        })?;
    }

    // Join the reader thread (with timeout to avoid hanging)
    if let Some(handle) = session.reader_thread {
        match handle.join() {
            Ok(()) => debug!("Reader thread for pid={pid} joined successfully"),
            Err(e) => warn!("Reader thread for pid={pid} panicked during join: {:?}", e),
        }
    }

    // Dropping the master closes the PTY
    drop(session);
    info!("Killed process: tracked_id={}, os_pid={}", pid, os_pid);
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p sandbox-core`
Expected: compiles cleanly

- [ ] **Step 3: Run existing tests**

Run: `cargo test -p sandbox-core`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-core/src/process/mod.rs
git commit -m "fix(process): signal stop flag and join reader thread on kill

Ensures clean shutdown of the dedicated reader thread when a
PTY session is killed, preventing thread leaks."
```

---

### Task 5: End-to-end test with release build

**Files:** None (testing only)

- [ ] **Step 1: Build release**

Run: `./release.sh`
Expected: build succeeds, app bundle created

- [ ] **Step 2: Test zsh scenario (regression check)**

Start the sandbox with zsh, type `echo "hello world"`, press Enter, screenshot:

```bash
# Start sandbox
./target/release/bundle/macos/System\ Test\ Sandbox.app/Contents/MacOS/cli-box --mode=cli --cmd=zsh &

# Wait for startup, then get port from registry
SANDBOX_ID=$(ls ~/.sandbox/instances/ | head -1 | sed 's/.json//')
PORT=$(cat ~/.sandbox/instances/${SANDBOX_ID}.json | python3 -c "import sys,json;print(json.load(sys.stdin)['port'])")

# Type and send
curl -s -X POST "http://127.0.0.1:${PORT}/pty/write/${SANDBOX_ID%% *}" \
  -H "Content-Type: application/json" \
  -d '{"data":"echo \"hello world\"\r"}' > /dev/null

# Wait for output
sleep 1

# Screenshot
curl -s "http://127.0.0.1:${PORT}/screenshot" -o /tmp/test_zsh.png
```

Expected: screenshot shows "hello world" output

- [ ] **Step 3: Test opencode scenario (the bug fix)**

```bash
# Start sandbox with opencode
./target/release/bundle/macos/System\ Test\ Sandbox.app/Contents/MacOS/cli-box --mode=cli --cmd=opencode &

# Wait for startup
sleep 3

SANDBOX_ID=$(ls ~/.sandbox/instances/ | head -1 | sed 's/.json//')
PORT=$(cat ~/.sandbox/instances/${SANDBOX_ID}.json | python3 -c "import sys,json;print(json.load(sys.stdin)['port'])")
PID=1000  # or read from processes endpoint

# Type input
curl -s -X POST "http://127.0.0.1:${PORT}/pty/write/${PID}" \
  -H "Content-Type: application/json" \
  -d '{"data":"你是谁？\r"}'

# Wait for response
sleep 3

# Screenshot
curl -s "http://127.0.0.1:${PORT}/screenshot" -o /tmp/test_opencode.png
```

Expected: screenshot shows opencode TUI with input and response visible (not blank)

- [ ] **Step 4: Clean up**

Kill sandbox processes and clean up test screenshots.

- [ ] **Step 5: Final commit (if test artifacts need committing)**

No commit needed — this is a manual verification step.

---

### Task 6: Update CLAUDE.md with architecture note

**Files:**
- Modify: `CLAUDE.md` (add note about reader thread architecture)

- [ ] **Step 1: Add reader thread note to CLAUDE.md**

In the `process/mod.rs` description or the architecture section, add a note:

```markdown
**PTY Reader Thread**: Each PTY session spawns a dedicated background thread
that continuously reads output into a shared buffer. The HTTP `/pty/output/:pid`
endpoint drains from this buffer non-blocking. This replaces the earlier
take/read/put-back pattern that blocked on idle TUI apps (opencode, vim, etc.).
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document PTY reader thread architecture in CLAUDE.md"
```
