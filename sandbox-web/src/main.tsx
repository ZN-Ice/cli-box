import { useState, useCallback, useEffect, useRef } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal from "./components/Terminal";
import { ThemeProvider, useTheme } from "./themes/ThemeContext";
import * as api from "./api";
import "./index.css";

function App() {
  const [activePid, setActivePid] = useState<number | null>(null);
  const [connected, setConnected] = useState(false);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [screenshotLoading, setScreenshotLoading] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const hasConnectedRef = useRef(false);
  const { theme, toggleTheme } = useTheme();

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

  const isDark = theme.kind === "dark";

  return (
    <div className="flex flex-col h-screen bg-sandbox-bg-primary text-sandbox-fg-primary">
      {/* Header bar — bridges native title bar and terminal */}
      <header
        className="h-8 flex items-center justify-between px-3 select-none shrink-0 border-b"
        style={{
          backgroundColor: "var(--sandbox-titlebar-bg)",
          borderColor: "var(--sandbox-border)",
        }}
      >
        {/* Left: sandbox info */}
        <div className="flex items-center gap-2 text-xs text-sandbox-fg-secondary">
          <div className="flex items-center gap-1.5">
            <span
              className="w-1.5 h-1.5 rounded-full"
              style={{ backgroundColor: connected ? "var(--sandbox-success)" : "var(--sandbox-fg-tertiary)" }}
            />
            <span>{connected ? "Connected" : "Waiting..."}</span>
          </div>
        </div>

        {/* Right: actions */}
        <div className="flex items-center gap-1">
          {/* Theme toggle */}
          <button
            onClick={toggleTheme}
            className="flex items-center justify-center w-6 h-6 rounded-md text-sandbox-fg-secondary hover:text-sandbox-fg-primary hover:bg-sandbox-bg-tertiary/50 transition-colors"
            title={`Switch to ${isDark ? "light" : "dark"} theme`}
          >
            {isDark ? (
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z" />
              </svg>
            ) : (
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z" />
              </svg>
            )}
          </button>

          {/* Screenshot button */}
          <button
            onClick={handleScreenshot}
            disabled={screenshotLoading}
            className="flex items-center gap-1 px-2 py-0.5 text-xs text-sandbox-fg-secondary hover:text-sandbox-fg-primary hover:bg-sandbox-bg-tertiary/50 rounded-md transition-colors disabled:opacity-40"
            title="Screenshot"
          >
            <svg
              className="w-3 h-3"
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
        </div>
      </header>

      {/* Terminal — fills remaining space */}
      <div className="flex-1 relative overflow-hidden">
        <SandboxTerminal
          onInput={handleTerminalInput}
          activePid={activePid}
        />

        {/* Screenshot preview — floating panel */}
        {showPreview && screenshotUrl && (
          <div className="absolute bottom-4 right-4 z-20" style={{ animation: "fadeIn 0.2s ease-out" }}>
            <div className="rounded-xl shadow-2xl shadow-black/40 overflow-hidden border"
              style={{
                backgroundColor: "var(--sandbox-bg-secondary)",
                borderColor: "var(--sandbox-border)",
              }}
            >
              {/* Preview header */}
              <div className="flex items-center justify-between px-3 py-2 border-b"
                style={{ borderColor: "var(--sandbox-border)" }}
              >
                <span className="text-xs text-sandbox-fg-secondary">Screenshot</span>
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
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <ThemeProvider>
    <App />
  </ThemeProvider>,
);
