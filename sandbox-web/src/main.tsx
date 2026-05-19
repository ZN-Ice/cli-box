import { useState, useCallback, useEffect, useRef } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal from "./components/Terminal";
import * as api from "./api";
import "./index.css";

function App() {
  const [activePid, setActivePid] = useState<number | null>(null);
  const [connected, setConnected] = useState(false);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [screenshotLoading, setScreenshotLoading] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
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

  // Terminal input → PTY
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
    setScreenshotLoading(true);
    try {
      const url = await api.takeScreenshot();
      setScreenshotUrl(url);
      setShowPreview(true);
    } catch {
      // silent
    } finally {
      setScreenshotLoading(false);
    }
  }, []);

  // Close preview
  const closePreview = useCallback(() => {
    setShowPreview(false);
    if (screenshotUrl) {
      URL.revokeObjectURL(screenshotUrl);
      setScreenshotUrl(null);
    }
  }, [screenshotUrl]);

  return (
    <div className="w-full h-screen bg-term-bg text-term-fg relative overflow-hidden">
      {/* Full-screen terminal */}
      <SandboxTerminal
        onInput={handleTerminalInput}
        activePid={activePid}
      />

      {/* Floating toolbar — top right, macOS style */}
      <div className="absolute top-3 right-4 z-10">
        <div className="flex items-center gap-1 bg-term-surface/80 backdrop-blur-md border border-term-border/50 rounded-lg px-2 py-1 shadow-lg shadow-black/20">
          {/* Screenshot button */}
          <button
            onClick={handleScreenshot}
            disabled={screenshotLoading}
            className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-term-muted hover:text-term-fg hover:bg-white/5 rounded-md transition-all duration-150 disabled:opacity-40"
            title="Screenshot"
          >
            <svg
              className="w-3.5 h-3.5"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M6.827 6.175A2.31 2.31 0 0 1 5.186 7.23c-.38.054-.757.112-1.134.175C2.999 7.58 2.25 8.507 2.25 9.574V18a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9.574c0-1.067-.75-1.994-1.802-2.169a47.865 47.865 0 0 0-1.134-.175 2.31 2.31 0 0 1-1.64-1.055l-.822-1.316a2.192 2.192 0 0 0-1.736-1.039 48.774 48.774 0 0 0-5.232 0 2.192 2.192 0 0 0-1.736 1.039l-.821 1.316Z"
              />
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M16.5 12.75a4.5 4.5 0 1 1-9 0 4.5 4.5 0 0 1 9 0ZM18.75 10.5h.008v.008h-.008V10.5Z"
              />
            </svg>
            {screenshotLoading ? (
              <span className="animate-pulse">...</span>
            ) : (
              <span>Screenshot</span>
            )}
          </button>

          {/* Divider */}
          <div className="w-px h-3.5 bg-term-border/50" />

          {/* Connection status */}
          <div className="flex items-center gap-1.5 px-2 py-1 text-xs text-term-muted">
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                connected ? "bg-green-400" : "bg-term-muted"
              }`}
            />
            <span>{connected ? "Connected" : "Waiting..."}</span>
          </div>
        </div>
      </div>

      {/* Screenshot preview — floating panel */}
      {showPreview && screenshotUrl && (
        <div className="absolute bottom-4 right-4 z-20" style={{ animation: "fadeIn 0.2s ease-out" }}>
          <div className="bg-term-surface/90 backdrop-blur-md border border-term-border/50 rounded-xl shadow-2xl shadow-black/40 overflow-hidden">
            {/* Preview header */}
            <div className="flex items-center justify-between px-3 py-2 border-b border-term-border/30">
              <span className="text-xs text-term-muted">Screenshot</span>
              <button
                onClick={closePreview}
                className="text-term-muted hover:text-term-fg transition-colors p-0.5"
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
            {/* Preview image */}
            <img
              src={screenshotUrl}
              alt="Screenshot"
              className="w-[400px] max-h-[300px] object-contain bg-black/30"
            />
          </div>
        </div>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
