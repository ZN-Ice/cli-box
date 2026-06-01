import { useState, useEffect, useCallback, useRef } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal, { SandboxTerminalHandle } from "./components/Terminal";
import {
  SandboxInfo,
  fetchSandboxList,
  setDaemonPort,
  getDaemonPort,
  createSandbox,
} from "./api";
import AppPanel from "./components/AppPanel";
import "./styles.css";

declare global {
  interface Window {
    sandbox: {
      getDaemonPort: () => Promise<number>;
      createTab: (sandboxId: string, kind: string, title: string) => Promise<void>;
      switchTab: (sandboxId: string) => Promise<void>;
      closeTab: (sandboxId: string) => Promise<void>;
      listTabs: () => Promise<{ id: string; kind: string; title: string }[]>;
      onSwitchTab: (callback: (sandboxId: string) => void) => void;
    };
  }
}

interface Tab {
  id: string;
  kind: string;
  title: string;
  sandbox: SandboxInfo;
}

type Theme = "dark" | "light" | "system";

function App() {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [theme, setTheme] = useState<Theme>(() => {
    return (localStorage.getItem("theme") as Theme) || "system";
  });
  const [connected, setConnected] = useState(false);
  const [showNewDialog, setShowNewDialog] = useState(false);
  const [newSandboxCmd, setNewSandboxCmd] = useState("");
  const [newSandboxMode, setNewSandboxMode] = useState<"cli" | "app">("cli");
  const refreshTimer = useRef<ReturnType<typeof setInterval>>();
  const terminalRefs = useRef<Map<string, React.RefObject<SandboxTerminalHandle>>>(new Map());

  // Apply theme
  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("dark", "light");
    if (theme === "system") {
      // Let CSS media query handle it
    } else {
      root.classList.add(theme);
    }
    localStorage.setItem("theme", theme);
  }, [theme]);

  // Initialize daemon port and load sandboxes
  useEffect(() => {
    window.sandbox.getDaemonPort().then((port) => {
      setDaemonPort(port);
      setConnected(true);
      refreshSandboxes();
    });
  }, []);

  // Listen for tab switch commands from main process
  useEffect(() => {
    window.sandbox.onSwitchTab((sandboxId) => {
      setActiveTabId(sandboxId);
    });
  }, []);

  // Poll for sandbox changes
  const refreshSandboxes = useCallback(async () => {
    try {
      const list = await fetchSandboxList();
      setTabs((prev) => {
        const existing = new Map(prev.map((t) => [t.id, t]));
        const next: Tab[] = [];
        for (const sb of list) {
          const title = sb.kind?.detail?.command || sb.id.slice(0, 8);
          const existingTab = existing.get(sb.id);
          next.push({
            id: sb.id,
            kind: sb.kind?.type || "cli",
            title,
            sandbox: sb,
          });
        }
        return next;
      });

      // Auto-select first tab if none selected
      if (!activeTabId && list.length > 0) {
        setActiveTabId(list[0].id);
      }
    } catch {
      setConnected(false);
    }
  }, [activeTabId]);

  // Periodic refresh
  useEffect(() => {
    refreshTimer.current = setInterval(refreshSandboxes, 3000);
    return () => {
      if (refreshTimer.current) clearInterval(refreshTimer.current);
    };
  }, [refreshSandboxes]);

  // Screenshot WebSocket: connect to daemon for per-tab capture
  useEffect(() => {
    if (!connected) return;
    const port = getDaemonPort();
    if (!port) return;

    const ws = new WebSocket(`ws://127.0.0.1:${port}/screenshot/ws`);

    ws.onopen = () => console.log("[screenshot-ws] connected");

    ws.onmessage = async (event) => {
      try {
        const msg = JSON.parse(event.data);
        if (msg.type === "capture_request") {
          const { sandbox_id, request_id } = msg;
          const tabRef = terminalRefs.current.get(sandbox_id);
          if (tabRef?.current) {
            try {
              const base64 = await tabRef.current.captureToPng();
              ws.send(JSON.stringify({
                type: "capture_response",
                request_id,
                sandbox_id,
                image_base64: base64,
              }));
            } catch (err) {
              ws.send(JSON.stringify({
                type: "capture_error",
                request_id,
                sandbox_id,
                error: String(err),
              }));
            }
          } else {
            ws.send(JSON.stringify({
              type: "capture_error",
              request_id,
              sandbox_id,
              error: "Terminal not found or not mounted",
            }));
          }
        }
      } catch (err) {
        console.error("[screenshot-ws] parse error:", err);
      }
    };

    ws.onclose = () => console.log("[screenshot-ws] disconnected");
    ws.onerror = (err) => console.error("[screenshot-ws] error:", err);

    return () => ws.close();
  }, [connected]);

  const handleCloseTab = useCallback(
    async (id: string) => {
      try {
        await fetch(`${getDaemonPort() ? `http://127.0.0.1:${getDaemonPort()}` : ""}/sandbox/${id}`, {
          method: "DELETE",
        });
      } catch {
        // ignore
      }
      terminalRefs.current.delete(id);
      setTabs((prev) => prev.filter((t) => t.id !== id));
      if (activeTabId === id) {
        const remaining = tabs.filter((t) => t.id !== id);
        setActiveTabId(remaining.length > 0 ? remaining[0].id : null);
      }
    },
    [activeTabId, tabs]
  );

  const handleTabClick = useCallback((id: string) => {
    setActiveTabId(id);
  }, []);

  const toggleTheme = useCallback(() => {
    setTheme((prev) => {
      if (prev === "dark") return "light";
      if (prev === "light") return "system";
      return "dark";
    });
  }, []);

  const activeTab = tabs.find((t) => t.id === activeTabId);

  return (
    <div className="main-content">
      {/* Title Bar */}
      <div className="titlebar">
        <div className="titlebar-traffic-lights" />
        <div className="titlebar-content">
          <span className="titlebar-title">System Test Sandbox</span>
        </div>
        <div className="titlebar-actions">
          <button className="theme-toggle" onClick={toggleTheme} title="Toggle theme">
            {theme === "dark" ? "◐" : theme === "light" ? "◑" : "◯"}
          </button>
        </div>
      </div>

      {/* Tab Bar */}
      <div className="tab-bar">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            className={`tab-item ${tab.id === activeTabId ? "active" : ""}`}
            onClick={() => handleTabClick(tab.id)}
          >
            <span className="tab-icon">{tab.kind === "cli" ? "▸" : "◻"}</span>
            <span>{tab.title}</span>
            <button
              className="tab-close"
              onClick={(e) => {
                e.stopPropagation();
                handleCloseTab(tab.id);
              }}
            >
              ×
            </button>
          </button>
        ))}
        <button
          className="tab-add"
          onClick={() => setShowNewDialog(true)}
          title="New sandbox"
        >
          +
        </button>
      </div>

      {/* Terminal Area */}
      {tabs.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">⌘</div>
          <div className="empty-state-text">No sandbox open</div>
          <div className="empty-state-hint">
            Run <code>sandbox start</code> in your terminal to get started
          </div>
        </div>
      ) : (
        <div className="terminal-area">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            const hiddenStyle: React.CSSProperties = isActive
              ? {}
              : {
                  position: "absolute",
                  left: "-9999px",
                  top: "-9999px",
                  width: "1200px",
                  height: "800px",
                  visibility: "hidden",
                };

            if (tab.kind === "app") {
              return (
                <div key={tab.id} className="terminal-container" style={hiddenStyle}>
                  <AppPanel sandboxId={tab.id} />
                </div>
              );
            }

            if (!terminalRefs.current.has(tab.id)) {
              terminalRefs.current.set(tab.id, { current: null } as React.RefObject<SandboxTerminalHandle>);
            }
            const tabRef = terminalRefs.current.get(tab.id)!;

            return (
              <div key={tab.id} style={{ ...hiddenStyle, display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
                <SandboxTerminal ref={tabRef} sandboxId={tab.id} ptyPid={tab.sandbox.pty_pid!} />
              </div>
            );
          })}
        </div>
      )}

      {/* Status Bar */}
      <div className="statusbar">
        <div className="statusbar-item">
          <div className={`statusbar-dot ${connected ? "" : "error"}`} />
          <span>{connected ? `Daemon :${getDaemonPort()}` : "Disconnected"}</span>
        </div>
        <div className="statusbar-item">
          <span>{tabs.length} sandbox{tabs.length !== 1 ? "es" : ""}</span>
        </div>
        {activeTab && (
          <div className="statusbar-item">
            <span>PTY PID: {activeTab.sandbox.pty_pid}</span>
          </div>
        )}
        <div className="statusbar-spacer" />
        <div className="statusbar-item">
          <span>{theme === "system" ? "Auto" : theme === "dark" ? "Dark" : "Light"}</span>
        </div>
      </div>

      {/* New Sandbox Dialog */}
      {showNewDialog && (
        <div className="dialog-overlay" onClick={() => setShowNewDialog(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-title">New Sandbox</div>
            <div className="dialog-field">
              <label>Mode:</label>
              <select
                value={newSandboxMode}
                onChange={(e) => setNewSandboxMode(e.target.value as "cli" | "app")}
              >
                <option value="cli">CLI</option>
                <option value="app">App</option>
              </select>
            </div>
            <div className="dialog-field">
              <label>{newSandboxMode === "cli" ? "Command:" : "App path:"}</label>
              <input
                type="text"
                value={newSandboxCmd}
                onChange={(e) => setNewSandboxCmd(e.target.value)}
                placeholder={newSandboxMode === "cli" ? "zsh" : "/Applications/TextEdit.app"}
                autoFocus
              />
            </div>
            <div className="dialog-actions">
              <button onClick={() => setShowNewDialog(false)}>Cancel</button>
              <button
                className="primary"
                onClick={async () => {
                  if (!newSandboxCmd.trim()) return;
                  try {
                    await createSandbox(newSandboxMode, newSandboxCmd);
                    setShowNewDialog(false);
                    setNewSandboxCmd("");
                    refreshSandboxes();
                  } catch (e) {
                    console.error("Failed to create sandbox:", e);
                  }
                }}
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
