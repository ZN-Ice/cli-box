import { debugLog, debugError } from "./logger";

/**
 * Sandbox HTTP API client.
 *
 * Port resolution order:
 *   1. `?sandbox_port=<N>` in the page URL
 *   2. `SANDOX_PORT` env var (Vite-injected at build time)
 *   3. Default `5801`
 */

function getPort(): number {
  if (typeof window !== "undefined") {
    const params = new URLSearchParams(window.location.search);
    const p = params.get("sandbox_port");
    if (p) return Number(p);
  }
  return 5801;
}

const BASE = () => `http://127.0.0.1:${getPort()}`;

// ── Types ──────────────────────────────────────────────

export interface ProcessInfo {
  pid: number;
  name: string;
  path: string | null;
  is_running: boolean;
}

export interface HealthResponse {
  status: string;
  version: string;
  uptime_secs: number;
  sandbox_id: string | null;
}

export interface SandboxInfo {
  sandbox_id: string | null;
  window_id: number | null;
  uptime_secs: number;
}

// ── Generic fetch helper ───────────────────────────────

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE()}${path}`, {
    ...options,
    headers: { "Content-Type": "application/json", ...options?.headers },
  });
  if (!res.ok) {
    const body = await res.text();
    let msg = body;
    try {
      msg = JSON.parse(body).error ?? body;
    } catch {
      /* keep raw text */
    }
    throw new Error(`HTTP ${res.status}: ${msg}`);
  }
  // Some endpoints return binary (image/png), caller handles raw response
  return res as unknown as T;
}

// ── Health & Info ──────────────────────────────────────

export async function health(): Promise<HealthResponse> {
  const res = await fetch(`${BASE()}/health`);
  return res.json();
}

export async function sandboxInfo(): Promise<SandboxInfo> {
  const res = await fetch(`${BASE()}/sandbox/info`);
  return res.json();
}

// ── Pending CLI ──────────────────────────────────────

export interface PendingCli {
  command: string | null;
  args?: string[];
}

export async function getPendingCli(): Promise<PendingCli> {
  const res = await fetch(`${BASE()}/sandbox/pending-cli`);
  if (!res.ok) return { command: null };
  return res.json();
}

// ── Screenshot ─────────────────────────────────────────

/** Capture the sandbox window. Returns a Blob URL. */
export async function takeScreenshot(): Promise<string> {
  const res = await fetch(`${BASE()}/screenshot`);
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`Screenshot failed: ${body}`);
  }
  const blob = await res.blob();
  return URL.createObjectURL(blob);
}

/** Capture a screen region. Returns a Blob URL. */
export async function takeScreenshotRegion(
  x: number,
  y: number,
  width: number,
  height: number,
): Promise<string> {
  const res = await fetch(
    `${BASE()}/screenshot/region?x=${x}&y=${y}&width=${width}&height=${height}`,
  );
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`Screenshot region failed: ${body}`);
  }
  const blob = await res.blob();
  return URL.createObjectURL(blob);
}

// ── Input ──────────────────────────────────────────────

export async function click(
  x: number,
  y: number,
  button: "left" | "right" | "middle" = "left",
): Promise<void> {
  await request("/input/click", {
    method: "POST",
    body: JSON.stringify({ x, y, button }),
  });
}

export async function typeText(text: string): Promise<void> {
  await request("/input/type", {
    method: "POST",
    body: JSON.stringify({ text }),
  });
}

export async function pressKey(
  key: string,
  modifiers: string[] = [],
): Promise<void> {
  await request("/input/key", {
    method: "POST",
    body: JSON.stringify({ key, modifiers }),
  });
}

export async function scroll(
  x: number,
  y: number,
  direction: string,
  amount: number,
): Promise<void> {
  await request("/input/scroll", {
    method: "POST",
    body: JSON.stringify({ x, y, direction, amount }),
  });
}

export async function drag(
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
): Promise<void> {
  await request("/input/drag", {
    method: "POST",
    body: JSON.stringify({
      from_x: fromX,
      from_y: fromY,
      to_x: toX,
      to_y: toY,
    }),
  });
}

// ── Process ────────────────────────────────────────────

export async function spawnApp(path: string): Promise<ProcessInfo> {
  const res = await fetch(`${BASE()}/app/spawn`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path }),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`spawnApp failed: ${body}`);
  }
  return res.json();
}

export async function spawnCli(
  command: string,
  args: string[],
  cols?: number,
  rows?: number,
): Promise<ProcessInfo> {
  const body: Record<string, unknown> = { command, args };
  if (cols !== undefined) body.cols = cols;
  if (rows !== undefined) body.rows = rows;
  const res = await fetch(`${BASE()}/cli/spawn`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`spawnCli failed: ${text}`);
  }
  return res.json();
}

export async function listProcesses(): Promise<ProcessInfo[]> {
  const res = await fetch(`${BASE()}/processes`);
  return res.json();
}

export async function killProcess(pid: number): Promise<void> {
  await request("/process/kill", {
    method: "POST",
    body: JSON.stringify({ pid }),
  });
}

// ── PTY WebSocket ──────────────────────────────────────

function wsBaseUrl(): string {
  const port = getPort();
  return `ws://127.0.0.1:${port}`;
}

