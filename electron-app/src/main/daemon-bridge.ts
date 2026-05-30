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
  return join(home, ".sandbox", "daemon.json");
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

function findDaemonBinary(): string {
  // Dev mode: relative to project
  const devPath = join(__dirname, "..", "..", "..", "target", "release", "sandbox-daemon");
  if (existsSync(devPath)) return devPath;
  // Production: bundled in app resources
  const prodPath = join(process.resourcesPath, "sandbox-daemon");
  if (existsSync(prodPath)) return prodPath;
  // Same directory as electron binary
  return join(dirname(app.getPath("exe")), "sandbox-daemon");
}

export async function ensureDaemon(): Promise<number> {
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
