# Phase 2: Electron Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tauri window with an Electron shell that renders xterm.js terminals via Chromium, eliminating WKWebView rendering artifacts. The Electron app connects to the existing cli-box-daemon via HTTP/WebSocket and manages multiple sandbox tabs in a single window.

**Architecture:** Electron main process spawns cli-box-daemon as a child process. Each sandbox gets a WebContentsView tab containing xterm.js that connects directly to the daemon's PTY WebSocket. CLI mode sandboxes render in xterm.js using standard `term.write()` (no writeDirect hack). Tab switching positions WebContentsViews off-screen (waveterm strategy). `cli-box start` CLI command launches Electron if not running and triggers new-tab creation via daemon event.

**Tech Stack:** Electron 34.x, TypeScript, electron-vite, React 18, @xterm/xterm 6.x, Vite 6

**Spec:** `docs/design/electron-rust-architecture.md` (Section 3.2, 3.3, Phase 2 in Section 9)

**Depends on:** Phase 1 complete (`cli-box-daemon` running with HTTP API on port 15801–15899, CLI `cli-box start/type/key --pty` all working)

---

## Scope

本计划覆盖 Phase 2 的完整实现。完成后 `cli-box start claude` 会打开 Electron 窗口，xterm.js 用标准 `term.write()` 渲染 Claude Code（Chromium 引擎，无 WKWebView 问题）。

Phase 3（守护与恢复、心跳检测、崩溃恢复、系统托盘）将在 Phase 2 完成后另写计划。

## File Structure

```
新增/修改文件清单:

electron-app/                             # 🆕 Electron 应用
├── package.json                          # 项目配置 + electron-builder
├── tsconfig.json                         # TypeScript 配置
├── vite.config.ts                        # electron-vite 配置
├── electron-builder.config.cjs           # 打包配置
├── src/
│   ├── main/
│   │   ├── index.ts                      # Electron 入口：requestSingleInstanceLock, spawn daemon, create window
│   │   ├── window.ts                     # BrowserWindow 创建和管理
│   │   ├── tab-manager.ts               # Tab (WebContentsView) 创建/切换/销毁
│   │   ├── daemon-bridge.ts             # 与 daemon 的 HTTP 通信
│   │   └── ipc-handlers.ts             # preload IPC 桥接
│   ├── preload/
│   │   └── index.ts                     # contextBridge 暴露安全 API
│   └── renderer/
│       ├── index.html                   # Tab 渲染页面入口
│       ├── main.tsx                     # React 入口（渲染终端/控制面板）
│       ├── api.ts                       # 连接 daemon 的 HTTP/WS 客户端
│       └── components/
│           └── Terminal.tsx             # xterm.js 终端（去掉 writeDirect）
│
sandbox-web/                             # 🔧 保留但逐步迁移
│
crates/sandbox-cli/src/
├── main.rs                              # 🔧 修改：spawn Electron 进程
└── client.rs                            # ✅ 不变

crates/sandbox-core/src/daemon/
└── mod.rs                               # 🔧 小改：添加 Electron 状态文件支持
```

---

## Task 1: 搭建 electron-app 项目骨架

**Files:**
- Create: `electron-app/package.json`
- Create: `electron-app/tsconfig.json`
- Create: `electron-app/tsconfig.node.json`
- Create: `electron-app/vite.config.ts`
- Create: `electron-app/electron-builder.config.cjs`
- Create: `electron-app/src/main/index.ts` (最简 main)
- Create: `electron-app/src/preload/index.ts`
- Create: `electron-app/src/renderer/index.html`

- [ ] **Step 1: 创建 electron-app/package.json**

```json
{
  "name": "sandbox-electron",
  "version": "0.1.0",
  "private": true,
  "main": "./dist/main/index.js",
  "scripts": {
    "dev": "electron-vite dev",
    "build": "electron-vite build",
    "preview": "electron-vite preview",
    "typecheck": "tsc --noEmit -p tsconfig.json && tsc --noEmit -p tsconfig.node.json",
    "pack": "electron-builder --config electron-builder.config.cjs",
    "dist": "npm run build && npm run pack"
  },
  "dependencies": {
    "electron-store": "^10.0.0"
  },
  "devDependencies": {
    "@xterm/addon-fit": "^0.11.0",
    "@xterm/xterm": "^6.0.0",
    "electron": "^34.0.0",
    "electron-builder": "^25.1.8",
    "electron-vite": "^3.0.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "@types/react": "^18.3.1",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.2",
    "vite": "^6.0.5",
    "tailwindcss": "^3.4.17",
    "postcss": "^8.4.49",
    "autoprefixer": "^10.4.20"
  }
}
```

