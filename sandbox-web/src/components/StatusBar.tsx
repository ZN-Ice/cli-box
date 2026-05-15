interface ProcessInfo {
  pid: number;
  name: string;
  is_running: boolean;
}

interface StatusBarProps {
  processes: ProcessInfo[];
  screenshotCount: number;
  serverStatus: "running" | "stopped" | "error";
  httpPort?: number;
}

export default function StatusBar({
  processes,
  screenshotCount,
  serverStatus,
  httpPort = 5801,
}: StatusBarProps) {
  const runningCount = processes.filter((p) => p.is_running).length;

  const statusColor = {
    running: "bg-green-500",
    stopped: "bg-gray-500",
    error: "bg-red-500",
  }[serverStatus];

  return (
    <div className="flex items-center justify-between h-8 px-3 bg-gray-850 border-t border-gray-700 text-xs text-gray-400">
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1.5">
          <span className={`w-2 h-2 rounded-full ${statusColor}`} />
          <span>
            Server: {serverStatus}
            {serverStatus === "running" && ` (:${httpPort})`}
          </span>
        </div>
        <span>|</span>
        <span>
          Processes: {runningCount} running / {processes.length} tracked
        </span>
      </div>

      <div className="flex items-center gap-4">
        <span>Screenshots: {screenshotCount}</span>
        <span>|</span>
        <span>macOS Sandbox v0.1.0</span>
      </div>
    </div>
  );
}
