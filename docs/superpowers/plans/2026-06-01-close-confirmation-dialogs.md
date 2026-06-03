# Close Confirmation Dialogs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add confirmation dialogs when closing tabs or windows to prevent orphaned cli-box processes.

**Architecture:** Two dialog flows — tab close (renderer-only, checks sandbox status) and window close (main↔renderer IPC coordination). All changes are in the Electron frontend; no daemon/Rust changes needed.

**Tech Stack:** Electron (main + renderer), React 18, TypeScript, CSS

---

## File Structure

| File | Responsibility |
|------|---------------|
| `electron-app/src/preload/index.ts` | Expose `onWindowClosing` and `sendCloseResponse` IPC bridge methods |
| `electron-app/src/main/index.ts` | Intercept window `close` event, send `window-closing` to renderer, handle `window-close-response` |
| `electron-app/src/renderer/main.tsx` | Add `CloseConfirmDialog` and `WindowCloseDialog` components, modify `handleCloseTab`, add `onWindowClosing` listener |
| `electron-app/src/renderer/styles.css` | Add danger button variant for "Close All Terminals" |

---

### Task 1: Add IPC bridge methods for window close coordination

**Files:**
- Modify: `electron-app/src/preload/index.ts`
- Modify: `electron-app/src/renderer/main.tsx` (Window type declaration only)

- [ ] **Step 1: Add `onWindowClosing` and `sendCloseResponse` to preload bridge**

In `electron-app/src/preload/index.ts`, add two new methods to the `contextBridge.exposeInMainWorld` call:

```typescript
import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("sandbox", {
  getDaemonPort: () => ipcRenderer.invoke("get-daemon-port"),
  createTab: (sandboxId: string, kind: string, title: string) =>
    ipcRenderer.invoke("create-tab", sandboxId, kind, title),
  switchTab: (sandboxId: string) => ipcRenderer.invoke("switch-tab", sandboxId),
  closeTab: (sandboxId: string) => ipcRenderer.invoke("close-tab", sandboxId),
  listTabs: () => ipcRenderer.invoke("list-tabs"),
  onSwitchTab: (callback: (sandboxId: string) => void) => {
    ipcRenderer.on("switch-to-tab", (_event, sandboxId) => callback(sandboxId));
  },
  // NEW: window close coordination
  onWindowClosing: (callback: (sandboxIds: string[]) => void) => {
    ipcRenderer.on("window-closing", (_event, sandboxIds) => callback(sandboxIds));
  },
  sendCloseResponse: (action: "cancel" | "close-window-only" | "close-all") =>
    ipcRenderer.invoke("window-close-response", action),
});
```

- [ ] **Step 2: Update the `Window` type declaration in renderer**

In `electron-app/src/renderer/main.tsx`, update the `Window` interface (lines 14-25) to include the new methods:

```typescript
declare global {
  interface Window {
    sandbox: {
      getDaemonPort: () => Promise<number>;
      createTab: (sandboxId: string, kind: string, title: string) => Promise<void>;
      switchTab: (sandboxId: string) => Promise<void>;
      closeTab: (sandboxId: string) => Promise<void>;
      listTabs: () => Promise<{ id: string; kind: string; title: string }[]>;
      onSwitchTab: (callback: (sandboxId: string) => void) => void;
      onWindowClosing: (callback: (sandboxIds: string[]) => void) => void;
      sendCloseResponse: (action: "cancel" | "close-window-only" | "close-all") => Promise<void>;
    };
  }
}
```

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/preload/index.ts electron-app/src/renderer/main.tsx
git commit -m "feat(electron): add IPC bridge for window close coordination"
```

---

### Task 2: Add main process window close handler

**Files:**
- Modify: `electron-app/src/main/index.ts`

- [ ] **Step 1: Add IPC handler for `window-close-response`**

In `electron-app/src/main/index.ts`, after the existing IPC handlers (line 56), add:

```typescript
// IPC: window close coordination
let pendingCloseResolve: ((action: string) => void) | null = null;