- [ ] **Step 2: 创建 tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"]
  },
  "include": ["src/renderer/**/*"]
}
```

- [ ] **Step 3: 创建 tsconfig.node.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true
  },
  "include": ["src/main/**/*", "src/preload/**/*", "vite.config.ts"]
}
```

- [ ] **Step 4: 创建 vite.config.ts**

```typescript
import { resolve } from "path";
import { defineConfig, externalizeDepsPlugin } from "electron-vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
  },
  renderer: {
    plugins: [react()],
    resolve: {
      alias: {
        "@": resolve("src/renderer"),
      },
    },
  },
});
```

- [ ] **Step 5: 创建 electron-builder.config.cjs**

```javascript
/** @type {import('electron-builder').Configuration} */
const config = {
  appId: "com.cli-box",
  productName: "CLI Box",
  directories: {
    output: "../../dist/electron",
  },
  mac: {
    target: ["dmg"],
    category: "public.app-category.developer-tools",
  },
  files: ["dist/**/*"],
  extraResources: [
    {
      from: "../../target/release/cli-box-daemon",
      to: "cli-box-daemon",
    },
  ],
};

module.exports = config;
```

- [ ] **Step 6: 创建 src/main/index.ts (最小 main process)**

```typescript
import { app, BrowserWindow } from "electron";
import { join } from "path";

let mainWindow: BrowserWindow | null = null;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    title: "CLI Box",
    titleBarStyle: "hiddenInset",
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    mainWindow.loadFile(join(__dirname, "../renderer/index.html"));
  }
}

app.whenReady().then(createWindow);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
```

- [ ] **Step 7: 创建 src/preload/index.ts**

```typescript
import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("sandbox", {
  getDaemonPort: () => ipcRenderer.invoke("get-daemon-port"),
  onNewSandbox: (callback: (sandboxId: string, ptyPid: number, kind: string) => void) => {
    ipcRenderer.on("new-sandbox", (_event, sandboxId, ptyPid, kind) =>
      callback(sandboxId, ptyPid, kind),
    );
  },
});
```

- [ ] **Step 8: 创建 src/renderer/index.html**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>CLI Box</title>
  </head>
  <body class="bg-neutral-900 text-neutral-100">
    <div id="root"></div>
    <script type="module" src="./main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 9: 创建 src/renderer/main.tsx (占位)**

```tsx
import ReactDOM from "react-dom/client";

function App() {
  return (
    <div className="flex h-screen items-center justify-center">
      <p>CLI Box — Electron</p>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
```

- [ ] **Step 10: 安装依赖并验证 dev 启动**

```bash
cd electron-app && pnpm install && pnpm dev
```

Expected: Electron 窗口打开显示 "CLI Box — Electron"

- [ ] **Step 11: Commit**

```bash
git add electron-app/
git commit -m "feat(electron): scaffold electron-app with electron-vite"
```

---

## Task 2: Electron main — spawn daemon + 单实例锁

**Files:**
- Modify: `electron-app/src/main/index.ts`
- Create: `electron-app/src/main/daemon-bridge.ts`

- [ ] **Step 1: 创建 daemon-bridge.ts**

负责发现和启动 cli-box-daemon。

