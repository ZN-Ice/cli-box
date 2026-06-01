# Terminal Layout Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the terminal not filling the available space in the Electron app — eliminate the white area at the bottom and the black area on the right side.

**Architecture:** Two-layer fix: (1) CSS flex chain repair with `min-height: 0` and padding removal; (2) FitAddon measurement precision by ensuring it measures the correct element. The CSS fix ensures the container has the right height, and the JS fix ensures the terminal fills that container exactly.

**Tech Stack:** TypeScript, CSS, xterm.js 6.0.0, @xterm/addon-fit 0.11.0, Electron, React 18

---

## Root Cause Analysis

**Problem 1: White area at bottom**
- `.terminal-container` uses `flex: 1` in a flex column layout
- Flex children default to `min-height: auto`, which prevents them from shrinking below content size
- When the window height changes, the flex chain doesn't propagate height changes correctly
- `fitAddon.fit()` calculates rows based on the container height, but the container height is wrong

**Problem 2: Black area on right**
- `fitAddon.fit()` measures the `.terminal-container` div (parent of the xterm element)
- But `.terminal-container .xterm` has `padding: 8px` applied via CSS
- FitAddon calculates cols based on the container width (e.g., 1200px)
- But the xterm canvas renders inside a padded area (1200 - 16 = 1184px)
- When the container width isn't an exact multiple of cell width, leftover pixels appear as black

**Problem 3: Horizontal vs Vertical scaling asymmetry**
- Width propagates correctly (100% width, no flex chain issues)
- Height depends on flex chain which breaks due to missing `min-height: 0`

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `electron-app/src/renderer/styles.css:274-289` | Modify | CSS flex chain fix + padding removal |
| `electron-app/src/renderer/components/Terminal.tsx:61-69,119-129` | Modify | FitAddon measurement precision |

---

### Task 1: CSS Flex Chain Fix

**Files:**
- Modify: `electron-app/src/renderer/styles.css:274-289`

The CSS flex chain is: `html/body/#root` (height: 100%) -> `.main-content` (flex: 1) -> `.terminal-container` (flex: 1). Without `min-height: 0`, flex children can't shrink below their content size, which breaks height propagation when the window resizes.

- [ ] **Step 1: Add `min-height: 0` to `.main-content`**

In `electron-app/src/renderer/styles.css`, change `.main-content` from:

```css
/* Main Content */
.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
```

To:

```css
/* Main Content */
.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}
```

- [ ] **Step 2: Add `min-height: 0` to `.terminal-container` and remove padding**

In the same file, change `.terminal-container` and `.terminal-container .xterm` from:

```css
.terminal-container {
  flex: 1;
  background: var(--bg-terminal);
  overflow: hidden;
}

.terminal-container .xterm {
  padding: 8px;
}
```

To:

```css
.terminal-container {
  flex: 1;
  background: var(--bg-terminal);
  overflow: hidden;
  min-height: 0;
}
```

Note: The `.terminal-container .xterm { padding: 8px; }` rule is removed entirely. This padding was causing FitAddon to miscalculate — it measures the container but the rendering surface was 16px smaller due to padding.

- [ ] **Step 3: Verify CSS compiles**

Run: `cd electron-app && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add electron-app/src/renderer/styles.css
git commit -m "fix(ui): repair CSS flex chain and remove terminal padding

- Add min-height: 0 to .main-content and .terminal-container
- Remove padding: 8px from .terminal-container .xterm
- Fixes bottom white space and right black space in terminal"
```

---

### Task 2: FitAddon Measurement Precision

**Files:**
- Modify: `electron-app/src/renderer/components/Terminal.tsx:119-129`

The current `containerRef` callback triggers `fitAddon.fit()` when the container mounts, but the fit happens asynchronously via `requestAnimationFrame`. This can race with the WebSocket connection's initial resize. Additionally, the `handleResize` listener only calls `fitAddon.fit()` without ensuring the resize message is sent after the fit completes.

- [ ] **Step 1: Extract fit-and-resize into a helper function**

In `electron-app/src/renderer/components/Terminal.tsx`, change the resize handler and container ref from:

```typescript
    const handleResize = () => {
      fitAddon.fit();
      connRef.current?.resize(term.cols, term.rows);
    };
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
```

To:

```typescript
    const doFit = () => {
      fitAddon.fit();
      connRef.current?.resize(term.cols, term.rows);
    };

    const handleResize = () => {
      doFit();
    };
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;
    fitFnRef.current = doFit;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
```

- [ ] **Step 2: Add the `fitFnRef` declaration**

At the top of the component, add the new ref alongside the existing ones. Change from:

```typescript
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);
```

To:

```typescript
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const fitFnRef = useRef<(() => void) | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);
```

- [ ] **Step 3: Update containerRef to use the helper**

Change from:

```typescript
  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitAddonRef.current?.fit());
    }
  }, []);
```

To:

```typescript
  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitFnRef.current?.());
    }
  }, []);
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd electron-app && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 5: Run unit tests**

Run: `cd electron-app && pnpm test:unit`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add electron-app/src/renderer/components/Terminal.tsx
git commit -m "fix(terminal): ensure FitAddon fit-and-resize is atomic

- Extract doFit helper that calls fit() then resize()
- Use fitFnRef so containerRef triggers the same atomic operation
- Fixes race condition between initial fit and WebSocket resize"
```

---

### Task 3: Manual Verification

**Files:**
- None (manual testing)

- [ ] **Step 1: Build and launch the Electron app**

```bash
cd electron-app && pnpm dev
```

- [ ] **Step 2: Create a sandbox and verify terminal fills the window**

Run: `sandbox start opencode` (or any CLI command)

Verify:
1. No white area at the bottom of the window
2. No black area on the right side of the terminal
3. Terminal content fills the entire available space

- [ ] **Step 3: Resize the window and verify**

Drag the window edges to resize:
1. Horizontal resize: terminal reflows correctly, no black gaps
2. Vertical resize: terminal reflows correctly, no white gaps
3. Combined resize: both directions work proportionally

- [ ] **Step 4: Verify with different terminal content**

1. Open opencode (TUI app with fixed layout)
2. Open zsh with a long command
3. Run `ls -la` to generate output
4. Verify all cases render correctly without gaps

- [ ] **Step 5: Final commit with all changes**

```bash
git status
# Verify only the two modified files are changed
git log --oneline -3
# Verify the two commits from Task 1 and Task 2
```
