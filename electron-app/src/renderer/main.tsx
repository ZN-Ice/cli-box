import { useState, useEffect, useCallback, useRef } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal, { SandboxTerminalHandle } from "./components/Terminal";
import {
  fetchSandboxList,
  setDaemonPort,
  getDaemonPort,
  createSandbox,
  closeSandbox,
} from "./api";
import { Tab, syncTabs, selectAfterClose } from "./tabState";
import AppPanel from "./components/AppPanel";
import { DaemonWaiting } from "./components/DaemonWaiting";
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
      onWindowClosing: (callback: () => void) => void;
      sendCloseResponse: (action: "cancel" | "close-window-only" | "close-all") => Promise<void>;
    };
  }
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
  // Close confirmation state
  const [closeConfirmTabId, setCloseConfirmTabId] = useState<string | null>(null);
  const [showWindowCloseDialog, setShowWindowCloseDialog] = useState(false);
  const refreshTimer = useRef<ReturnType<typeof setInterval>>();
  const terminalRefs = useRef<Map<string, React.RefObject<SandboxTerminalHandle>>>(new Map());
  const screenshotWsRef = useRef<WebSocket | null>(null);

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

  // Poll for daemon port every 1s until daemon is available
  useEffect(() => {
    let cancelled = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;

    function poll() {
      if (cancelled) return;
      window.sandbox
        .getDaemonPort()
        .then((port) => {
          if (cancelled) return;
          if (port && port > 0) {
            // Daemon is up
            if (port !== getDaemonPort()) {
              setDaemonPort(port);
              setConnected(true);
              refreshSandboxes();
            }
            // Connected — no need to poll
          } else {
            // Daemon not running yet
            setConnected(false);
            pollTimer = setTimeout(poll, 1000);
          }
        })
        .catch(() => {
          if (cancelled) return;
          setConnected(false);
          pollTimer = setTimeout(poll, 1000);
        });
    }

    poll();
    return () => {
      cancelled = true;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, []);

  // Listen for tab switch commands from main process
  useEffect(() => {
    window.sandbox.onSwitchTab((sandboxId) => {
      setActiveTabId(sandboxId);
    });
  }, []);

  // Ref to access latest tabs in IPC callback without re-registering listener
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  // Listen for window close request from main process (register once)
  useEffect(() => {
    window.sandbox.onWindowClosing(() => {
      if (tabsRef.current.length === 0) {
        // No sandboxes, close directly
        window.sandbox.sendCloseResponse("close-window-only");
      } else {
        setShowWindowCloseDialog(true);
      }
    });
  }, []);

  // Poll for sandbox changes
  const refreshSandboxes = useCallback(async () => {
    try {
      const list = await fetchSandboxList();
      const { tabs: nextTabs } = syncTabs(tabsRef.current, list);
      setTabs(nextTabs);
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

  // Screenshot WebSocket: connect to daemon for per-tab capture (with reconnection)
  useEffect(() => {
    if (!connected) return;
    let port = getDaemonPort();
    if (!port) return;

    let ws: WebSocket | null = null;
    let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
    let reconnectDelay = 1000;
    const MAX_RECONNECT_DELAY = 30000;
    let unmounted = false;

    function connect() {
      if (unmounted) return;

      ws = new WebSocket(`ws://127.0.0.1:${port}/screenshot/ws`);
      screenshotWsRef.current = ws;

      ws.onopen = () => {
        console.log("[screenshot-ws] connected");
        reconnectDelay = 1000; // Reset backoff on successful connection
        // Notify daemon that existing terminals are ready
        for (const tab of tabsRef.current) {
          const ref = terminalRefs.current.get(tab.id);
          if (ref?.current) {
            ws?.send(JSON.stringify({
              type: "terminal_ready",
              sandbox_id: tab.id,
            }));
          }
        }
        // Periodically notify daemon about ready terminals (handles newly created tabs)
        const readyInterval = setInterval(() => {
          if (ws?.readyState !== WebSocket.OPEN) return;
          for (const tab of tabsRef.current) {
            const ref = terminalRefs.current.get(tab.id);
            if (ref?.current) {
              ws.send(JSON.stringify({
                type: "terminal_ready",
                sandbox_id: tab.id,
              }));
            }
          }
        }, 2000);
        (ws as any)._readyInterval = readyInterval;
      };

      ws.onmessage = async (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.type === "switch_tab_request") {
            const { sandbox_id, request_id } = msg;
            // Switch tab via IPC so main process repositions WebContentsView
            try {
              await window.sandbox.switchTab(sandbox_id);
            } catch {
              // fallback: update React state only
              setActiveTabId(sandbox_id);
            }
            // Small delay for tab to render
            await new Promise((r) => setTimeout(r, 200));
            ws?.send(JSON.stringify({
              type: "switch_tab_response",
              request_id,
              sandbox_id,
            }));
          } else if (msg.type === "capture_request") {
            const { sandbox_id, request_id } = msg;
            const tabRef = terminalRefs.current.get(sandbox_id);
            if (tabRef?.current) {
              try {
                const base64 = await tabRef.current.captureToPng();
                ws?.send(JSON.stringify({
                  type: "capture_response",
                  request_id,
                  sandbox_id,
                  image_base64: base64,
                }));
              } catch (err) {
                ws?.send(JSON.stringify({
                  type: "capture_error",
                  request_id,
                  sandbox_id,
                  error: String(err),
                }));
              }
            } else {
              ws?.send(JSON.stringify({
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

      ws.onclose = () => {
        console.log("[screenshot-ws] disconnected");
        if ((ws as any)._readyInterval) clearInterval((ws as any)._readyInterval);
        if (!unmounted) {
          console.log(`[screenshot-ws] reconnecting in ${reconnectDelay}ms...`);
          reconnectTimeout = setTimeout(async () => {
            // Check if daemon port changed (e.g., daemon restarted)
            try {
              const newPort = await window.sandbox.getDaemonPort();
              if (newPort && newPort !== port) {
                console.log(`[screenshot-ws] daemon port changed: ${port} → ${newPort}`);
                setDaemonPort(newPort);
                port = newPort;
              }
            } catch {
              // IPC failed, keep current port
            }
            reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
            connect();
          }, reconnectDelay);
        }
      };

      ws.onerror = (err) => {
        console.error("[screenshot-ws] error:", err);
      };
    }

    connect();

    return () => {
      unmounted = true;
      screenshotWsRef.current = null;
      if ((ws as any)._readyInterval) clearInterval((ws as any)._readyInterval);
      if (reconnectTimeout) clearTimeout(reconnectTimeout);
      if (ws) ws.close();
    };
  }, [connected]);

  const handleCloseTab = useCallback(
    (id: string) => {
      const tab = tabs.find((t) => t.id === id);
      if (tab && tab.sandbox.status?.type === "Running") {
        // Show confirmation dialog
        setCloseConfirmTabId(id);
        return;
      }
      // Not running, close directly
      doCloseTab(id);
    },
    [tabs]
  );

  const doCloseTab = useCallback(
    async (id: string) => {
      try {
        await closeSandbox(id);
      } catch {
        // ignore — sandbox may already be gone
      }
      terminalRefs.current.delete(id);
      setTabs((prev) => {
        const next = prev.filter((t) => t.id !== id);
        setActiveTabId(selectAfterClose(prev, id, activeTabId));
        return next;
      });
    },
    [activeTabId]
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

  if (!connected) {
    return <DaemonWaiting />;
  }

  return (
    <div className="main-content">
      {/* Title Bar */}
      <div className="titlebar">
        <div className="titlebar-traffic-lights" />
        <div className="titlebar-content">
          <span className="titlebar-title">CLI Box</span>
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
          title="New CLI Box"
        >
          +
        </button>
      </div>

      {/* Terminal Area */}
      {tabs.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">⌘</div>
          <div className="empty-state-text">No CLI Box open</div>
          <div className="empty-state-hint">
            Run <code>cli-box start</code> in your terminal to get started
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
                <SandboxTerminal
                  ref={tabRef}
                  sandboxId={tab.id}
                  ptyPid={tab.sandbox.pty_pid!}
                  onReady={() => {
                    const ws = screenshotWsRef.current;
                    if (ws?.readyState === WebSocket.OPEN) {
                      ws.send(JSON.stringify({
                        type: "terminal_ready",
                        sandbox_id: tab.id,
                      }));
                    }
                  }}
                />
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
          <span>{tabs.length} CLI Box{tabs.length !== 1 ? "es" : ""}</span>
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

      {/* Close Tab Confirmation Dialog */}
      {closeConfirmTabId && (
        <div className="dialog-overlay">
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-title">Close Terminal</div>
            <div className="dialog-message">
              This terminal is still running. Are you sure you want to close it?
            </div>
            <div className="dialog-actions">
              <button onClick={() => setCloseConfirmTabId(null)}>Cancel</button>
              <button
                className="danger"
                onClick={() => {
                  doCloseTab(closeConfirmTabId);
                  setCloseConfirmTabId(null);
                }}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Window Close Dialog */}
      {showWindowCloseDialog && (
        <div className="dialog-overlay">
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-title">Close Window</div>
            <div className="dialog-message">
              {tabs.length} terminal{tabs.length !== 1 ? "s" : ""} running. What would you like to do?
            </div>
            <div className="dialog-actions">
              <button onClick={() => {
                setShowWindowCloseDialog(false);
                window.sandbox.sendCloseResponse("cancel");
              }}>
                Cancel
              </button>
              <button onClick={() => {
                setShowWindowCloseDialog(false);
                window.sandbox.sendCloseResponse("close-window-only");
              }}>
                Close Window Only
              </button>
              <button
                className="danger"
                onClick={async () => {
                  for (const tab of tabs) {
                    try {
                      await closeSandbox(tab.id);
                    } catch {
                      // ignore — sandbox may already be gone
                    }
                  }
                  setShowWindowCloseDialog(false);
                  window.sandbox.sendCloseResponse("close-all");
                }}
              >
                Close All Terminals
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