ipcMain.handle("window-close-response", (_event, action: string) => {
  if (pendingCloseResolve) {
    pendingCloseResolve(action);
    pendingCloseResolve = null;
  }
});
```

- [ ] **Step 2: Add `close` event handler on BrowserWindow**

In the `createWindow` function, after the `mainWindow.on("closed", ...)` handler (line 86-88), add a `close` handler that intercepts the close event:

```typescript
mainWindow.on("closed", () => {
  mainWindow = null;
});

// NEW: intercept close to show confirmation dialog
mainWindow.on("close", (e) => {
  if (!mainWindow) return;

  // Query renderer for cli-box list, then wait for user's choice
  e.preventDefault();

  mainWindow.webContents.send("window-closing");

  // Wait for renderer response via IPC
  const responsePromise = new Promise<string>((resolve) => {
    pendingCloseResolve = resolve;
  });

  responsePromise.then((action) => {
    if (action === "cancel") {
      // Do nothing, window stays open
      return;
    }

    if (action === "close-window-only") {
      // Remove this handler to avoid infinite loop, then close
      mainWindow?.removeAllListeners("close");
      mainWindow?.close();
      return;
    }

    if (action === "close-all") {
      // Renderer will have already closed all sandboxes before sending this
      mainWindow?.removeAllListeners("close");
      mainWindow?.close();
    }
  });
});
```

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/main/index.ts
git commit -m "feat(electron): intercept window close with confirmation flow"
```

---

### Task 3: Add close confirmation dialogs to renderer

**Files:**
- Modify: `electron-app/src/renderer/main.tsx`
- Modify: `electron-app/src/renderer/styles.css`

- [ ] **Step 1: Add dialog state variables**

In the `App` function, after the existing state declarations (around line 45), add:

```typescript
// Close confirmation state
const [closeConfirmTabId, setCloseConfirmTabId] = useState<string | null>(null);
const [showWindowCloseDialog, setShowWindowCloseDialog] = useState(false);
```

- [ ] **Step 2: Modify `handleCloseTab` to check sandbox status**

Replace the existing `handleCloseTab` (lines 167-184) with:

```typescript
const handleCloseTab = useCallback(
  (id: string) => {
    const tab = tabs.find((t) => t.id === id);
    if (tab && tab.sandbox.status?.type === "Running") {
      // Show confirmation dialog
      setCloseConfirmTabId(id);
      return;
    }
    // Not running, close directly
    doCloseTab(id);
  },
  [tabs]
);

const doCloseTab = useCallback(
  async (id: string) => {
    try {
      await fetch(`${getDaemonPort() ? `http://127.0.0.1:${getDaemonPort()}` : ""}/sandbox/${id}`, {
        method: "DELETE",
      });
    } catch {
      // ignore
    }
    terminalRefs.current.delete(id);
    setTabs((prev) => prev.filter((t) => t.id !== id));
    if (activeTabId === id) {
      const remaining = tabs.filter((t) => t.id !== id);
      setActiveTabId(remaining.length > 0 ? remaining[0].id : null);
    }
  },
  [activeTabId, tabs]
);
```

- [ ] **Step 3: Add `onWindowClosing` listener**

After the existing `onSwitchTab` useEffect (around line 75), add. Use a ref to avoid re-registering the IPC listener on every `tabs` change:

```typescript
// Ref to access latest tabs in IPC callback without re-registering listener
const tabsRef = useRef(tabs);
tabsRef.current = tabs;

// Listen for window close request from main process (register once)
useEffect(() => {
  window.sandbox.onWindowClosing(() => {
    if (tabsRef.current.length === 0) {
      // No sandboxes, close directly
      window.sandbox.sendCloseResponse("close-window-only");
    } else {
      setShowWindowCloseDialog(true);
    }
  });
}, []);
```

- [ ] **Step 4: Add `CloseConfirmDialog` JSX**

After the existing "New Sandbox Dialog" JSX block (around line 357), before the closing `</div>` of `.main-content`, add:

```tsx
{/* Close Tab Confirmation Dialog */}
{closeConfirmTabId && (
  <div className="dialog-overlay" onClick={() => setCloseConfirmTabId(null)}>
    <div className="dialog" onClick={(e) => e.stopPropagation()}>
      <div className="dialog-title">Close Terminal</div>
      <div className="dialog-message">
        This terminal is still running. Are you sure you want to close it?
      </div>
      <div className="dialog-actions">
        <button onClick={() => setCloseConfirmTabId(null)}>Cancel</button>
        <button
          className="danger"
          onClick={() => {
            doCloseTab(closeConfirmTabId);
            setCloseConfirmTabId(null);
          }}
        >
          Close
        </button>
      </div>
    </div>
  </div>
)}
```

- [ ] **Step 5: Add `WindowCloseDialog` JSX**

Immediately after the `CloseConfirmDialog` block:

```tsx
{/* Window Close Dialog */}
{showWindowCloseDialog && (
  <div className="dialog-overlay">
    <div className="dialog" onClick={(e) => e.stopPropagation()}>
      <div className="dialog-title">Close Window</div>
      <div className="dialog-message">
        {tabs.length} terminal{tabs.length !== 1 ? "s" : ""} running. What would you like to do?
      </div>
      <div className="dialog-actions">
        <button onClick={() => {
          setShowWindowCloseDialog(false);
          window.sandbox.sendCloseResponse("cancel");
        }}>
          Cancel
        </button>
        <button onClick={() => {
          setShowWindowCloseDialog(false);
          window.sandbox.sendCloseResponse("close-window-only");
        }}>
          Close Window Only
        </button>
        <button
          className="danger"
          onClick={async () => {
            // Close all sandboxes first
            for (const tab of tabs) {
              try {
                await fetch(`http://127.0.0.1:${getDaemonPort()}/sandbox/${tab.id}`, {
                  method: "DELETE",
                });
              } catch {
                // ignore
              }
            }
            setShowWindowCloseDialog(false);
            window.sandbox.sendCloseResponse("close-all");
          }}
        >
          Close All Terminals
        </button>
      </div>
    </div>
  </div>
)}
```

- [ ] **Step 6: Add danger button and dialog-message styles**

In `electron-app/src/renderer/styles.css`, after the `.dialog-actions button.primary` rule (around line 493), add:

```css
.dialog-actions button.danger {
  background: var(--error);
  color: white;
  border-color: var(--error);
}

.dialog-actions button.danger:hover {
  opacity: 0.9;
}

.dialog-message {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 16px;
  line-height: 1.5;
}
```

- [ ] **Step 7: Commit**

```bash
git add electron-app/src/renderer/main.tsx electron-app/src/renderer/styles.css
git commit -m "feat(ui): add close confirmation dialogs for tabs and windows"
```

---

### Task 4: Verify and push

- [ ] **Step 1: Run typecheck and lint**

```bash
cd electron-app && pnpm typecheck && pnpm format:check
```

Expected: No errors.

- [ ] **Step 2: Run build**

```bash
cd electron-app && pnpm build
```

Expected: Build succeeds.

- [ ] **Step 3: Push and create PR**

```bash
git push -u origin feat/close-confirmation-dialogs
gh pr create --title "feat: add close confirmation dialogs for tabs and windows" --body "$(cat <<'EOF'
## Summary
- Tab close: if sandbox is Running, show confirmation dialog before closing
- Window close: show 3-option dialog (Cancel / Close Window Only / Close All Terminals)
- Prevents orphaned cli-box processes when closing the Electron app

## Test plan
- [ ] Open a sandbox tab, click × → confirmation dialog appears
- [ ] Click Cancel → tab stays open
- [ ] Click Close → tab and sandbox are closed
- [ ] Open multiple sandboxes, close the window → 3-option dialog appears
- [ ] Click Cancel → window stays open
- [ ] Click Close Window Only → window closes, `cli-box list` shows sandboxes still running
- [ ] Click Close All Terminals → window closes, `cli-box list` shows no sandboxes
- [ ] Close window with no sandboxes open → window closes immediately (no dialog)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