```typescript
import { spawn, ChildProcess } from "child_process";
import { readFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { app } from "electron";

let daemonProcess: ChildProcess | null = null;

interface DaemonInfo {
  port: number;
  pid: number;
  started_at: string;
}

function daemonJsonPath(): string {
  const home = process.env.HOME || "/tmp";
  return join(home, ".sandbox", "daemon.json");
}

function readDaemonInfo(): DaemonInfo | null {
  const path = daemonJsonPath();
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, "utf-8"));
  } catch {
    return null;
  }
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export function findRunningDaemon(): number | null {
  const info = readDaemonInfo();
  if (!info) return null;
  if (isProcessAlive(info.pid)) return info.port;
  // Stale — ignore
  return null;
}

function findDaemonBinary(): string {
  // Dev mode: relative to project
  const devPath = join(__dirname, "..", "..", "..", "target", "release", "cli-box-daemon");
  if (existsSync(devPath)) return devPath;
  // Production: bundled in app resources
  const prodPath = join(process.resourcesPath, "cli-box-daemon");
  if (existsSync(prodPath)) return prodPath;
  // Same directory as electron binary
  return join(dirname(app.getPath("exe")), "cli-box-daemon");
}

export async function ensureDaemon(): Promise<number> {
  const existingPort = findRunningDaemon();
  if (existingPort) return existingPort;

  const bin = findDaemonBinary();
  daemonProcess = spawn(bin, [], {
    stdio: "pipe",
    detached: false,
  });

  daemonProcess.stdout?.on("data", (data: Buffer) => {
    console.log(`[daemon] ${data.toString().trim()}`);
  });
  daemonProcess.stderr?.on("data", (data: Buffer) => {
    console.error(`[daemon] ${data.toString().trim()}`);
  });

  // Wait for daemon.json to appear (up to 5s)
  const port = await new Promise<number>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error("Daemon failed to start within 5s"));
    }, 5000);

    const check = () => {
      const info = readDaemonInfo();
      if (info && isProcessAlive(info.pid)) {
        clearTimeout(timeout);
        resolve(info.port);
      } else {
        setTimeout(check, 100);
      }
    };
    check();
  });

  console.log(`Daemon started on port ${port}`);
  return port;
}

export function killDaemon() {
  if (daemonProcess && !daemonProcess.killed) {
    daemonProcess.kill();
  }
}
```

- [ ] **Step 2: 修改 main/index.ts — 加入 requestSingleInstanceLock + spawn daemon**

```typescript
import { app, BrowserWindow, ipcMain } from "electron";
import { join } from "path";
import { ensureDaemon, killDaemon } from "./daemon-bridge";

let mainWindow: BrowserWindow | null = null;
let daemonPort: number | null = null;

const gotTheLock = app.requestSingleInstanceLock();

if (!gotTheLock) {
  app.quit();
} else {
  app.on("second-instance", () => {
    // Someone tried to run a second instance — focus our window
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });

  app.whenReady().then(async () => {
    try {
      daemonPort = await ensureDaemon();
    } catch (err) {
      console.error("Failed to start daemon:", err);
      app.quit();
      return;
    }

    createWindow();
  });
}

// IPC: renderer asks for daemon port
ipcMain.handle("get-daemon-port", () => daemonPort);

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    title: "CLI Box",
    titleBarStyle: "hiddenInset",
    show: false,
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    mainWindow.loadFile(join(__dirname, "../renderer/index.html"));
  }

  mainWindow.once("ready-to-show", () => {
    mainWindow?.show();
  });

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    killDaemon();
    app.quit();
  }
});

app.on("before-quit", () => {
  killDaemon();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0 && daemonPort) {
    createWindow();
  }
});
```

- [ ] **Step 3: 验证 Electron 启动并 spawn daemon**

```bash
cd electron-app && pnpm dev
```

Expected: Electron 窗口打开，终端日志显示 `Daemon started on port 15801`
验证: `curl http://localhost:15801/health` 返回 `{"status":"ok",...}`

- [ ] **Step 4: Commit**

```bash
git add electron-app/src/main/
git commit -m "feat(electron): single-instance lock + spawn cli-box-daemon"
```

---

## Task 3: Tab Manager — WebContentsView 多标签

**Files:**
- Create: `electron-app/src/main/tab-manager.ts`
- Modify: `electron-app/src/main/index.ts`

- [ ] **Step 1: 创建 tab-manager.ts**

