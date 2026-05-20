import { type ReactNode } from "react";
import SandboxTerminal from "./Terminal";
import type { ProcessInfo } from "../api";

interface DashboardProps {
  sandboxName: string;
  connected: boolean;
  activePid: number | null;
  onTerminalInput: (data: string) => void;
  onScreenshot: () => void;
  processes: ProcessInfo[];
  children?: ReactNode;
}

export default function Dashboard({
  sandboxName,
  connected,
  activePid,
  onTerminalInput,
  onScreenshot,
  processes,
  children,
}: DashboardProps) {
  return (
    <div
      className="flex-1 flex flex-col min-w-0 h-full overflow-hidden"
      style={{ backgroundColor: "var(--sandbox-bg-primary)" }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-6 py-3 shrink-0 border-b"
        style={{ borderColor: "var(--sandbox-border)" }}
      >
        <h1
          className="text-lg font-semibold"
          style={{ color: "var(--sandbox-fg-primary)" }}
        >
          Dashboard
        </h1>
        <div className="flex items-center gap-2">
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
      <div className="flex-1 relative overflow-y-auto p-6 space-y-6">
        {/* Sandbox card */}
        <div
          className="rounded-xl overflow-hidden border"
          style={{ borderColor: "var(--sandbox-border)" }}
        >
          {/* Card header */}
          <div
            className="flex items-center justify-between px-4 py-2.5"
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
                {sandboxName} (Sandboxed)
              </span>
            </div>
            <ResourceStats connected={connected} />
          </div>

          {/* Terminal */}
          <div className="h-[320px]">
            <SandboxTerminal onInput={onTerminalInput} activePid={activePid} />
          </div>
        </div>

        {/* Instances list */}
        <div>
          <h2
            className="text-sm font-semibold mb-3"
            style={{ color: "var(--sandbox-fg-primary)" }}
          >
            Instances
          </h2>
          <div className="space-y-1">
            {processes.map((p) => (
              <InstanceRow key={p.pid} process={p} />
            ))}
            {processes.length === 0 && (
              <div
                className="text-xs py-3 px-3 rounded-lg"
                style={{ color: "var(--sandbox-fg-tertiary)" }}
              >
                No running instances
              </div>
            )}
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

function InstanceRow({ process }: { process: ProcessInfo }) {
  const isRunning = process.is_running;
  return (
    <div
      className="flex items-center justify-between px-3 py-2 rounded-lg border text-sm"
      style={{
        backgroundColor: "var(--sandbox-bg-secondary)",
        borderColor: "var(--sandbox-border)",
      }}
    >
      <div className="flex items-center gap-2.5">
        <span
          className="text-xs"
          style={{ color: "var(--sandbox-fg-tertiary)" }}
        >
          {">_"}
        </span>
        <span style={{ color: "var(--sandbox-fg-primary)" }}>
          {process.name}
        </span>
      </div>
      <div className="flex items-center gap-2 text-xs">
        <span
          className="font-medium"
          style={{
            color: isRunning
              ? "var(--sandbox-success)"
              : "var(--sandbox-fg-tertiary)",
          }}
        >
          {isRunning ? "Running" : "Stopped"}
        </span>
        {isRunning && (
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={{ backgroundColor: "var(--sandbox-success)" }}
          />
        )}
      </div>
    </div>
  );
}
