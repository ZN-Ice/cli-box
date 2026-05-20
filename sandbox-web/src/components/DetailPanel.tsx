import type { HealthResponse } from "../api";

interface DetailPanelProps {
  health: HealthResponse | null;
  connected: boolean;
}

export default function DetailPanel({ health, connected }: DetailPanelProps) {
  const uptime = health?.uptime_secs ? formatUptime(health.uptime_secs) : "--";

  return (
    <aside
      className="w-60 shrink-0 flex flex-col h-full overflow-y-auto border-l"
      style={{
        backgroundColor: "var(--sandbox-panel-bg)",
        borderColor: "var(--sandbox-border)",
      }}
    >
      <div className="p-5 space-y-5">
        {/* Sandbox name */}
        <h2
          className="text-sm font-semibold"
          style={{ color: "var(--sandbox-fg-primary)" }}
        >
          {health?.sandbox_id ?? "Sandbox"}
        </h2>

        {/* Status */}
        <Section title="Status">
          <div className="flex items-center gap-2">
            <span
              className="text-xs font-semibold"
              style={{
                color: connected
                  ? "var(--sandbox-success)"
                  : "var(--sandbox-fg-tertiary)",
              }}
            >
              {connected ? "ACTIVE" : "INACTIVE"}
            </span>
            {connected && (
              <span
                className="w-1.5 h-1.5 rounded-full"
                style={{ backgroundColor: "var(--sandbox-success)" }}
              />
            )}
          </div>
          <span
            className="text-[11px]"
            style={{ color: "var(--sandbox-fg-tertiary)" }}
          >
            uptime {uptime}
          </span>
        </Section>

        {/* Tools */}
        <Section title="Tools">
          <DetailLine label="Node.js" value="18.17" />
          <DetailLine label="Python" value="3.10" />
        </Section>

        {/* Network */}
        <Section title="Network">
          <span
            className="text-[11px]"
            style={{ color: "var(--sandbox-fg-secondary)" }}
          >
            Disabled
          </span>
        </Section>

        {/* Files */}
        <Section title="Files">
          <span
            className="text-[11px]"
            style={{ color: "var(--sandbox-fg-secondary)" }}
          >
            Isolated to ~/Sandbox
          </span>
        </Section>
      </div>

      {/* Running GUI App */}
      <div
        className="mt-auto p-5 border-t"
        style={{ borderColor: "var(--sandbox-border)" }}
      >
        <h3
          className="text-xs font-semibold mb-2"
          style={{ color: "var(--sandbox-fg-primary)" }}
        >
          Running GUI App
        </h3>
        <div
          className="rounded-lg border p-3 text-center"
          style={{
            borderColor: "var(--sandbox-border)",
            backgroundColor: "var(--sandbox-bg-primary)",
          }}
        >
          <svg
            className="w-5 h-5 mx-auto mb-1.5"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={1.5}
            style={{ color: "var(--sandbox-fg-tertiary)" }}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
            />
          </svg>
          <div
            className="text-[11px] font-medium"
            style={{ color: "var(--sandbox-fg-secondary)" }}
          >
            No GUI App
          </div>
          <div
            className="text-[10px]"
            style={{ color: "var(--sandbox-fg-tertiary)" }}
          >
            Isolated window
          </div>
        </div>
      </div>
    </aside>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <h3
        className="text-xs font-semibold mb-1.5"
        style={{ color: "var(--sandbox-fg-primary)" }}
      >
        {title}
      </h3>
      <div className="space-y-0.5">{children}</div>
    </div>
  );
}

function DetailLine({ label, value }: { label: string; value: string }) {
  return (
    <div
      className="text-[11px]"
      style={{ color: "var(--sandbox-fg-secondary)" }}
    >
      {label} {value}
    </div>
  );
}

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
