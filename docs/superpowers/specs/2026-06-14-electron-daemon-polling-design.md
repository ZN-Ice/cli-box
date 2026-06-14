# Electron Daemon Polling Design

**Date**: 2026-06-14
**Status**: Draft

## Problem

Currently, Electron's `ensureDaemon()` either:
1. **Finds an existing daemon** → reuses it
2. **Spawns a new daemon** as a child process
3. **Quits** if spawning fails

This creates several problems:
- **WebSocket race condition**: If CLI spawns daemon *after* Electron starts, Electron's WebSocket fails to connect (no reconnection logic)
- **Double ownership**: Both Electron and CLI can spawn daemons, leading to port conflicts
- **Stale `electron.json`**: Manual `pkill` of Electron leaves the file with dead PID, breaking `kill_stale_electron` retry logic
- **Tight coupling**: Electron's launch is gated on daemon availability, which makes GUI-only usage impossible

## Solution

Refactor the launch sequence so that:

1. **Electron never auto-spawns daemon on launch** — it only polls for an existing daemon
2. **Electron only spawns daemon on-demand** when the user tries to create a sandbox from the GUI
3. **Renderer polls daemon status** every 1s and displays a clear UI state
4. **GUI shows red error modal** if daemon startup fails
5. **CLI prints structured error** to its own stderr when daemon operations fail

## Architecture

### Launch Sequence (Scenario A: CLI first)

```
1. User: cli-box start claude
2. CLI: spawn daemon (PID A, port 15801), write daemon.json
3. CLI: spawn Electron
4. Electron: whenReady → findRunningDaemon() → found PID A → connect
5. Renderer: connected=true, load sandboxes
```

### Launch Sequence (Scenario B: Electron standalone)

```
1. User: double-click CLI Box.app
2. Electron: whenReady → findRunningDaemon() → NOT found
3. Renderer: connected=false, show "Waiting for daemon..." UI
4. Renderer: poll /readyz every 1s
5. User: open terminal, run cli-box start
6. CLI: spawn daemon
7. Renderer: poll detects daemon, connected=true, refresh sandboxes
```

### Launch Sequence (Scenario C: GUI creates sandbox, no daemon)

```
1. User: double-click CLI Box.app (no daemon)
2. Electron: whenReady → findRunningDaemon() → NOT found
3. Renderer: show "Waiting for daemon..." UI
4. User: clicks "New Sandbox" button in GUI
5. Renderer: IPC → main → ensureDaemonOnDemand()
6. Main: spawn daemon subprocess
7. Main: wait for daemon.json (up to 5s)
8. Main: create sandbox via daemon HTTP API
9. Main: notify renderer to refresh tab list
10. Renderer: show new tab
```

## Component Changes

### 1. `electron-app/src/main/daemon-bridge.ts`

```typescript
// New: poll for daemon without spawning
export async function waitForDaemon(timeoutMs = 0): Promise<number> {
  const start = Date.now();
  while (true) {
    const port = findRunningDaemon();
    if (port) return port;
    if (timeoutMs > 0 && Date.now() - start > timeoutMs) {
      throw new Error(`Daemon not available within ${timeoutMs}ms`);
    }
    await new Promise((r) => setTimeout(r, 1000)); // 1s poll interval
  }
}

// Renamed from ensureDaemon: only spawn on demand
export async function ensureDaemonOnDemand(): Promise<number> {
  const existing = findRunningDaemon();
  if (existing) return existing;
  // ... existing spawn logic
  return port;
}
```

### 2. `electron-app/src/main/index.ts`

**Before**:
```typescript
app.whenReady().then(async () => {
  try {
    daemonPort = await ensureDaemon();  // Spawns or fails
  } catch (err) {
    console.error("Failed to start daemon:", err);
    app.quit();  // Quits Electron entirely
    return;
  }
  writeElectronJson(daemonPort);
  createWindow();
});
```

**After**:
```typescript
let daemonStartedByElectron = false;

app.whenReady().then(async () => {
  // Always try to find existing daemon first
  const existingPort = findRunningDaemon();
  if (existingPort) {
    daemonPort = existingPort;
  }
  // Don't spawn — just create the window
  // If daemon is missing, the renderer will show "waiting" state
  createWindow();
});

// New IPC handler for on-demand daemon start
ipcMain.handle("ensure-daemon", async () => {
  if (daemonStartedByElectron) return daemonPort;
  const port = await ensureDaemonOnDemand();
  daemonPort = port;
  daemonStartedByElectron = true;
  return port;
});
```

### 3. `electron-app/src/renderer/main.tsx`

**Before** (one-shot):
```typescript
useEffect(() => {
  window.sandbox.getDaemonPort().then((port) => {
    setDaemonPort(port);
    setConnected(true);
    refreshSandboxes();
  });
}, []);
```