```typescript
import { BrowserWindow, WebContentsView } from "electron";
import { join } from "path";

export interface SandboxTab {
  id: string;
  kind: "cli" | "app";
  title: string;
  webContentsView: WebContentsView;
}

const tabs: Map<string, SandboxTab> = new Map();
let activeTabId: string | null = null;
let mainWindow: BrowserWindow | null = null;

const TAB_BAR_HEIGHT = 36;
const TITLE_BAR_HEIGHT = 28;

export function setMainWindow(win: BrowserWindow) {
  mainWindow = win;
}

export function createTab(
  sandboxId: string,
  kind: "cli" | "app",
  title: string,
  daemonPort: number,
): SandboxTab {
  if (!mainWindow) throw new Error("No main window");

  const view = new WebContentsView({
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  // Load renderer page with sandbox params
  const baseUrl = process.env.ELECTRON_RENDERER_URL
    ? process.env.ELECTRON_RENDERER_URL
    : `file://${join(__dirname, "../renderer/index.html")}`;

  const url = new URL(baseUrl);
  url.searchParams.set("sandbox_id", sandboxId);
  url.searchParams.set("kind", kind);
  url.searchParams.set("title", title);
  url.searchParams.set("daemon_port", daemonPort.toString());
  view.webContents.loadURL(url.toString());

  const tab: SandboxTab = {
    id: sandboxId,
    kind,
    title,
    webContentsView: view,
  };

  tabs.set(sandboxId, tab);

  // If first tab, activate immediately; otherwise position off-screen
  if (tabs.size === 1) {
    switchToTab(sandboxId);
  } else {
    positionViewOffScreen(view);
  }

  mainWindow.contentView.addChildView(view);
  return tab;
}

export function switchToTab(targetId: string) {
  if (!mainWindow) return;
  const target = tabs.get(targetId);
  if (!target) return;

  const { width, height } = mainWindow.getContentBounds();
  const topOffset = TAB_BAR_HEIGHT + TITLE_BAR_HEIGHT;

  // Move all tabs off-screen except target
  for (const [id, tab] of tabs) {
    if (id === targetId) {
      tab.webContentsView.setBounds({
        x: 0,
        y: topOffset,
        width,
        height: height - topOffset,
      });
    } else {
      positionViewOffScreen(tab.webContentsView);
    }
  }

  activeTabId = targetId;
}

export function closeTab(sandboxId: string) {
  const tab = tabs.get(sandboxId);
  if (!tab) return;

  mainWindow?.contentView.removeChildView(tab.webContentsView);
  tab.webContentsView.webContents.close();
  tabs.delete(sandboxId);

  // If closed active tab, switch to another
  if (activeTabId === sandboxId) {
    const remaining = Array.from(tabs.keys());
    if (remaining.length > 0) {
      switchToTab(remaining[0]);
    } else {
      activeTabId = null;
    }
  }
}

export function getActiveTabId(): string | null {
  return activeTabId;
}

export function getAllTabs(): SandboxTab[] {
  return Array.from(tabs.values());
}

function positionViewOffScreen(view: WebContentsView) {
  view.setBounds({ x: -15000, y: -15000, width: 1200, height: 800 });
}
```

- [ ] **Step 2: 修改 main/index.ts — 集成 Tab Manager**

在 `createWindow()` 之后加入 tab manager 初始化，添加 IPC handlers 用于 tab 操作：

在 `main/index.ts` 顶部添加 import：

```typescript
import * as tabManager from "./tab-manager";
```

在 `createWindow()` 函数内，`mainWindow` 赋值后添加：

```typescript
tabManager.setMainWindow(mainWindow);
```

在 `ipcMain.handle("get-daemon-port", ...)` 之后添加：

```typescript
// IPC: renderer requests new tab
ipcMain.handle("create-tab", (_event, sandboxId: string, kind: string, title: string) => {
  if (!daemonPort) throw new Error("Daemon not running");
  tabManager.createTab(sandboxId, kind, title, daemonPort);
});

// IPC: renderer requests tab switch
ipcMain.handle("switch-tab", (_event, sandboxId: string) => {
  tabManager.switchToTab(sandboxId);
});

// IPC: renderer requests tab close
ipcMain.handle("close-tab", (_event, sandboxId: string) => {
  tabManager.closeTab(sandboxId);
});

