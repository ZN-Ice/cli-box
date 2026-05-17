import { useState, useCallback } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal from "./components/Terminal";
import StatusBar from "./components/StatusBar";
import ControlPanel from "./components/ControlPanel";
import RecordControls from "./components/RecordControls";
import * as api from "./api";
import "./index.css";

interface ProcessInfo {
  pid: number;
  name: string;
  is_running: boolean;
}

type RecordStatus = "idle" | "recording" | "playing";

function App() {
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [screenshotCount, setScreenshotCount] = useState(0);
  const [serverStatus] = useState<"running" | "stopped" | "error">("running");
  const [recordStatus, setRecordStatus] = useState<RecordStatus>("idle");
  const [actionCount, setActionCount] = useState(0);
  const [screenshotLoading, setScreenshotLoading] = useState(false);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [activePid, setActivePid] = useState<number | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const showError = useCallback((msg: string) => {
    setErrorMsg(msg);
    setTimeout(() => setErrorMsg(null), 4000);
  }, []);

  // ── Terminal input → PTY ─────────────────────────────

  const handleTerminalInput = useCallback(
    (data: string) => {
      if (activePid !== null) {
        api.ptyWrite(activePid, data).catch(() => {
          // PTY write failures are expected when the process exits
        });
      }
    },
    [activePid],
  );

  // ── Screenshot ───────────────────────────────────────

  const handleScreenshot = useCallback(async () => {
    setScreenshotLoading(true);
    try {
      const url = await api.takeScreenshot();
      setScreenshotUrl(url);
      setScreenshotCount((c) => c + 1);
    } catch (e) {
      showError(`Screenshot failed: ${e}`);
    } finally {
      setScreenshotLoading(false);
    }
  }, [showError]);

  // ── Spawn App ────────────────────────────────────────

  const handleSpawnApp = useCallback(
    (path: string) => {
      api
        .spawnApp(path)
        .then((info) => {
          setProcesses((prev) => [
            ...prev,
            { pid: info.pid, name: info.name, is_running: info.is_running },
          ]);
        })
        .catch((e) => showError(`spawnApp failed: ${e}`));
    },
    [showError],
  );

  // ── Spawn CLI ────────────────────────────────────────

  const handleSpawnCli = useCallback(
    (command: string, args: string[]) => {
      api
        .spawnCli(command, args)
        .then((info) => {
          setProcesses((prev) => [
            ...prev,
            { pid: info.pid, name: info.name, is_running: info.is_running },
          ]);
          // Auto-connect terminal to this PTY
          setActivePid(info.pid);
        })
        .catch((e) => showError(`spawnCli failed: ${e}`));
    },
    [showError],
  );

  // ── Click ────────────────────────────────────────────

  const handleClick = useCallback(
    (x: number, y: number, button: string) => {
      api
        .click(x, y, button as "left" | "right" | "middle")
        .catch((e) => showError(`Click failed: ${e}`));
    },
    [showError],
  );

  // ── Type Text ────────────────────────────────────────

  const handleTypeText = useCallback(
    (text: string) => {
      api.typeText(text).catch((e) => showError(`Type failed: ${e}`));
    },
    [showError],
  );

  // ── Press Key ────────────────────────────────────────

  const handlePressKey = useCallback(
    (key: string, modifiers: string[]) => {
      api.pressKey(key, modifiers).catch((e) => showError(`Key failed: ${e}`));
    },
    [showError],
  );

  // ── Recording ────────────────────────────────────────

  const handleRecordStart = useCallback(() => {
    setRecordStatus("recording");
    setActionCount(0);
    api.recordStart().catch((e) => showError(`Record start failed: ${e}`));
  }, [showError]);

  const handleRecordStop = useCallback(() => {
    setRecordStatus("idle");
    api
      .recordStop()
      .then((r) => setActionCount(r.actions_count))
      .catch((e) => showError(`Record stop failed: ${e}`));
  }, [showError]);

  const handlePlay = useCallback((_speed: number) => {
    setRecordStatus("playing");
  }, []);

  const handlePlayStop = useCallback(() => {
    setRecordStatus("idle");
  }, []);

  return (
    <div className="flex h-screen bg-gray-900 text-white">
      {/* Main content area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header */}
        <header className="h-10 bg-gray-800 flex items-center justify-between px-4 border-b border-gray-700 flex-shrink-0">
          <span className="text-sm font-medium text-gray-300">
            System Test Sandbox
          </span>
          <span className="text-xs text-gray-500">
            macOS Desktop Automation
          </span>
        </header>

        {/* Error toast */}
        {errorMsg && (
          <div className="bg-red-900/80 text-red-200 text-xs px-4 py-1.5 text-center">
            {errorMsg}
          </div>
        )}

        {/* Content: Terminal + Screenshot / App view */}
        <div className="flex-1 flex min-h-0">
          {/* Terminal — left half */}
          <div className="w-1/2 border-r border-gray-700">
            <SandboxTerminal
              onInput={handleTerminalInput}
              connected={activePid !== null}
              activePid={activePid}
            />
          </div>

          {/* Screenshot preview / App view — right half */}
          <div className="w-1/2 flex items-center justify-center bg-gray-850">
            {screenshotUrl ? (
              <div className="w-full h-full p-2 flex flex-col">
                <div className="flex justify-between items-center mb-1">
                  <span className="text-xs text-gray-400">
                    Latest Screenshot
                  </span>
                  <button
                    className="text-xs text-gray-500 hover:text-gray-300"
                    onClick={() => setScreenshotUrl(null)}
                  >
                    Clear
                  </button>
                </div>
                <img
                  src={screenshotUrl}
                  alt="Sandbox screenshot"
                  className="flex-1 object-contain bg-black rounded"
                />
              </div>
            ) : (
              <div className="text-center text-gray-600">
                <div className="text-4xl mb-2">🖥</div>
                <p className="text-sm">Screenshot Preview</p>
                <p className="text-xs text-gray-700 mt-1">
                  Click "Screenshot" to capture
                </p>
              </div>
            )}
          </div>
        </div>

        {/* Record controls — bottom strip above status bar */}
        <RecordControls
          onRecordStart={handleRecordStart}
          onRecordStop={handleRecordStop}
          onPlay={handlePlay}
          onPlayStop={handlePlayStop}
          status={recordStatus}
          actionCount={actionCount}
        />

        {/* Status bar */}
        <StatusBar
          processes={processes}
          screenshotCount={screenshotCount}
          serverStatus={serverStatus}
          httpPort={5801}
        />
      </div>

      {/* Right sidebar — control panel */}
      <div className="w-60 flex-shrink-0">
        <ControlPanel
          onScreenshot={handleScreenshot}
          onSpawnApp={handleSpawnApp}
          onSpawnCli={handleSpawnCli}
          onClick={handleClick}
          onTypeText={handleTypeText}
          onPressKey={handlePressKey}
          screenshotLoading={screenshotLoading}
        />
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
