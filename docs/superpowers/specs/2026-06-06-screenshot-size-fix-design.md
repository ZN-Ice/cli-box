# Screenshot Size Fix Design

## Problem

Screenshots of different sandbox instances have inconsistent pixel sizes. For example:
- OpenCode: 1208x627 (correct, matches terminal area)
- Claude Code: 640x456 (incorrect, uses default 80x24 PTY size)

## Root Cause

`captureToPng()` in `electron-app/src/renderer/components/Terminal.tsx` has two capture paths:

- **Sub-path A** (line 33-37): Direct xterm canvas export — used when tab is visible and canvas element exists. Size determined by `fitAddon.fit()` based on actual container dimensions.
- **Sub-path B** (line 39-70): Manual offscreen canvas — used when tab is hidden/offscreen and canvas element doesn't exist. Uses `term.cols * 8` x `term.rows * 19`.

When a screenshot is requested for a hidden tab, the daemon sends `switch_tab_request` to make it active. However, `captureToPng` is called before `fitAddon.fit()` runs on the now-visible container. `term.cols` and `term.rows` remain at the initial 80x24 PTY default, producing a 640x456 canvas.

## Solution

Call `fitAddon.fit()` at the start of the fallback path (Sub-path B) to ensure `term.cols`/`term.rows` reflect the current container dimensions before rendering the canvas.

### File: `electron-app/src/renderer/components/Terminal.tsx`

In the `captureToPng` fallback path (line 39), add a `fitAddon.fit()` call before reading `term.cols`/`term.rows`:

```typescript
// Fallback: render xterm buffer to canvas (works for hidden/offscreen tabs)
const fitAddon = fitAddonRef.current;
if (fitAddon) {
  fitAddon.fit();
}
const cols = term.cols;
const rows = term.rows;
// ... rest unchanged
```

### Why this works

The daemon's screenshot flow switches the tab to active before sending `capture_request`. Once the tab is active, its container uses actual terminal-area dimensions (not the hidden 1200x800 style). Calling `fitAddon.fit()` at this point computes correct cols/rows from the real container size.

### No other changes needed

- Daemon screenshot flow: unchanged
- Tab switching logic: unchanged
- Fallback canvas rendering (charWidth/lineHeight): unchanged
- Sub-path A (direct canvas export): unchanged

## Testing

- Verify Claude Code sandbox screenshots have consistent size matching the terminal area (should be ~1208x627, same as OpenCode)
- Verify OpenCode screenshots remain unchanged
- Run existing E2E screenshot tests