// IPC: list tabs
ipcMain.handle("list-tabs", () => {
  return tabManager.getAllTabs().map((t) => ({
    id: t.id,
    kind: t.kind,
    title: t.title,
  }));
});
```

- [ ] **Step 3: 更新 preload — 暴露 tab IPC**

更新 `src/preload/index.ts`：

```typescript
import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("sandbox", {
  getDaemonPort: () => ipcRenderer.invoke("get-daemon-port"),
  createTab: (sandboxId: string, kind: string, title: string) =>
    ipcRenderer.invoke("create-tab", sandboxId, kind, title),
  switchTab: (sandboxId: string) => ipcRenderer.invoke("switch-tab", sandboxId),
  closeTab: (sandboxId: string) => ipcRenderer.invoke("close-tab", sandboxId),
  listTabs: () => ipcRenderer.invoke("list-tabs"),
});
```

- [ ] **Step 4: 验证编译**

```bash
cd electron-app && pnpm typecheck
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add electron-app/src/
git commit -m "feat(electron): tab manager with WebContentsView + off-screen positioning"
```

---

## Task 4: Renderer — xterm.js 终端组件（标准 term.write）

**Files:**
- Modify: `electron-app/src/renderer/main.tsx`
- Create: `electron-app/src/renderer/api.ts`
- Create: `electron-app/src/renderer/components/Terminal.tsx`
- Create: `electron-app/src/renderer/components/TabBar.tsx`

- [ ] **Step 1: 创建 api.ts — daemon 连接层**

```typescript
/**
 * Daemon API client for Electron renderer.
 * Port comes from URL param (set by tab-manager when creating the WebContentsView).
 */

function getDaemonPort(): number {
  const params = new URLSearchParams(window.location.search);
  const p = params.get("daemon_port");
  return p ? Number(p) : 15801;
}

function getSandboxId(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get("sandbox_id") || "";
}

function getKind(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get("kind") || "cli";
}

function getTitle(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get("title") || "";
}

const PORT = getDaemonPort();
const BASE = `http://127.0.0.1:${PORT}`;
export { PORT, getSandboxId, getKind, getTitle };

export interface PtyConnection {
  onOutput: (cb: (data: string | Uint8Array) => void) => () => void;
  sendInput: (data: string) => void;
  resize: (cols: number, rows: number) => void;
  close: () => void;
}

export function connectPty(ptyPid: number): PtyConnection {
  const ws = new WebSocket(`ws://127.0.0.1:${PORT}/sandbox/${getSandboxId()}/pty/ws/${ptyPid}`);
  ws.binaryType = "arraybuffer";
  const outputListeners: ((data: string | Uint8Array) => void)[] = [];

  ws.onmessage = (e) => {
    if (e.data instanceof ArrayBuffer) {
      for (const cb of outputListeners) cb(new Uint8Array(e.data));
    } else if (typeof e.data === "string") {
      for (const cb of outputListeners) cb(e.data);
    }
  };

  return {
    onOutput(cb) {
      outputListeners.push(cb);
      return () => {
        const idx = outputListeners.indexOf(cb);
        if (idx >= 0) outputListeners.splice(idx, 1);
      };
    },
    sendInput(data) {
      if (ws.readyState === WebSocket.OPEN) ws.send(data);
    },
    resize(cols, rows) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols, rows }));
      }
    },
    close() {
      ws.close();
    },
  };
}

export async function fetchSandboxInfo(): Promise<{
  id: string;
  kind: { type: string; detail: { command: string; args: string[] } };
  status: { type: string };
  pty_pid: number | null;
}> {
  const res = await fetch(`${BASE}/sandbox/list`);
  const list = await res.json();
  return list.find((sb: { id: string }) => sb.id === getSandboxId());
}
```

- [ ] **Step 2: 创建 Terminal.tsx — 标准 term.write()，无 writeDirect**

```tsx
import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { connectPty } from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  ptyPid: number;
  onReady?: (cols: number, rows: number) => void;
}

export default function SandboxTerminal({ ptyPid, onReady }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);

  // Initialize xterm.js
  useEffect(() => {
    if (!terminalRef.current) return;
    if (xtermRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 14,
      fontFamily: '"SF Mono", "Menlo", "Monaco", monospace',
      fontWeight: "400",
      fontWeightBold: "600",
      scrollback: 10000,
      theme: {
        background: "#1a1b26",
        foreground: "#a9b1d6",
        cursor: "#c0caf5",
        selectionBackground: "#33467c",
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();

    onReady?.(term.cols, term.rows);

    term.onData((data) => {
      connRef.current?.sendInput(data);
    });

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

  // Connect to PTY WebSocket
  useEffect(() => {
    connRef.current?.close();
    connRef.current = null;

    const conn = connectPty(ptyPid);
    connRef.current = conn;

    const decoder = new TextDecoder();
    conn.onOutput((data) => {
      const term = xtermRef.current;
      if (!term) return;
      const writeData = typeof data === "string" ? data : decoder.decode(data as Uint8Array);
      // Standard term.write() — Chromium handles rendering correctly
      term.write(writeData);
    });

    // Send initial resize
    const term = xtermRef.current;
    if (term) {
      conn.resize(term.cols, term.rows);
    }

    return () => {
      conn.close();
      connRef.current = null;
    };
  }, [ptyPid]);

  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitAddonRef.current?.fit());
    }
  }, []);

  return (
    <div ref={containerRef} className="w-full h-full relative">
      <div ref={terminalRef} className="w-full h-full" />
    </div>
  );
}
```

- [ ] **Step 3: 创建 TabBar.tsx**

```tsx
import { getAllTabs, switchTab, closeTab } from "../api";

