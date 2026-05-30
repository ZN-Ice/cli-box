import { app, BrowserWindow, ipcMain } from "electron";
import { join } from "path";
import { writeFileSync, unlinkSync, mkdirSync } from "fs";
import { ensureDaemon, killDaemon } from "./daemon-bridge";
import * as tabManager from "./tab-manager";

const ELECTRON_JSON_PATH = join(process.env.HOME || "/tmp", ".sandbox", "electron.json");

function writeElectronJson(port: number) {
  const dir = join(process.env.HOME || "/tmp", ".sandbox");
  mkdirSync(dir, { recursive: true });
  writeFileSync(ELECTRON_JSON_PATH, JSON.stringify({ pid: process.pid, port }));
}

function removeElectronJson() {
  try { unlinkSync(ELECTRON_JSON_PATH); } catch { /* ignore */ }
}

let mainWindow: BrowserWindow | null = null;
let daemonPort: number | null = null;

const gotTheLock = app.requestSingleInstanceLock();

if (!gotTheLock) {
  app.quit();
} else {
  app.on("second-instance", async () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }

    // Poll daemon for new sandboxes and create tabs for them
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

    // Sync existing sandboxes from daemon (e.g., daemon was already running)
    if (daemonPort) {
      try {
        const resp = await fetch(`http://127.0.0.1:${daemonPort}/sandbox/list`);
        const sandboxes = await resp.json();
        for (const sb of sandboxes) {
          const title = sb.kind?.detail?.command || sb.id;
          tabManager.createTab(sb.id, sb.kind?.type || "cli", title, daemonPort);
        }
        const tabs = tabManager.getAllTabs();
        if (tabs.length > 0) {
          tabManager.switchToTab(tabs[0].id);
        }
      } catch (err) {
        console.error("Failed to sync sandboxes:", err);
      }
    }
  });
}

// IPC: renderer asks for daemon port
ipcMain.handle("get-daemon-port", () => daemonPort);

// IPC: renderer requests new tab
ipcMain.handle("create-tab", (_event, sandboxId: string, kind: string, title: string) => {
  if (!daemonPort) throw new Error("Daemon not running");
  tabManager.createTab(sandboxId, kind as "cli" | "app", title, daemonPort);
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

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    title: "System Test Sandbox",
    titleBarStyle: "hiddenInset",
    show: false,
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  tabManager.setMainWindow(mainWindow);

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
  removeElectronJson();
  killDaemon();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0 && daemonPort) {
    createWindow();
  }
});