export interface PtyWsConnection {
  ws: WebSocket;
  onOutput: (cb: (data: string | Uint8Array) => void) => () => void;
  onError: (cb: (msg: string) => void) => () => void;
  onClose: (cb: (code: number, reason: string) => void) => () => void;
  sendInput: (data: string) => void;
  resize: (cols: number, rows: number) => void;
  close: () => void;
}

export function ptyConnectWs(pid: number): PtyWsConnection {
  const ws = new WebSocket(`${wsBaseUrl()}/pty/ws/${pid}`);
  ws.binaryType = "arraybuffer";
  const outputListeners: ((data: string | Uint8Array) => void)[] = [];
  const errorListeners: ((msg: string) => void)[] = [];
  const closeListeners: ((code: number, reason: string) => void)[] = [];

  ws.onopen = () => {
    debugLog(`frontend: connected to /pty/ws/${pid}`);
  };
  ws.onclose = (e) => {
    debugLog(`frontend: connection closed, code=${e.code}, reason=${e.reason}`);
    for (const cb of closeListeners) cb(e.code, e.reason);
  };
  ws.onerror = () => {
    const msg = `WebSocket connection to PTY ${pid} failed`;
    debugError(`frontend: ${msg}`);
    for (const cb of errorListeners) cb(msg);
  };
  ws.onmessage = (e) => {
    if (e.data instanceof ArrayBuffer) {
      const u8 = new Uint8Array(e.data);
      debugLog(`frontend: received binary message, len=${u8.length}`);
      for (const cb of outputListeners) cb(u8);
    } else if (typeof e.data === "string") {
      const preview = e.data.length > 80 ? e.data.substring(0, 80) : e.data;
      debugLog(`frontend: received text message, len=${e.data.length}, preview=${JSON.stringify(preview)}`);
      for (const cb of outputListeners) cb(e.data);
    }
  };

  return {
    ws,
    onOutput(cb) {
      outputListeners.push(cb);
      return () => {
        const idx = outputListeners.indexOf(cb);
        if (idx >= 0) outputListeners.splice(idx, 1);
      };
    },
    onError(cb) {
      errorListeners.push(cb);
      return () => {
        const idx = errorListeners.indexOf(cb);
        if (idx >= 0) errorListeners.splice(idx, 1);
      };
    },
    onClose(cb) {
      closeListeners.push(cb);
      return () => {
        const idx = closeListeners.indexOf(cb);
        if (idx >= 0) closeListeners.splice(idx, 1);
      };
    },
    sendInput(data) {
      if (ws.readyState === WebSocket.OPEN) ws.send(data);
    },
    resize(cols, rows) {
      if (ws.readyState === WebSocket.OPEN)
        ws.send(JSON.stringify({ type: "resize", cols, rows }));
    },
    close() {
      ws.close();
    },
  };
}

// ── Windows ────────────────────────────────────────────

export async function listWindows(): Promise<[number, string][]> {
  const res = await fetch(`${BASE()}/windows`);
  return res.json();
}