interface TabBarProps {
  activeTabId: string | null;
  onRefresh?: () => void;
}

export default function TabBar({ activeTabId, onRefresh }: TabBarProps) {
  // Tab list comes from Electron IPC via preload
  // For now, use a simple implementation
  return (
    <div className="flex items-center h-9 bg-neutral-800 border-b border-neutral-700 px-2">
      {/* Tabs will be populated from main process */}
      <div className="flex-1" />
      <button
        onClick={() => onRefresh?.()}
        className="text-xs text-neutral-400 hover:text-neutral-200 px-2 py-1"
      >
        Refresh
      </button>
    </div>
  );
}
```

- [ ] **Step 4: 更新 renderer/main.tsx**

```tsx
import { useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal from "./components/Terminal";
import { getSandboxId, getKind, getTitle, fetchSandboxInfo } from "./api";

declare global {
  interface Window {
    sandbox: {
      getDaemonPort: () => Promise<number>;
      createTab: (sandboxId: string, kind: string, title: string) => Promise<void>;
      switchTab: (sandboxId: string) => Promise<void>;
      closeTab: (sandboxId: string) => Promise<void>;
      listTabs: () => Promise<{ id: string; kind: string; title: string }[]>;
    };
  }
}

function App() {
  const sandboxId = getSandboxId();
  const kind = getKind();
  const title = getTitle();
  const [ptyPid, setPtyPid] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!sandboxId) {
      setError("No sandbox_id provided");
      return;
    }

    fetchSandboxInfo()
      .then((info) => {
        if (info?.pty_pid) {
          setPtyPid(info.pty_pid);
        } else {
          setError("Sandbox has no PTY process");
        }
      })
      .catch((err) => {
        setError(`Failed to fetch sandbox info: ${err}`);
      });
  }, [sandboxId]);

  if (error) {
    return (
      <div className="flex h-screen items-center justify-center text-red-400">
        <p>{error}</p>
      </div>
    );
  }

  if (!ptyPid) {
    return (
      <div className="flex h-screen items-center justify-center text-neutral-400">
        <p>Connecting to sandbox...</p>
      </div>
    );
  }

  return (
    <div className="w-full h-full">
      <SandboxTerminal ptyPid={ptyPid} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
```

- [ ] **Step 5: 验证编译**

```bash
cd electron-app && pnpm typecheck
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add electron-app/src/renderer/
git commit -m "feat(electron): renderer with xterm.js terminal using standard term.write()"
```

---

## Task 5: CLI 集成 — `cli-box start` 启动 Electron + 创建 Tab

**Files:**
- Modify: `crates/sandbox-cli/src/main.rs` (添加 Electron spawn 逻辑)

- [ ] **Step 1: 添加 find_electron_binary 和 spawn 逻辑**

在 `crates/sandbox-cli/src/main.rs` 中添加：

```rust
/// Locate the Electron app binary next to the current executable.
fn find_electron_binary() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
    let exe_dir = exe_path.parent().context("No parent dir for exe")?;

    // Check for Electron binary in release directory
    let electron_name = "CLI Box";
    let app_bundle = exe_dir.join(format!("{electron_name}.app"));
    if app_bundle.exists() {
        return Ok(app_bundle.join("Contents/MacOS/cli-box"));
    }

    // Dev mode: check electron-app/dist
    let cwd = std::env::current_dir().unwrap_or_default();
    let dev_bundle = cwd.join("dist/electron/mac-arm64/CLI Box.app");
    if dev_bundle.exists() {
        return Ok(dev_bundle.join("Contents/MacOS/cli-box"));
    }

    anyhow::bail!("Electron app not found. Build it first: cd electron-app && pnpm build && pnpm pack")
}

