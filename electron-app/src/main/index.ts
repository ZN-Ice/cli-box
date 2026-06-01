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

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 600,
    minHeight: 400,
    title: "System Test Sandbox",
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
