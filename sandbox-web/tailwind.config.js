/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        sandbox: {
          bg: {
            primary: "var(--sandbox-bg-primary)",
            secondary: "var(--sandbox-bg-secondary)",
            tertiary: "var(--sandbox-bg-tertiary)",
          },
          fg: {
            primary: "var(--sandbox-fg-primary)",
            secondary: "var(--sandbox-fg-secondary)",
            tertiary: "var(--sandbox-fg-tertiary)",
          },
          border: "var(--sandbox-border)",
          accent: "var(--sandbox-accent)",
          scrollbar: {
            bg: "var(--sandbox-scrollbar-bg)",
            fg: "var(--sandbox-scrollbar-fg)",
          },
          success: "var(--sandbox-success)",
          error: "var(--sandbox-error)",
          titlebar: {
            bg: "var(--sandbox-titlebar-bg)",
            fg: "var(--sandbox-titlebar-fg)",
          },
          sidebar: {
            bg: "var(--sandbox-sidebar-bg)",
            fg: "var(--sandbox-sidebar-fg)",
            border: "var(--sandbox-sidebar-border)",
            active: "var(--sandbox-sidebar-active)",
          },
          panel: {
            bg: "var(--sandbox-panel-bg)",
          },
        },
      },
      fontFamily: {
        mono: [
          '"SF Mono"',
          '"Menlo"',
          '"Monaco"',
          '"Cascadia Code"',
          '"JetBrains Mono"',
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
