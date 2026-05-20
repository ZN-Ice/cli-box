import { useTheme } from "../themes/ThemeContext";

interface SidebarProps {
  sandboxName: string;
}

export default function Sidebar({ sandboxName }: SidebarProps) {
  const { toggleTheme, theme } = useTheme();
  const isDark = theme.kind === "dark";

  return (
    <aside
      className="flex flex-col h-full w-56 shrink-0 select-none"
      style={{
        backgroundColor: "var(--sandbox-sidebar-bg)",
        color: "var(--sandbox-sidebar-fg)",
        borderRight: "1px solid var(--sandbox-sidebar-border)",
      }}
    >
      {/* Logo */}
      <div className="flex items-center gap-2.5 px-5 py-4">
        <div
          className="w-7 h-7 rounded-lg flex items-center justify-center"
          style={{ background: "linear-gradient(135deg, #7aa2f7, #bb9af7)" }}
        >
          <svg
            className="w-4 h-4 text-white"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
        </div>
        <span
          className="text-sm font-semibold"
          style={{ color: "var(--sandbox-sidebar-fg)" }}
        >
          System Sandbox
        </span>
      </div>

      {/* Sandboxes nav */}
      <div className="px-3 mt-1">
        <div
          className="text-[10px] font-medium uppercase tracking-wider px-2 mb-1"
          style={{ color: "var(--sandbox-fg-tertiary)" }}
        >
          Sandboxes
        </div>
        <NavItem icon={<CircleIcon />} label="Active" active />
        <NavItem icon={<TemplateIcon />} label="Templates" />
        <NavItem icon={<HistoryIcon />} label="History" />
      </div>

      {/* Instances */}
      <div className="px-3 mt-4">
        <div
          className="text-[10px] font-medium uppercase tracking-wider px-2 mb-1"
          style={{ color: "var(--sandbox-fg-tertiary)" }}
        >
          Instances
        </div>
        <InstanceItem name={sandboxName} icon={<TerminalIcon />} selected />
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Bottom nav */}
      <div className="px-3 pb-4">
        <NavItem icon={<AppsIcon />} label="Apps" />
        <NavItem icon={<LogsIcon />} label="Logs" />
        {/* Theme toggle */}
        <button
          onClick={toggleTheme}
          className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-xs transition-colors mt-1"
          style={{ color: "var(--sandbox-fg-secondary)" }}
          onMouseEnter={(e) => {
            e.currentTarget.style.backgroundColor =
              "var(--sandbox-sidebar-active)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.backgroundColor = "transparent";
          }}
          title={`Switch to ${isDark ? "light" : "dark"} theme`}
        >
          {isDark ? (
            <svg
              className="w-4 h-4"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z"
              />
            </svg>
          ) : (
            <svg
              className="w-4 h-4"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z"
              />
            </svg>
          )}
          <span>{isDark ? "Light Mode" : "Dark Mode"}</span>
        </button>
      </div>
    </aside>
  );
}

function NavItem({
  icon,
  label,
  active,
}: {
  icon: React.ReactNode;
  label: string;
  active?: boolean;
}) {
  return (
    <div
      className="flex items-center gap-2 px-2 py-1.5 rounded-md text-xs cursor-pointer transition-colors"
      style={{
        backgroundColor: active
          ? "var(--sandbox-sidebar-active)"
          : "transparent",
        color: active
          ? "var(--sandbox-sidebar-fg)"
          : "var(--sandbox-fg-secondary)",
      }}
      onMouseEnter={(e) => {
        if (!active)
          e.currentTarget.style.backgroundColor =
            "var(--sandbox-sidebar-active)";
      }}
      onMouseLeave={(e) => {
        if (!active) e.currentTarget.style.backgroundColor = "transparent";
      }}
    >
      {icon}
      <span>{label}</span>
    </div>
  );
}

function InstanceItem({
  name,
  icon,
  selected,
}: {
  name: string;
  icon: React.ReactNode;
  selected?: boolean;
}) {
  return (
    <div
      className="flex items-center gap-2 px-2 py-1.5 rounded-md text-xs cursor-pointer transition-colors"
      style={{
        backgroundColor: selected
          ? "var(--sandbox-sidebar-active)"
          : "transparent",
        color: selected
          ? "var(--sandbox-sidebar-fg)"
          : "var(--sandbox-fg-secondary)",
      }}
      onMouseEnter={(e) => {
        if (!selected)
          e.currentTarget.style.backgroundColor =
            "var(--sandbox-sidebar-active)";
      }}
      onMouseLeave={(e) => {
        if (!selected) e.currentTarget.style.backgroundColor = "transparent";
      }}
    >
      {icon}
      <span className="truncate">{name}</span>
    </div>
  );
}

// --- Icons ---

function CircleIcon() {
  return (
    <svg
      className="w-4 h-4"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
    >
      <circle cx="12" cy="12" r="10" />
      <circle
        cx="12"
        cy="12"
        r="3"
        fill="var(--sandbox-success)"
        stroke="none"
      />
    </svg>
  );
}

function TemplateIcon() {
  return (
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      strokeWidth={1.5}
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
      />
    </svg>
  );
}

function HistoryIcon() {
  return (
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      strokeWidth={1.5}
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
  );
}

function TerminalIcon() {
  return (
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      strokeWidth={1.5}
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0021 18V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v12a2.25 2.25 0 002.25 2.25z"
      />
    </svg>
  );
}

function AppsIcon() {
  return (
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      strokeWidth={1.5}
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 6v2.25a2.25 2.25 0 01-2.25 2.25H6a2.25 2.25 0 01-2.25-2.25V6zM3.75 15.75A2.25 2.25 0 016 13.5h2.25a2.25 2.25 0 012.25 2.25V18a2.25 2.25 0 01-2.25 2.25H6A2.25 2.25 0 013.75 18v-2.25zM13.5 6a2.25 2.25 0 012.25-2.25H18A2.25 2.25 0 0120.25 6v2.25A2.25 2.25 0 0118 10.5h-2.25a2.25 2.25 0 01-2.25-2.25V6zM13.5 15.75a2.25 2.25 0 012.25-2.25H18a2.25 2.25 0 012.25 2.25V18A2.25 2.25 0 0118 20.25h-2.25A2.25 2.25 0 0113.5 18v-2.25z"
      />
    </svg>
  );
}

function LogsIcon() {
  return (
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      strokeWidth={1.5}
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M3.75 12h16.5m-16.5 3.75h16.5M3.75 19.5h16.5M5.625 4.5h12.75a1.875 1.875 0 010 3.75H5.625a1.875 1.875 0 010-3.75z"
      />
    </svg>
  );
}