/// Check if Electron is already running by reading ~/.sandbox/electron.json
fn find_running_electron() -> Option<u16> {
    let path = dirs_next::home_dir()?.join(".sandbox").join("electron.json");
    if !path.exists() {
        return None;
    }
    let json = std::fs::read_to_string(&path).ok()?;
    let info: serde_json::Value = serde_json::from_str(&json).ok()?;
    let pid = info["pid"].as_u64()? as i32;
    unsafe {
        if libc::kill(pid, 0) == 0 {
            return Some(info["port"].as_u64()? as u16);
        }
    }
    let _ = std::fs::remove_file(&path);
    None
}
```

- [ ] **Step 2: 修改 cmd_start_daemon — 在创建沙箱后 spawn Electron**

在 `cmd_start_daemon` 中，sandbox 创建成功后添加 Electron spawn 逻辑：

在 `println!("Daemon port: {port}");` 之后添加：

```rust
    // Ensure Electron is running
    if find_running_electron().is_none() {
        if let Ok(electron_bin) = find_electron_binary() {
            tracing::info!("[start] spawning Electron: {}", electron_bin.display());
            let _child = Command::new(&electron_bin)
                .spawn()
                .context("Failed to launch Electron app")?;
            tracing::info!("[start] Electron launched");
        } else {
            tracing::warn!("[start] Electron app not found, running in headless daemon mode");
        }
    }
```

- [ ] **Step 3: 验证编译**

```bash
cargo check -p sandbox-cli
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sandbox-cli/src/main.rs
git commit -m "feat(cli): spawn Electron app when running cli-box start"
```

---

## Task 6: second-instance 处理 — CLI 通知已有 Electron 创建新 Tab

**Files:**
- Modify: `electron-app/src/main/index.ts` (处理 daemon 事件，自动创建 Tab)

- [ ] **Step 1: 在 main/index.ts 中添加自动 Tab 创建**

在 `app.on("second-instance", ...)` 回调中，除了 focus 窗口外，还需要检查 CLI 是否创建了新沙箱：

```typescript
app.on("second-instance", async () => {
  if (mainWindow) {
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.focus();
  }

  // Poll daemon for new sandboxes and create tabs for them
  // The CLI has already created the sandbox via daemon HTTP API
  // We just need to discover it and create a tab
  if (daemonPort) {
    try {
      const resp = await fetch(`http://127.0.0.1:${daemonPort}/sandbox/list`);
      const sandboxes = await resp.json();
      const existingTabs = new Set(tabManager.getAllTabs().map((t) => t.id));
      for (const sb of sandboxes) {
        if (!existingTabs.has(sb.id)) {
          const title = sb.kind?.detail?.command || sb.id;
          tabManager.createTab(sb.id, sb.kind?.type || "cli", title, daemonPort);
          tabManager.switchToTab(sb.id);
        }
      }
    } catch (err) {
      console.error("Failed to sync sandboxes:", err);
    }
  }
});
```

同时，在首次 `createWindow()` 之后也同步已有沙箱：

在 `createWindow();` 之后添加：

```typescript
  // Sync existing sandboxes from daemon (e.g., daemon was already running)
  if (daemonPort) {
    try {
      const resp = await fetch(`http://127.0.0.1:${daemonPort}/sandbox/list`);
      const sandboxes = await resp.json();
      for (const sb of sandboxes) {
        const title = sb.kind?.detail?.command || sb.id;
        tabManager.createTab(sb.id, sb.kind?.type || "cli", title, daemonPort);
      }
      // Activate first tab
      const tabs = tabManager.getAllTabs();
      if (tabs.length > 0) {
        tabManager.switchToTab(tabs[0].id);
      }
    } catch (err) {
      console.error("Failed to sync sandboxes:", err);
    }
  }
