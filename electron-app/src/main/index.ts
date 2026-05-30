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
    title: "System Test Sandbox",
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
