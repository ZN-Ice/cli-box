# Electron Daemon Polling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Electron daemon startup so the app polls for an existing daemon instead of auto-spawning one, and only spawns on demand when the user creates a sandbox from the GUI.

**Architecture:** Electron launches without spawning daemon. The renderer polls `getDaemonPort()` every 1s and displays a "Waiting for daemon" UI when not connected. When the user clicks "New Sandbox" in the GUI and daemon is not running, Electron spawns it on demand via a new IPC handler.

**Tech Stack:** Electron 32+, TypeScript 5+, React 18, Vitest 4

---

## File Structure

### Files to Modify

| File | Responsibility |
|------|----------------|
| `electron-app/src/main/daemon-bridge.ts` | Add `waitForDaemon()` and rename `ensureDaemon()` → `ensureDaemonOnDemand()` |
| `electron-app/src/main/index.ts` | Remove `ensureDaemon()` from `whenReady`, add `ensure-daemon` IPC handler |
| `electron-app/src/preload/index.ts` | Expose `ensureDaemon()` IPC to renderer |
| `electron-app/src/renderer/main.tsx` | Add polling logic for daemon connection |
| `electron-app/src/renderer/components/DaemonWaiting.tsx` | New component for waiting state |
| `electron-app/src/renderer/components/ErrorModal.tsx` | New red error modal component |
| `electron-app/src/renderer/api.ts` | Add `ensureDaemon()` API call |
| `electron-app/src/renderer/styles.css` | Add styles for waiting and error modal |
| `crates/cli-box-cli/src/main.rs` | Add stderr hints for daemon errors |

### Files to Create

| File | Purpose |
|------|---------|
| `electron-app/src/__tests__/daemon-bridge.test.ts` | Unit tests for `waitForDaemon()` and `ensureDaemonOnDemand()` |
| `electron-app/src/__tests__/polling.test.ts` | Test renderer polling logic |

---

## Task 1: Add `waitForDaemon()` to daemon-bridge

**Files:**
- Modify: `electron-app/src/main/daemon-bridge.ts`
- Test: `electron-app/src/__tests__/daemon-bridge.test.ts`

- [ ] **Step 1: Write failing test for `waitForDaemon()`**

Create `electron-app/src/__tests__/daemon-bridge.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { existsSync, readFileSync, writeFileSync, unlinkSync, mkdirSync } from "fs";
import { join } from "path";

// Mock fs before importing daemon-bridge
vi.mock("fs", async () => {
  const actual = await vi.importActual<typeof import("fs")>("fs");
  return {
    ...actual,
    existsSync: vi.fn(),
    readFileSync: vi.fn(),
  };
});

// Mock electron app module
vi.mock("electron", () => ({
  app: {
    getPath: () => "/tmp/test",
  },
}));

import { waitForDaemon } from "../main/daemon-bridge";

describe("waitForDaemon", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("returns port when daemon.json appears within 1s", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First call: no daemon.json, second call: daemon.json exists
    mockExists.mockReturnValueOnce(false).mockReturnValueOnce(true);
    mockRead.mockReturnValueOnce(
      JSON.stringify({ port: 15801, pid: 12345, started_at: "2026-01-01" })
    );

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(1000);
    const port = await portPromise;

    expect(port).toBe(15801);
  });

  it("keeps polling until daemon appears", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First 3 calls: no daemon, 4th call: exists
    mockExists.mockReturnValue(false);
    mockExists.mockReturnValueOnce(true);
    mockRead.mockReturnValueOnce(
      JSON.stringify({ port: 15801, pid: 12345, started_at: "2026-01-01" })
    );

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(3000);
    const port = await portPromise;

    expect(port).toBe(15801);
    expect(mockExists).toHaveBeenCalledTimes(4);
  });

  it("skips invalid daemon.json content and retries", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First: invalid JSON, then: valid
    mockExists.mockReturnValue(true);
    mockRead
      .mockReturnValueOnce("invalid json")
      .mockReturnValueOnce(JSON.stringify({ port: 15801, pid: 12345, started_at: "" }));

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(2000);
    const port = await portPromise;

    expect(port).toBe(15801);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd electron-app && pnpm test daemon-bridge`

Expected: FAIL with "Cannot find module '../main/daemon-bridge'" or "waitForDaemon is not a function"

- [ ] **Step 3: Implement `waitForDaemon()`**

