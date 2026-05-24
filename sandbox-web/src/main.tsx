import { useState, useCallback, useEffect, useRef } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import Sidebar from "./components/Sidebar";
import Dashboard from "./components/Dashboard";
import { ThemeProvider } from "./themes/ThemeContext";
import * as api from "./api";
import "./index.css";

const isMac =
  typeof navigator !== "undefined" && navigator.platform.startsWith("Mac");

function App() {
  const [activePid, setActivePid] = useState<number | null>(null);
  const [connected, setConnected] = useState(false);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const [screenshotError, setScreenshotError] = useState<string | null>(null);
  const [command, setCommand] = useState("Sandbox");
  const hasConnectedRef = useRef(false);

  // Auto-connect to spawned processes
  useEffect(() => {
    const pollProcesses = async () => {
      try {
        const list = await api.listProcesses();
        if (list.length > 0) {
          setConnected(true);
          if (activePid === null && !hasConnectedRef.current) {
            const running = list.find((p) => p.is_running);
            if (running) {
              setActivePid(running.pid);
              hasConnectedRef.current = true;
            }
          }
        } else {
          setConnected(false);
        }
      } catch {
        setConnected(false);
      }
    };

    pollProcesses();
    const interval = setInterval(pollProcesses, 2000);
    return () => clearInterval(interval);
  }, [activePid]);

  // Fetch command from Tauri sandbox config
  useEffect(() => {
    invoke<{
      command?: string;
      mode?: string;
      args?: string[];
    }>("get_sandbox_config")
      .then((config) => {
        if (config.command) {
          const args =
            config.args && config.args.length > 0
              ? " " + config.args.join(" ")
              : "";
          setCommand(config.command + args);
        }
      })
      .catch(() => {});
  }, []);

  // Spawn pending CLI with correct terminal size
  const handleSpawnReady = useCallback(async (cols: number, rows: number) => {
    try {
      const pending = await api.getPendingCli();
      if (pending.command) {
        console.log(`[App] spawning pending CLI: ${pending.command} (${cols}x${rows})`);
        await api.spawnCli(pending.command, pending.args || [], cols, rows);
      }
    } catch (err) {
      console.error("[App] failed to spawn pending CLI:", err);
    }
  }, []);

  // Screenshot
  const handleScreenshot = useCallback(async () => {
    setScreenshotError(null);
    try {
      const url = await api.takeScreenshot();
      setScreenshotUrl(url);
      setShowPreview(true);
    } catch (err) {
      setScreenshotError(
        err instanceof Error ? err.message : "Screenshot failed",
      );
      setTimeout(() => setScreenshotError(null), 4000);
    }
  }, []);

  const closePreview = useCallback(() => {
    setShowPreview(false);
    if (screenshotUrl) {
      URL.revokeObjectURL(screenshotUrl);
      setScreenshotUrl(null);
    }
  }, [screenshotUrl]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-sandbox-bg-primary text-sandbox-fg-primary">
      {/* macOS drag region — reserves space for traffic light buttons */}
      {isMac && (
        <div
          className="fixed top-0 left-0 right-0 z-50 h-7"
          data-tauri-drag-region
          style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
        />
      )}

      <Sidebar command={command} />
      <Dashboard
        command={command}
        connected={connected}
        activePid={activePid}
        onSpawnReady={handleSpawnReady}
        onScreenshot={handleScreenshot}
      >
        {/* Screenshot error toast */}
        {screenshotError && (
          <div
            className="absolute bottom-4 right-4 z-30 animate-[fadeIn_0.2s_ease-out]"
            style={{
              backgroundColor: "var(--sandbox-bg-secondary)",
              color: "var(--sandbox-fg-primary)",
              borderColor: "var(--sandbox-border)",
            }}
          >
            <div className="flex items-center gap-2 px-4 py-2.5 rounded-xl border shadow-lg text-xs">
              <svg
                className="w-4 h-4 shrink-0"
                style={{ color: "var(--sandbox-error, #f7768e)" }}
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z"
                />
              </svg>
              <span>{screenshotError}</span>
            </div>
          </div>
        )}

        {/* Screenshot preview floating panel */}
        {showPreview && screenshotUrl && (
          <div
            className="absolute bottom-4 right-4 z-20"
            style={{ animation: "fadeIn 0.2s ease-out" }}
          >
            <div
              className="rounded-xl shadow-2xl shadow-black/40 overflow-hidden border"
              style={{
                backgroundColor: "var(--sandbox-bg-secondary)",
                borderColor: "var(--sandbox-border)",
              }}
            >
              <div
                className="flex items-center justify-between px-3 py-2 border-b"
                style={{ borderColor: "var(--sandbox-border)" }}
              >
                <span className="text-xs text-sandbox-fg-secondary">
                  Screenshot
                </span>
                <button
                  onClick={closePreview}
                  className="text-sandbox-fg-secondary hover:text-sandbox-fg-primary transition-colors p-0.5"
                >
                  <svg
                    className="w-3.5 h-3.5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={2}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M6 18 18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>
              <img
                src={screenshotUrl}
                alt="Screenshot"
                className="w-[400px] max-h-[300px] object-contain bg-black/30"
              />
            </div>
          </div>
        )}
      </Dashboard>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <ThemeProvider>
    <App />
  </ThemeProvider>,
);