**After** (polling):
```typescript
useEffect(() => {
  let cancelled = false;
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let pollDelay: ReturnType<typeof setTimeout> | null = null;

  function poll() {
    if (cancelled) return;
    window.sandbox.getDaemonPort().then(async (port) => {
      if (cancelled) return;
      if (port && port > 0) {
        if (port !== daemonPort) {
          setDaemonPort(port);
          setConnected(true);
          refreshSandboxes();
        }
      } else {
        setConnected(false);
        // Re-poll in 1s
        pollDelay = setTimeout(poll, 1000);
      }
    }).catch(() => {
      if (cancelled) return;
      setConnected(false);
      pollDelay = setTimeout(poll, 1000);
    });
  }

  poll();
  return () => {
    cancelled = true;
    if (pollInterval) clearInterval(pollInterval);
    if (pollDelay) clearTimeout(pollDelay);
  };
}, []);
```

### 4. New UI States

Add a "Waiting for daemon" view when `!connected`:

```tsx
{!connected && (
  <div className="daemon-waiting">
    <h2>Waiting for cli-box-daemon...</h2>
    <p>To start the daemon, run in a terminal:</p>
    <code>cli-box start</code>
    <p>This window will connect automatically.</p>
  </div>
)}
```

### 5. New Red Error Modal

When daemon start fails (triggered from GUI), show a red modal:

```tsx
{daemonError && (
  <div className="error-modal">
    <div className="error-modal-content">
      <h2 style={{ color: 'red' }}>Failed to start daemon</h2>
      <pre>{daemonError}</pre>
      <button onClick={retryDaemon}>Retry</button>
      <button onClick={closeError}>Close</button>
    </div>
  </div>
)}
```

### 6. CLI Error Output

CLI already prints errors via `tracing::warn!` and `eprintln!`. The key addition: when CLI exits because daemon operation failed, print a clear message to **CLI stderr**:

```rust
// In CLI commands
eprintln!("Error: Failed to create sandbox: {err}");
eprintln!("Hint: Run 'cli-box start' in another terminal to start the daemon.");
```

## Data Flow

### IPC API Changes

| IPC | Direction | Purpose |
|-----|-----------|---------|
| `get-daemon-port` | renderer → main | Returns current port (null if not connected) |
| `ensure-daemon` | renderer → main | Spawn daemon on demand, returns port |
| `daemon-status` | main → renderer | Broadcast status changes (started, exited) |

### Polling Behavior

- **Polling interval**: 1s (as user requested)
- **Polling endpoint**: `GET /readyz` (already exists in daemon)
- **Polling condition**: renderer polls only when not connected; once connected, polling stops

### Error Display

| Error | CLI Output | GUI Display |
|-------|-----------|-------------|
| Daemon binary not found | `Error: cli-box-daemon not found at <path>` | Red modal: "Daemon binary not found" |
| Port already in use | `Error: port 15801 already in use` | Red modal: "Port conflict" |
| Daemon crashes mid-operation | `Error: connection lost: <details>` | Red modal: "Daemon disconnected" |
| Sandbox creation fails | `Error: failed to create sandbox: <details>` | Red modal: "Failed to create sandbox" |

## Testing Strategy

### Unit Tests
- `daemon-bridge.ts`: mock file system for `findRunningDaemon()`, verify polling behavior
- `waitForDaemon()`: verify timeout, retry, success paths

### E2E Tests
1. CLI first: spawn daemon → spawn Electron → connect → create sandbox
2. Electron standalone: launch app → wait for daemon → user runs CLI → connect
3. GUI create with no daemon: click "New" → trigger daemon start → create
4. Daemon crash: kill daemon mid-session → GUI shows red error → user retries

### Regression Tests
- Existing CLI workflows (zsh, opencode, claude) still work
- Existing `mcp-serve` workflow still works
- Multiple sandboxes still work

## Migration Path

1. Add `waitForDaemon()` and `ensureDaemonOnDemand()` to `daemon-bridge.ts`
2. Update `index.ts` to remove `ensureDaemon()` from `whenReady`, add `ensure-daemon` IPC
3. Update `renderer/main.tsx` with polling logic and "waiting" UI
4. Add red error modal component
5. Add CLI stderr hints for daemon operations
6. Test all three scenarios

## Out of Scope

- Changing the daemon's HTTP API
- Changing the WebSocket protocol
- Adding daemon reconnection logic on the server side (daemon is short-lived anyway)
- Supporting multiple daemon instances

## Open Questions

None — all questions resolved during brainstorming:

| Question | Answer |
|----------|--------|
| Polling interval | 1s |
| On-demand daemon start | Yes, when GUI creates sandbox |
| Error display | CLI: text to stderr, GUI: red modal |
| Daemon binary path | Use existing `findDaemonBinary()` (dev/prod/same-dir) |
