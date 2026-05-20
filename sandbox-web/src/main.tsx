import { useState, useCallback, useEffect, useRef } from "react";
import ReactDOM from "react-dom/client";
import Sidebar from "./components/Sidebar";
import Dashboard from "./components/Dashboard";
import DetailPanel from "./components/DetailPanel";
import { ThemeProvider } from "./themes/ThemeContext";
import * as api from "./api";
import type { ProcessInfo, HealthResponse } from "./api";
import "./index.css";

function App() {
  const [activePid, setActivePid] = useState<number | null>(null);
  const [connected, setConnected] = useState(false);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const hasConnectedRef = useRef(false);

  // Auto-connect to spawned processes
  useEffect(() => {
    const pollProcesses = async () => {
      try {
        const list = await api.listProcesses();
        setProcesses(list);
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

  // Health poll
  useEffect(() => {
    const pollHealth = async () => {
      try {
        const h = await api.health();
        setHealth(h);
      } catch {
        // silent
      }
    };

    pollHealth();
    const interval = setInterval(pollHealth, 5000);
    return () => clearInterval(interval);
  }, []);

  // Terminal input -> PTY
  const handleTerminalInput = useCallback(
    (data: string) => {
      if (activePid !== null) {
        api.ptyWrite(activePid, data).catch(() => {});
      }
    },
    [activePid],
  );

  // Screenshot
  const handleScreenshot = useCallback(async () => {
    try {
      const url = await api.takeScreenshot();
      setScreenshotUrl(url);
      setShowPreview(true);
    } catch {
      // silent
    }
  }, []);

  const closePreview = useCallback(() => {
    setShowPreview(false);
    if (screenshotUrl) {
      URL.revokeObjectURL(screenshotUrl);
      setScreenshotUrl(null);
    }
  }, [screenshotUrl]);

  const sandboxName = health?.sandbox_id ?? "Sandbox";

  return (
    <div className="three-panel">
      <Sidebar sandboxName={sandboxName} />
      <Dashboard
        sandboxName={sandboxName}
        connected={connected}
        activePid={activePid}
        onTerminalInput={handleTerminalInput}
        onScreenshot={handleScreenshot}
        processes={processes}
      >
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
      <DetailPanel health={health} connected={connected} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <ThemeProvider>
    <App />
  </ThemeProvider>,
);
