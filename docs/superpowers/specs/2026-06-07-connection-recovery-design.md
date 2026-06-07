# Connection Recovery Design

**Date**: 2026-06-07
**Status**: Approved
**Scope**: CLI + Electron renderer connection recovery

---

## Problem

When running `cli-box start <command>`, the CLI waits for two things:
1. Electron renderer WebSocket connection to daemon's `/screenshot/ws` (60s timeout)
2. Terminal readiness signal from renderer (60s timeout)

Both can fail when:
- **Stale Electron process**: A previous Electron process is still alive (PID in `~/.cli-box/electron.json`) but its renderer is disconnected or crashed. The CLI sees "Electron already running" and skips spawning, but the renderer never connects.
- **Daemon port change**: If the daemon restarts on a different port, the renderer keeps trying to connect to the old port.

## Solution

### Part 1: CLI — Timeout Recovery for Stale Electron

**Location**: `crates/cli-box-cli/src/main.rs` — `cmd_start_daemon()`

**Logic**: After renderer connection timeout (60s), attempt one recovery:

1. Read `~/.cli-box/electron.json` to get the Electron PID
2. Check if process is alive via `kill(pid, 0)`
3. If alive but renderer not connected → kill with `SIGTERM`
4. Clean up `electron.json`
5. Spawn fresh Electron
6. Wait again for renderer connection (60s)
7. If still fails → print warning, continue (non-blocking)

**New helper function**:
```rust
fn kill_stale_electron() -> bool {
    // Read electron.json, check PID, kill if alive, cleanup
    // Returns true if a stale process was found and killed
}
```

**Key constraints**:
- Maximum 1 retry (prevent infinite loops)
- Recovery only attempted when `electron_newly_spawned == false` (Electron was "already running")
- If Electron was freshly spawned by this invocation, no retry (the spawn itself failed)

### Part 2: Electron — Port Change Detection on Reconnect

**Location**: `electron-app/src/renderer/main.tsx` — screenshot WebSocket `onclose` handler

**Current behavior**: On WebSocket close, reconnect using the same port with exponential backoff.

**New behavior**: On WebSocket close, before reconnecting:
1. Call `window.sandbox.getDaemonPort()` via IPC to get the current daemon port
2. If port changed → update `setDaemonPort(newPort)` and local `port` variable
3. Reconnect with the (possibly updated) port

**Modified `onclose` handler**:
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

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Electron alive, renderer disconnected | CLI kills Electron, respawns |
| Electron alive, renderer connected but slow | Normal timeout, no retry |
| Electron dead, electron.json stale | CLI cleans up, spawns fresh |
| Daemon restarts, same port | Renderer reconnects normally |
| Daemon restarts, different port | Renderer detects port change, reconnects |
| Recovery still fails | CLI prints warning, continues |

## Testing

### Unit Tests
- `kill_stale_electron()` returns false when no electron.json
- `kill_stale_electron()` returns false when PID is dead
- `kill_stale_electron()` returns true when PID is alive (mock kill)

### Integration Tests
- CLI recovers when Electron process is alive but renderer disconnected
- Renderer reconnects after daemon port change

### Manual Test
1. Start a sandbox: `cli-box start zsh`
2. Kill the renderer process (not Electron main): `pkill -f "CLI Box Helper (Renderer)"`
3. Run `cli-box start zsh` again → should detect stale Electron, kill, respawn
4. Verify new sandbox works