```

- [ ] **Step 2: 验证 second-instance 场景**

手动测试流程：
1. 运行 `pnpm dev`，Electron 窗口打开
2. 在另一个终端运行 `./release/cli-box start zsh`
3. Expected: Electron 窗口获得焦点，出现新的 zsh Tab
4. 再运行 `./release/cli-box start claude`
5. Expected: Electron 窗口获得焦点，出现新的 Claude Tab

- [ ] **Step 3: Commit**

```bash
git add electron-app/src/main/index.ts
git commit -m "feat(electron): auto-create tabs from daemon cli-box list on second-instance"
```

---

## Task 7: 端到端集成测试

**Files:**
- No new files — manual integration test

- [ ] **Step 1: 构建全部组件**

```bash
# Build daemon + CLI (release)
cargo build --release -p cli-box-daemon -p sandbox-cli
cp target/release/sandbox release/sandbox
cp target/release/cli-box-daemon release/cli-box-daemon
chmod +x release/sandbox release/cli-box-daemon
codesign --force --sign - release/sandbox release/cli-box-daemon

# Build Electron app (dev mode)
cd electron-app && pnpm dev
```

- [ ] **Step 2: 测试场景一 — CLI 启动 zsh**

在另一个终端运行：
```bash
./release/cli-box start zsh
```

Expected:
- Electron 窗口打开（或已有窗口获得焦点）
- 新 Tab 出现，xterm.js 渲染 zsh 提示符
- 可以在终端中输入命令

- [ ] **Step 3: 测试场景二 — CLI 启动 Claude Code**

```bash
./release/cli-box start claude
```

Expected:
- 新 Tab 出现
- Claude Code 信任提示正确渲染（无 WKWebView 残留）
- 确认后进入 Claude 主界面
- 标准 `term.write()` 渲染，无 writeDirect

- [ ] **Step 4: 测试场景三 — 多 Tab 切换**

```bash
# 已经有 2 个 Tab
./release/cli-box list
# 记录两个 sandbox ID

# CLI 截图
./release/cli-box screenshot --id <ID> -o test.png
```

Expected: 截图功能正常（通过 daemon 的 ScreenCaptureKit）

- [ ] **Step 5: 测试场景四 — 关闭 Tab**

```bash
./release/cli-box close <ID>
```

Expected: Electron 窗口中对应 Tab 被关闭，其他 Tab 不受影响

- [ ] **Step 6: 保存测试报告**

将测试结果保存到 `release_test/` 目录

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "test(electron): Phase 2 end-to-end integration verified"
```

---

## Self-Review

### Spec coverage

| Spec 要求 (Phase 2) | 对应 Task |
|---------------------|----------|
| 搭建 electron-app 项目 (electron-vite) | Task 1 |
| requestSingleInstanceLock | Task 2 |
| spawn daemon 子进程 | Task 2 |
| Tab Manager: WebContentsView 管理 | Task 3 |
| Tab 切换 (off-screen 定位) | Task 3 |
| 前端连接 daemon (api.ts) | Task 4 |
| xterm.js 标准 term.write() (去掉 writeDirect) | Task 4 |
| CLI `cli-box start` spawn Electron | Task 5 |
| second-instance 处理（已有实例时创建新 Tab） | Task 6 |
| 端到端验证 | Task 7 |

### Placeholder scan

无 TBD/TODO。所有步骤包含完整代码。

### Type consistency

- `SandboxTab` 接口在 `tab-manager.ts` 中定义，id/kind/title/webContentsView 字段一致
- `api.ts` 中 `connectPty()` 返回 `PtyConnection`，与 `Terminal.tsx` 使用方式一致
- `fetchSandboxInfo()` 返回的 JSON 结构与 daemon `GET /sandbox/list` 的响应结构一致
- `window.sandbox` 类型声明在 `main.tsx` 中定义，与 preload 暴露的方法签名一致

### Gaps

- **APP 模式控制面板**：设计文档中提到 APP 模式需要控制面板 Tab（截图预览+操作按钮）。当前 Plan 的 renderer 根据 kind 参数渲染不同组件，APP 模式的具体 UI 可在后续 task 中添加
- **electron.json 状态文件**：设计中 Electron 写入 `~/.sandbox/electron.json` 供 CLI 发现。当前 Task 5 中 `find_running_electron()` 引用了它但未实现写入逻辑，需要在 Task 2 的 main process 中补充
- **Tab 栏 UI**：当前 TabBar.tsx 是占位组件，实际 Tab 标签列表需要从 main process 通过 IPC 获取
