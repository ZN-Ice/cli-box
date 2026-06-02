/**
 * Daemon API client for Electron renderer.
 * Connects to sandbox-daemon HTTP/WebSocket API.
 */

let _port = 15801;

export function getDaemonPort(): number {
  return _port;
}

export function setDaemonPort(port: number) {
  _port = port;
}

export function getBaseUrl(): string {
  return `http://127.0.0.1:${_port}`;
}

export interface SandboxInfo {
  id: string;
  kind: { type: string; detail: { command: string; args: string[] } };
  status: { type: string };
  pty_pid: number | null;
  port: number;
}

export async function fetchSandboxList(): Promise<SandboxInfo[]> {
  const res = await fetch(`${getBaseUrl()}/sandbox/list`);
  return res.json();
}

export async function fetchSandboxInfo(id: string): Promise<SandboxInfo | undefined> {
  const list = await fetchSandboxList();
  return list.find((sb) => sb.id === id);
}

export function connectPty(sandboxId: string, ptyPid: number): PtyConnection {
  let ws: WebSocket | null = null;
  const outputListeners: ((data: string | Uint8Array) => void)[] = [];
  let pendingResize: { cols: number; rows: number } | null = null;

  function ensureWs() {
    if (ws) return;
    ws = new WebSocket(`ws://127.0.0.1:${_port}/sandbox/${sandboxId}/pty/ws/${ptyPid}`);
    ws.binaryType = "arraybuffer";
    ws.onopen = () => {
      if (pendingResize) {
        ws!.send(JSON.stringify({ type: "resize", ...pendingResize }));
        pendingResize = null;
      }
    };
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        for (const cb of outputListeners) cb(new Uint8Array(e.data));
      } else if (typeof e.data === "string") {
        for (const cb of outputListeners) cb(e.data);
      }
    };
  }

  return {
    onOutput(cb) {
      outputListeners.push(cb);
      ensureWs();
      return () => {
        const idx = outputListeners.indexOf(cb);
        if (idx >= 0) outputListeners.splice(idx, 1);
      };
    },
    sendInput(data) {
      if (ws && ws.readyState === WebSocket.OPEN) ws.send(data);
    },
    resize(cols, rows) {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols, rows }));
      } else {
        pendingResize = { cols, rows };
      }
    },
    close() {
      ws?.close();
    },
  };
}

export async function createSandbox(
  mode: "cli" | "app",
  command: string,
  args: string[] = []
): Promise<{ sandbox_id: string; pty_pid: number | null; window_id: number | null }> {
  const res = await fetch(`${getBaseUrl()}/sandbox/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ mode, command, args }),
  });
  if (!res.ok) throw new Error(`Create failed: ${res.status}`);
  return res.json();
}

export async function takeScreenshot(sandboxId: string): Promise<Blob> {
  const res = await fetch(`${getBaseUrl()}/sandbox/${sandboxId}/screenshot`);
  if (!res.ok) throw new Error(`Screenshot failed: ${res.status}`);
  return res.blob();
}

export async function closeSandbox(sandboxId: string): Promise<void> {
  const res = await fetch(`${getBaseUrl()}/sandbox/${sandboxId}/close`, { method: "POST" });
  if (!res.ok) throw new Error(`Close failed: ${res.status}`);
}

export async function setWindowId(sandboxId: string, windowId: number): Promise<void> {
  const res = await fetch(`${getBaseUrl()}/sandbox/${sandboxId}/window`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ window_id: windowId }),
  });
  if (!res.ok) throw new Error(`Set window_id failed: ${res.status}`);
}

export interface PtyConnection {
  onOutput: (cb: (data: string | Uint8Array) => void) => () => void;
  sendInput: (data: string) => void;
  resize: (cols: number, rows: number) => void;
  close: () => void;
}
