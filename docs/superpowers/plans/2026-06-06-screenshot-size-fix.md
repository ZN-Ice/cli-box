# Screenshot Size Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix inconsistent screenshot sizes by ensuring FitAddon re-fits before fallback canvas rendering.

**Architecture:** Add a `fitAddon.fit()` call at the start of the `captureToPng` fallback path so that `term.cols`/`term.rows` reflect the actual container dimensions when the canvas is rendered.

**Tech Stack:** TypeScript, React, xterm.js

---

### Task 1: Add fitAddon.fit() to captureToPng fallback path

**Files:**
- Modify: `electron-app/src/renderer/components/Terminal.tsx:39`

- [ ] **Step 1: Add fitAddon.fit() call before reading cols/rows**

In `electron-app/src/renderer/components/Terminal.tsx`, replace line 39-41:

```typescript
      // Fallback: render xterm buffer to canvas (works for hidden/offscreen tabs)
      const cols = term.cols;
      const rows = term.rows;
```

With:

```typescript
      // Fallback: render xterm buffer to canvas (works for hidden/offscreen tabs)
      const fitAddon = fitAddonRef.current;
      if (fitAddon) {
        fitAddon.fit();
      }
      const cols = term.cols;
      const rows = term.rows;
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd electron-app && pnpm typecheck`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/renderer/components/Terminal.tsx
git commit -m "fix(capture): re-fit terminal before fallback screenshot rendering"
```
