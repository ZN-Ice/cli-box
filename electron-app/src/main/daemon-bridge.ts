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
  return join(home, ".cli-box", "daemon.json");
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
  return null;
}

/**
 * Poll for an existing daemon without spawning one.
 * Returns the port once daemon.json is found, or throws on timeout.
 *
 * @param timeoutMs - 0 means poll forever (default), >0 means timeout after N ms
 * @param pollIntervalMs - polling interval in ms (default 1000ms = 1s)
 */
export async function waitForDaemon(
  timeoutMs: number = 0,
  pollIntervalMs: number = 1000
): Promise<number> {
  const start = Date.now();
  while (true) {
    const port = findRunningDaemon();
    if (port) return port;
    if (timeoutMs > 0 && Date.now() - start > timeoutMs) {
      throw new Error(`Daemon not available within ${timeoutMs}ms`);
    }
    await new Promise((r) => setTimeout(r, pollIntervalMs));
  }
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

/**
 * Spawn daemon subprocess on demand.
 * Use this when user explicitly requests daemon (e.g., creates sandbox from GUI
 * while daemon is not running). Do NOT call this on app launch — use
 * waitForDaemon() instead to poll for existing daemon.
 *
 * @returns The daemon port number
 * @throws If daemon binary not found or fails to start within timeout
 */
export async function ensureDaemonOnDemand(): Promise<number> {
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
