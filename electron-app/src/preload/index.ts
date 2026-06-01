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
});
