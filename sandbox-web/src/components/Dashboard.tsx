import { type ReactNode } from "react";
import SandboxTerminal from "./Terminal";

interface DashboardProps {
  command: string;
  connected: boolean;
  activePid: number | null;
  onTerminalInput: (data: string) => void;
  onScreenshot: () => void;
  children?: ReactNode;
}

export default function Dashboard({
  command,
  connected,
  activePid,
  onTerminalInput,
  onScreenshot,
  children,
}: DashboardProps) {
  return (
    <div
      className="flex-1 flex flex-col min-w-0 h-full overflow-hidden"
      style={{ backgroundColor: "var(--sandbox-bg-primary)" }}
    >
      {/* Header — draggable region */}
      <div
        data-tauri-drag-region
        className="flex items-center justify-between px-6 py-3 shrink-0 border-b"
        style={{
          borderColor: "var(--sandbox-border)",
          WebkitAppRegion: "drag",
        } as React.CSSProperties}
      >
        <h1
          className="text-lg font-semibold"
          style={{ color: "var(--sandbox-fg-primary)" }}
        >
          Dashboard
        </h1>
        <div
          data-tauri-no-drag
          className="flex items-center gap-2"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <button
            onClick={onScreenshot}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{
              color: "var(--sandbox-fg-secondary)",
              backgroundColor: "var(--sandbox-bg-tertiary)",
            }}
            title="Take Screenshot"
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
            Screenshot
          </button>
          <button
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-white transition-opacity hover:opacity-90"
            style={{ backgroundColor: "var(--sandbox-accent)" }}
          >
            Create New Sandbox
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 relative flex flex-col overflow-hidden">
        {/* Sandbox card */}
        <div
          className="flex-1 flex flex-col m-4 rounded-xl overflow-hidden border"
          style={{ borderColor: "var(--sandbox-border)" }}
        >
          {/* Card header */}
          <div
            className="flex items-center justify-between px-4 py-2.5 shrink-0"
            style={{ backgroundColor: "var(--sandbox-bg-secondary)" }}
          >
            <div className="flex items-center gap-2">
              <span
                className="text-xs"
                style={{ color: "var(--sandbox-fg-tertiary)" }}
              >
                {">_"}
              </span>
              <span
                className="text-sm font-medium"
                style={{ color: "var(--sandbox-fg-primary)" }}
              >
                {command} (Sandboxed)
              </span>
            </div>
            <ResourceStats connected={connected} />
          </div>

          {/* Terminal — fills remaining space */}
          <div className="flex-1 min-h-0">
            <SandboxTerminal onInput={onTerminalInput} activePid={activePid} />
          </div>
        </div>
      </div>

      {/* Screenshot preview floating panel */}
      {children}
    </div>
  );
}

function ResourceStats({ connected }: { connected: boolean }) {
  return (
    <div
      className="flex items-center gap-4 text-[10px]"
      style={{ color: "var(--sandbox-fg-tertiary)" }}
    >
      <Stat label="CPU" value={connected ? "12%" : "--"} />
      <Stat label="Memory" value={connected ? "180MB" : "--"} />
      <Stat label="Network" value={connected ? "1.5MB/s" : "--"} />
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-center">
      <div
        className="font-medium"
        style={{ color: "var(--sandbox-fg-secondary)" }}
      >
        {value}
      </div>
      <div>{label}</div>
    </div>
  );
}
