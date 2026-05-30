import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("sandbox", {
  getDaemonPort: () => ipcRenderer.invoke("get-daemon-port"),
  onNewSandbox: (callback: (sandboxId: string, ptyPid: number, kind: string) => void) => {
    ipcRenderer.on("new-sandbox", (_event, sandboxId, ptyPid, kind) =>
      callback(sandboxId, ptyPid, kind),
    );
  },
});
