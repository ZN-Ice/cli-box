import { useTheme } from "../themes/ThemeContext";

interface SidebarProps {
  command: string;
}

export default function Sidebar({ command }: SidebarProps) {
  const { toggleTheme, theme } = useTheme();
  const isDark = theme.kind === "dark";

  return (
    <aside
      className="flex flex-col h-full w-52 shrink-0 select-none"
      data-tauri-drag-region
      style={{
        backgroundColor: "var(--sandbox-sidebar-bg)",
        color: "var(--sandbox-sidebar-fg)",
        borderRight: "1px solid var(--sandbox-sidebar-border)",
        WebkitAppRegion: "drag",
      } as React.CSSProperties}
    >
      {/* Logo */}
      <div className="flex items-center gap-2.5 px-5 pt-10 pb-4">
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
          Sandbox
        </span>
      </div>

      {/* Instances */}
      <div className="px-3 mt-1"
        data-tauri-no-drag
        style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
      >
        <div
          className="text-[10px] font-medium uppercase tracking-wider px-2 mb-1"
          style={{ color: "var(--sandbox-fg-tertiary)" }}
        >
          Instances
        </div>
        <div
          className="flex items-center gap-2 px-2 py-1.5 rounded-md text-xs"
          style={{
            backgroundColor: "var(--sandbox-sidebar-active)",
            color: "var(--sandbox-sidebar-fg)",
          }}
        >
          <svg
            className="w-4 h-4 shrink-0"
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
          <span className="truncate">{command}</span>
        </div>
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Theme toggle */}
      <div className="px-3 pb-4"
        data-tauri-no-drag
        style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
      >
        <button
          onClick={toggleTheme}
          className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-xs transition-colors"
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