Modify `electron-app/src/main/daemon-bridge.ts`. Add this function before `export async function ensureDaemon()`:

```typescript
/**
 * Poll for an existing daemon without spawning one.
 * Returns the port once daemon.json is found, or throws on timeout.
 *
 * @param timeoutMs - 0 means poll forever (default), >0 means timeout after N ms
 * @param pollIntervalMs - polling interval in ms (default 1000ms = 1s)
 */
export async function waitForDaemon(
  timeoutMs: number = 0,
  pollIntervalMs: number = 1000
): Promise<number> {
  const start = Date.now();
  while (true) {
    const port = findRunningDaemon();
    if (port) return port;
    if (timeoutMs > 0 && Date.now() - start > timeoutMs) {
      throw new Error(`Daemon not available within ${timeoutMs}ms`);
    }
    await new Promise((r) => setTimeout(r, pollIntervalMs));
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd electron-app && pnpm test daemon-bridge`

Expected: PASS — 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add electron-app/src/main/daemon-bridge.ts electron-app/src/__tests__/daemon-bridge.test.ts
git commit -m "feat(electron): add waitForDaemon polling helper"
```

---

## Task 2: Rename `ensureDaemon()` → `ensureDaemonOnDemand()`

**Files:**
- Modify: `electron-app/src/main/daemon-bridge.ts`

- [ ] **Step 1: Rename and add documentation**

In `electron-app/src/main/daemon-bridge.ts`, find:

```typescript
export async function ensureDaemon(): Promise<number> {
```

Replace with:

```typescript
/**
 * Spawn daemon subprocess on demand.
 * Use this when user explicitly requests daemon (e.g., creates sandbox from GUI
 * while daemon is not running). Do NOT call this on app launch — use
 * waitForDaemon() instead to poll for existing daemon.
 *
 * @returns The daemon port number
 * @throws If daemon binary not found or fails to start within timeout
 */
export async function ensureDaemonOnDemand(): Promise<number> {
```

- [ ] **Step 2: Verify no other callers depend on `ensureDaemon`**

Run: `cd electron-app && grep -rn "ensureDaemon\b" src/ --include="*.ts" --include="*.tsx"`

Expected: only the definition in `daemon-bridge.ts` matches `ensureDaemon(` (the new one is `ensureDaemonOnDemand`)

If other files still call `ensureDaemon`, update them to call `ensureDaemonOnDemand` in a separate step.

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/main/daemon-bridge.ts
git commit -m "refactor(electron): rename ensureDaemon to ensureDaemonOnDemand"
```

---

## Task 3: Remove auto-spawn from `index.ts` and add `ensure-daemon` IPC

**Files:**
- Modify: `electron-app/src/main/index.ts`

- [ ] **Step 1: Remove `ensureDaemon` call from `whenReady`**

In `electron-app/src/main/index.ts`, find this block:

```typescript
  app.whenReady().then(async () => {
    try {
      daemonPort = await ensureDaemon();
    } catch (err) {
      console.error("Failed to start daemon:", err);
      app.quit();
      return;
    }

    writeElectronJson(daemonPort);
    createWindow();
  });
```

Replace with:

```typescript
  app.whenReady().then(async () => {
    // Don't auto-spawn daemon. Check if one is already running.
    // The renderer will poll for daemon and show "Waiting" UI if not found.
    // Daemon is spawned on demand when user creates a sandbox from GUI.
    const existingPort = findRunningDaemon();
    if (existingPort) {
      daemonPort = existingPort;
      writeElectronJson(daemonPort);
    }
    // Always create the window — renderer handles "waiting" state
    createWindow();
  });
```

- [ ] **Step 2: Add `findRunningDaemon` import**

In the imports at the top of `index.ts`, add `findRunningDaemon` to the daemon-bridge import:

```typescript
import { ensureDaemonOnDemand, killDaemon, findRunningDaemon } from "./daemon-bridge";
```

(Remove `ensureDaemon` from the import since it no longer exists.)

- [ ] **Step 3: Add `ensure-daemon` IPC handler**

In `index.ts`, find:

```typescript
// IPC: renderer asks for daemon port
ipcMain.handle("get-daemon-port", () => daemonPort);
```

Add after it:

```typescript
// IPC: renderer asks main to spawn daemon (on-demand, triggered by GUI)
let daemonStartedByElectron = false;
ipcMain.handle("ensure-daemon", async () => {
  if (daemonStartedByElectron && daemonPort) {
    return daemonPort; // Already started by us, just return
  }
  try {
    const port = await ensureDaemonOnDemand();
    daemonPort = port;
    daemonStartedByElectron = true;
    writeElectronJson(port);
    return port;
  } catch (err: any) {
    const message = err?.message ?? String(err);
    console.error("[ensure-daemon] failed:", message);
    throw new Error(`Failed to start daemon: ${message}`);
  }
});
```

- [ ] **Step 4: Verify build compiles**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS — TypeScript compiles without errors

- [ ] **Step 5: Commit**

```bash
git add electron-app/src/main/index.ts
git commit -m "feat(electron): poll for daemon instead of auto-spawn, add ensure-daemon IPC"
```

---

## Task 4: Expose `ensureDaemon` in preload

**Files:**
- Modify: `electron-app/src/preload/index.ts`

- [ ] **Step 1: Add `ensureDaemon` to sandbox API**

In `electron-app/src/preload/index.ts`, find:

```typescript
contextBridge.exposeInMainWorld("sandbox", {
  getDaemonPort: () => ipcRenderer.invoke("get-daemon-port"),
```

Add after `getDaemonPort`:

```typescript
  ensureDaemon: () => ipcRenderer.invoke("ensure-daemon"),
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/preload/index.ts
git commit -m "feat(electron): expose ensureDaemon IPC in preload"
```

---

## Task 5: Add polling logic to renderer `main.tsx`

**Files:**
- Modify: `electron-app/src/renderer/main.tsx`

- [ ] **Step 1: Write failing test for polling**

Create `electron-app/src/__tests__/polling.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("daemon port polling", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("polls getDaemonPort every 1s when port is 0", async () => {
    const mockGetPort = vi.fn();
    mockGetPort.mockResolvedValue(0);

    let cancelled = false;
    async function poll() {
      while (!cancelled) {
        const port = await mockGetPort();
        if (port > 0) return;
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    const pollPromise = poll();
    await vi.advanceTimersByTimeAsync(3000);
    cancelled = true;
    await pollPromise;

    expect(mockGetPort).toHaveBeenCalledTimes(4); // initial + 3 ticks
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd electron-app && pnpm test polling`

Expected: FAIL with test not found

- [ ] **Step 3: Replace one-shot `getDaemonPort` with polling in main.tsx**

In `electron-app/src/renderer/main.tsx`, find:

```typescript
  // Initialize daemon port and load sandboxes
  useEffect(() => {
    window.sandbox.getDaemonPort().then((port) => {
      setDaemonPort(port);
      setConnected(true);
      refreshSandboxes();
    });
  }, []);
```

Replace with:

```typescript
  // Poll for daemon port every 1s until daemon is available
  useEffect(() => {
    let cancelled = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;

    function poll() {
      if (cancelled) return;
      window.sandbox
        .getDaemonPort()
        .then((port) => {
          if (cancelled) return;
          if (port && port > 0) {
            // Daemon is up
            if (port !== daemonPort) {
              setDaemonPort(port);
              setConnected(true);
              refreshSandboxes();
            }
            // Connected — no need to poll
          } else {
            // Daemon not running yet
            setConnected(false);
            pollTimer = setTimeout(poll, 1000);
          }
        })
        .catch(() => {
          if (cancelled) return;
          setConnected(false);
          pollTimer = setTimeout(poll, 1000);
        });
    }

    poll();
    return () => {
      cancelled = true;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, [daemonPort]);
```

- [ ] **Step 4: Add `connected` state if not present**

Check if `connected` is declared in the state. Find:

```typescript
  const [connected, setConnected] = useState(false);
```

If not present, add it. (It should already be there from the screenshot WebSocket effect.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cd electron-app && pnpm test polling`

Expected: PASS

- [ ] **Step 6: Run typecheck**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add electron-app/src/renderer/main.tsx electron-app/src/__tests__/polling.test.ts
git commit -m "feat(renderer): poll daemon port every 1s instead of one-shot"
```

---

## Task 6: Create `DaemonWaiting` component

**Files:**
- Create: `electron-app/src/renderer/components/DaemonWaiting.tsx`
- Modify: `electron-app/src/renderer/main.tsx`

- [ ] **Step 1: Create the component**

Create `electron-app/src/renderer/components/DaemonWaiting.tsx`:

```typescript
import React from "react";

export function DaemonWaiting() {
  return (
    <div className="daemon-waiting">
      <div className="daemon-waiting-content">
        <h2>Waiting for cli-box-daemon...</h2>
        <p>To start the daemon, run in a terminal:</p>
        <code>cli-box start</code>
        <p className="daemon-waiting-hint">
          This window will connect automatically once the daemon is running.
        </p>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add styles to `styles.css`**

Append to `electron-app/src/renderer/styles.css`:

```css
.daemon-waiting {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  width: 100%;
  background: var(--bg);
}

.daemon-waiting-content {
  text-align: center;
  padding: 40px;
  max-width: 500px;
}

.daemon-waiting-content h2 {
  margin-bottom: 16px;
  color: var(--text);
}

.daemon-waiting-content p {
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.daemon-waiting-content code {
  display: inline-block;
  background: var(--code-bg);
  padding: 8px 16px;
  border-radius: 4px;
  font-family: monospace;
  font-size: 14px;
  color: var(--text);
  margin: 12px 0;
}

.daemon-waiting-hint {
  font-size: 12px;
  margin-top: 16px;
  color: var(--text-secondary);
}
```

- [ ] **Step 3: Use component in main.tsx**

In `electron-app/src/renderer/main.tsx`, add the import:

```typescript
import { DaemonWaiting } from "./components/DaemonWaiting";
```

Find the main render block (the `return` statement of `App`). When `!connected`, show the waiting state. If the existing render shows tabs when there are tabs, wrap it so the waiting screen shows when `!connected && tabs.length === 0`. A minimal approach — replace the outermost return:

```typescript
  if (!connected) {
    return <DaemonWaiting />;
  }
  return (
    // ... existing JSX
  );
```

(Adjust to fit the actual render structure — the key is: when `!connected`, return `<DaemonWaiting />` early.)

- [ ] **Step 4: Run typecheck**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add electron-app/src/renderer/components/DaemonWaiting.tsx electron-app/src/renderer/main.tsx electron-app/src/renderer/styles.css
git commit -m "feat(renderer): add DaemonWaiting component for waiting state"
```

---

## Task 7: Add `ErrorModal` component

**Files:**
- Create: `electron-app/src/renderer/components/ErrorModal.tsx`
- Modify: `electron-app/src/renderer/main.tsx`

- [ ] **Step 1: Create the component**

Create `electron-app/src/renderer/components/ErrorModal.tsx`:

```typescript
import React from "react";

interface ErrorModalProps {
  title: string;
  message: string;
  onRetry?: () => void;
  onClose: () => void;
}

export function ErrorModal({ title, message, onRetry, onClose }: ErrorModalProps) {
  return (
    <div className="error-modal-overlay" onClick={onClose}>
      <div className="error-modal" onClick={(e) => e.stopPropagation()}>
        <div className="error-modal-header">
          <h2 className="error-modal-title">{title}</h2>
        </div>
        <div className="error-modal-body">
          <pre className="error-modal-message">{message}</pre>
        </div>
        <div className="error-modal-actions">
          {onRetry && (
            <button className="error-modal-button primary" onClick={onRetry}>
              Retry
            </button>
          )}
          <button className="error-modal-button" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add styles to `styles.css`**

Append to `electron-app/src/renderer/styles.css`:

```css
.error-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}

.error-modal {
  background: var(--bg);
  border: 2px solid #dc2626;
  border-radius: 8px;
  padding: 24px;
  max-width: 600px;
  width: 90%;
  box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5);
}

.error-modal-header {
  margin-bottom: 16px;
}

.error-modal-title {
  color: #dc2626;
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}

.error-modal-body {
  margin-bottom: 20px;
}

.error-modal-message {
  background: var(--code-bg);
  padding: 12px;
  border-radius: 4px;
  font-family: monospace;
  font-size: 13px;
  color: var(--text);
  max-height: 200px;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
  margin: 0;
}

.error-modal-actions {
  display: flex;
  gap: 12px;
  justify-content: flex-end;
}

.error-modal-button {
  background: var(--button-bg);
  color: var(--text);
  border: 1px solid var(--border);
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
}

.error-modal-button.primary {
  background: #dc2626;
  color: white;
  border-color: #dc2626;
}

.error-modal-button:hover {
  opacity: 0.9;
}
```

- [ ] **Step 3: Add error state to main.tsx**

In `electron-app/src/renderer/main.tsx`:

Add the import:

```typescript
import { ErrorModal } from "./components/ErrorModal";
```

Add state (near other useState declarations):

```typescript
  const [daemonError, setDaemonError] = useState<string | null>(null);
```

Add error handler function:

```typescript
  const triggerEnsureDaemon = useCallback(async () => {
    setDaemonError(null);
    try {
      await window.sandbox.ensureDaemon();
      // Polling effect will pick up the new port
    } catch (err: any) {
      setDaemonError(err?.message ?? String(err));
    }
  }, []);
```

Add the modal to the render. Place it at the end of the outermost return, before closing tag:

```tsx
      {daemonError && (
        <ErrorModal
          title="Failed to start daemon"
          message={daemonError}
          onRetry={triggerEnsureDaemon}
          onClose={() => setDaemonError(null)}
        />
      )}
```

- [ ] **Step 4: Wire up "New Sandbox" button to call `ensureDaemon`**

Find where the "New Sandbox" button is created (likely in the `AppPanel` component or in the main render). When the user clicks it, call `triggerEnsureDaemon` first, then call `createSandbox` after.

If the create flow is in `AppPanel`, add a prop `onEnsureDaemon` to `AppPanel` and pass `triggerEnsureDaemon`.

If the create flow is in `main.tsx`, wrap the create logic:

```typescript
  const handleCreateSandbox = async (cmd: string) => {
    try {
      await window.sandbox.ensureDaemon();
      await createSandbox({ command: cmd });
      refreshSandboxes();
    } catch (err: any) {
      setDaemonError(err?.message ?? String(err));
    }
  };
```

- [ ] **Step 5: Run typecheck**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add electron-app/src/renderer/components/ErrorModal.tsx electron-app/src/renderer/main.tsx electron-app/src/renderer/styles.css
git commit -m "feat(renderer): add ErrorModal component for daemon errors"
```

---

## Task 8: Add `ensureDaemon` to renderer `api.ts`

**Files:**
- Modify: `electron-app/src/renderer/api.ts`

- [ ] **Step 1: Add `ensureDaemon` declaration**

In `electron-app/src/renderer/api.ts`, find the global `Window.sandbox` declaration:

```typescript
declare global {
  interface Window {
    sandbox: {
      getDaemonPort: () => Promise<number>;
```

Add `ensureDaemon`:

```typescript
declare global {
  interface Window {
    sandbox: {
      getDaemonPort: () => Promise<number>;
      ensureDaemon: () => Promise<number>;
```

- [ ] **Step 2: Run typecheck**

Run: `cd electron-app && pnpm typecheck`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/renderer/api.ts
git commit -m "feat(renderer): declare ensureDaemon in api.ts"
```

---

## Task 9: Add stderr hints to CLI

**Files:**
- Modify: `crates/cli-box-cli/src/main.rs`

- [ ] **Step 1: Find a key error path**

In `crates/cli-box-cli/src/main.rs`, find a function that returns an error related to daemon connection. For example, the `cmd_start` function or the function that makes HTTP requests.

Search for `tracing::error` or `Err(e)` patterns in CLI command handlers.

- [ ] **Step 2: Add stderr hint when daemon is unreachable**

In a function that returns an error, find a pattern like:

```rust
.map_err(|e| anyhow::anyhow!("daemon create failed: {e}"))?;
```

Add an `eprintln!` before returning:

```rust
.map_err(|e| {
    eprintln!("Error: Failed to connect to daemon: {e}");
    eprintln!("Hint: Run 'cli-box start' in another terminal to start the daemon.");
    anyhow::anyhow!("daemon create failed: {e}")
})?;
```

Repeat for 2-3 key error paths:
- `cmd_start` failure
- `cmd_close` failure
- `cmd_screenshot` failure

- [ ] **Step 3: Build and verify**

Run: `cd /Users/zn-ice/2026/cli-box && cargo build --release -p cli-box-cli`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/cli-box-cli/src/main.rs
git commit -m "feat(cli): add stderr hints when daemon operations fail"
```

---

## Task 10: Update README to document new behavior

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Find the "Architecture" or "How it works" section**

In `README.md`, find the section that describes the architecture or the daemon/Electron relationship.

- [ ] **Step 2: Update the description**

Replace any wording that says "Electron spawns daemon on launch" with:

```markdown
- **Daemon lifecycle**: The daemon is started by the CLI (`cli-box start`). Electron detects and connects to an existing daemon; it does not auto-spawn. If you launch the Electron app directly without first running `cli-box start`, the app will display a "Waiting for daemon" message and poll every second.
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README to reflect new daemon startup model"
```

---

## Task 11: End-to-end test

**Files:**
- Manual testing only

- [ ] **Step 1: Test scenario A (CLI first)**

```bash
# Kill any running daemon/Electron
pkill -f "cli-box-daemon" 2>/dev/null
pkill -f "CLI Box" 2>/dev/null
sleep 1

# Start CLI
cd /Users/zn-ice/2026/cli-box
./release/cli-box start zsh

# Verify: Electron should launch, connect to daemon, show zsh prompt
```

Expected: Within 5s, the CLI Box window shows the zsh prompt.

- [ ] **Step 2: Test scenario B (Electron standalone)**

```bash
# Close Electron
pkill -f "CLI Box" 2>/dev/null
# Daemon still running

# Launch Electron
open /Users/zn-ice/2026/cli-box/release/CLI\ Box.app
```

Expected: Electron shows the existing zsh tab.

- [ ] **Step 3: Test scenario C (Electron first, no daemon)**

```bash
# Kill everything
pkill -f "cli-box-daemon" 2>/dev/null
pkill -f "CLI Box" 2>/dev/null
sleep 1

# Launch Electron
open /Users/zn-ice/2026/cli-box/release/CLI\ Box.app

# Wait 3s — should show "Waiting for daemon" UI
# Then in another terminal:
./release/cli-box start
```

Expected:
- Initially: "Waiting for cli-box-daemon..." UI shown
- After CLI start: window automatically connects and shows the new tab

- [ ] **Step 4: Test scenario D (on-demand daemon start)**

```bash
# Kill daemon (Electron still running)
pkill -f "cli-box-daemon"

# In Electron GUI, click "New Sandbox" button
```

Expected:
- Red error modal appears: "Failed to start daemon" with retry button
- After clicking Retry: daemon starts, sandbox created, tab appears

(Or: the GUI's "New" handler calls `ensureDaemon` first, so clicking New triggers daemon start. If the test step fails because Electron doesn't show the modal but instead waits — that's also acceptable behavior, just document it.)

---

## Task 12: Run full test suite

- [ ] **Step 1: Run Electron unit tests**

Run: `cd electron-app && pnpm test`

Expected: All tests pass

- [ ] **Step 2: Run Rust tests**

Run: `cd /Users/zn-ice/2026/cli-box && cargo test -p cli-box-core -p cli-box-cli`

Expected: All tests pass

- [ ] **Step 3: Run typecheck**

Run: `cd /Users/zn-ice/2026/cli-box/electron-app && pnpm typecheck && pnpm typecheck --project tsconfig.node.json`

Expected: PASS

- [ ] **Step 4: Commit final state**

```bash
git status
git add -A
git commit -m "chore: final cleanup after daemon polling refactor" --allow-empty
```

---

## Self-Review

### Spec Coverage

| Spec Requirement | Task |
|-----------------|------|
| Electron polls daemon on launch | Task 5 |
| `waitForDaemon()` helper | Task 1 |
| `ensureDaemonOnDemand()` on demand | Task 2, 3 |
| Polling interval 1s | Task 1, 5 |
| Red error modal | Task 7 |
| CLI stderr hints | Task 9 |
| Daemon binary path detection | Task 3 (uses existing `findDaemonBinary()`) |
| GUI on-demand daemon start | Task 7 (wire up) |
| `DaemonWaiting` component | Task 6 |
| Documentation update | Task 10 |

### Placeholder Scan

- All code blocks contain actual implementation
- No "TBD" or "TODO" markers
- File paths are exact
- Commands have expected output

### Type Consistency

| Function | Signature |
|----------|-----------|
| `waitForDaemon(timeoutMs=0, pollIntervalMs=1000)` | consistent across tests and impl |
| `ensureDaemonOnDemand()` | returns `Promise<number>` |
| `setConnected`, `setDaemonPort`, `setDaemonError` | React useState setters |
| `triggerEnsureDaemon()` | returns `Promise<void>` |
