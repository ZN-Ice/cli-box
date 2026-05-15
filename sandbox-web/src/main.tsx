import { useState, useCallback } from "react";
import ReactDOM from "react-dom/client";
import SandboxTerminal from "./components/Terminal";
import StatusBar from "./components/StatusBar";
import ControlPanel from "./components/ControlPanel";
import RecordControls from "./components/RecordControls";
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

  const handleTerminalInput = useCallback((_data: string) => {
    // Terminal input is forwarded to the PTY via Tauri shell plugin
  }, []);

  const handleScreenshot = useCallback(async () => {
    setScreenshotLoading(true);
    try {
      // Invoke Tauri command or call HTTP API
      setScreenshotCount((c) => c + 1);
    } catch {
      // Silently handle screenshot failures
    } finally {
      setScreenshotLoading(false);
    }
  }, []);

  const handleSpawnApp = useCallback((path: string) => {
    setProcesses((prev) => [
      ...prev,
      {
        pid: Date.now(),
        name: path.split("/").pop() ?? path,
        is_running: true,
      },
    ]);
  }, []);

  const handleSpawnCli = useCallback((command: string, _args: string[]) => {
    setProcesses((prev) => [
      ...prev,
      { pid: Date.now(), name: command, is_running: true },
    ]);
  }, []);

  const handleClick = useCallback((_x: number, _y: number, _button: string) => {
    // Invoke Tauri or HTTP click
  }, []);

  const handleTypeText = useCallback((_text: string) => {
    // Invoke Tauri or HTTP type_text
  }, []);

  const handlePressKey = useCallback((_key: string, _modifiers: string[]) => {
    // Invoke Tauri or HTTP press_key
  }, []);

  const handleRecordStart = useCallback(() => {
    setRecordStatus("recording");
    setActionCount(0);
  }, []);

  const handleRecordStop = useCallback(() => {
    setRecordStatus("idle");
  }, []);

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

        {/* Content: Terminal + App view */}
        <div className="flex-1 flex min-h-0">
          {/* Terminal — left half */}
          <div className="w-1/2 border-r border-gray-700">
            <SandboxTerminal onInput={handleTerminalInput} connected />
          </div>

          {/* App view — right half */}
          <div className="w-1/2 flex items-center justify-center bg-gray-850">
            <div className="text-center text-gray-600">
              <div className="text-4xl mb-2">🖥</div>
              <p className="text-sm">App View Area</p>
              <p className="text-xs text-gray-700 mt-1">
                Embedded macOS app will render here
              </p>
            </div>
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
