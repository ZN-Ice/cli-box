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
): Promise<ProcessInfo> {
  const res = await fetch(`${BASE()}/cli/spawn`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args }),
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`spawnCli failed: ${body}`);
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

// ── PTY ────────────────────────────────────────────────

export async function ptyWrite(pid: number, data: string): Promise<void> {
  await request("/pty/write", {
    method: "POST",
    body: JSON.stringify({ pid, data }),
  });
}

export async function ptyRead(pid: number): Promise<{ output: string | null }> {
  const res = await fetch(`${BASE()}/pty/output/${pid}`);
  return res.json();
}

// ── Windows ────────────────────────────────────────────

export async function listWindows(): Promise<[number, string][]> {
  const res = await fetch(`${BASE()}/windows`);
  return res.json();
}

// ── Recording ──────────────────────────────────────────

export async function recordStart(): Promise<void> {
  await request("/record/start", { method: "POST", body: "{}" });
}

export async function recordStop(): Promise<{ actions_count: number }> {
  const res = await fetch(`${BASE()}/record/stop`, {
    method: "POST",
    body: "{}",
  });
  return res.json();
}
