import { app, BrowserWindow, ipcMain } from "electron";
import { join } from "path";
import { writeFileSync, unlinkSync, mkdirSync } from "fs";
import { ensureDaemon, killDaemon } from "./daemon-bridge";

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
  app.on("second-instance", () => {
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

    writeElectronJson(daemonPort);
    createWindow();
  });
}

// IPC: renderer asks for daemon port
ipcMain.handle("get-daemon-port", () => daemonPort);

// IPC: forward tab commands to renderer
ipcMain.handle("create-tab", () => {});
ipcMain.handle("switch-tab", (_event, sandboxId: string) => {
  mainWindow?.webContents.send("switch-to-tab", sandboxId);
});
ipcMain.handle("close-tab", () => {});
ipcMain.handle("list-tabs", () => []);

// IPC: window close coordination
let pendingCloseResolve: ((action: string) => void) | null = null;

ipcMain.handle("window-close-response", (_event, action: string) => {
  if (pendingCloseResolve) {
    pendingCloseResolve(action);
    pendingCloseResolve = null;
  }
});

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 600,
    minHeight: 400,
    title: "CLI Box",
    titleBarStyle: "hiddenInset",
    vibrancy: "sidebar",
    backgroundColor: "#1e1e1e",
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

  // NEW: intercept close to show confirmation dialog
  let isClosing = false;
  mainWindow.on("close", (e) => {
    if (!mainWindow || isClosing) return;
    isClosing = true;

    // Query renderer for sandbox list, then wait for user's choice
    e.preventDefault();

    mainWindow.webContents.send("window-closing");

    // Wait for renderer response via IPC, with 5s timeout fallback
    const responsePromise = new Promise<string>((resolve) => {
      pendingCloseResolve = resolve;
    });

    const timeout = new Promise<string>((resolve) => {
      setTimeout(() => resolve("close-window-only"), 5000);
    });

    Promise.race([responsePromise, timeout]).then((action) => {
      if (action === "cancel") {
        // Reset guard so user can try closing again
        isClosing = false;
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
