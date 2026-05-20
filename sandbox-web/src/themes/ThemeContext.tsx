import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from "react";
import type { SandboxTheme, TerminalTheme } from "./types";
import { themeRegistry } from "./registry";

interface ThemeContextValue {
  theme: SandboxTheme;
  setTheme: (id: string) => void;
  toggleTheme: () => void;
  themes: SandboxTheme[];
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = "sandbox-theme";

function applyThemeCSS(theme: SandboxTheme): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", theme.id);
  root.setAttribute("data-theme-kind", theme.kind);

  const c = theme.colors;
  root.style.setProperty("--sandbox-bg-primary", c.bgPrimary);
  root.style.setProperty("--sandbox-bg-secondary", c.bgSecondary);
  root.style.setProperty("--sandbox-bg-tertiary", c.bgTertiary);
  root.style.setProperty("--sandbox-fg-primary", c.fgPrimary);
  root.style.setProperty("--sandbox-fg-secondary", c.fgSecondary);
  root.style.setProperty("--sandbox-fg-tertiary", c.fgTertiary);
  root.style.setProperty("--sandbox-border", c.border);
  root.style.setProperty("--sandbox-accent", c.accent);
  root.style.setProperty("--sandbox-scrollbar-bg", c.scrollbarBg);
  root.style.setProperty("--sandbox-scrollbar-fg", c.scrollbarFg);
  root.style.setProperty("--sandbox-success", c.success);
  root.style.setProperty("--sandbox-error", c.error);
  root.style.setProperty("--sandbox-titlebar-bg", c.titlebarBg);
  root.style.setProperty("--sandbox-titlebar-fg", c.titlebarFg);
  root.style.setProperty("--sandbox-sidebar-bg", c.sidebarBg);
  root.style.setProperty("--sandbox-sidebar-fg", c.sidebarFg);
  root.style.setProperty("--sandbox-sidebar-border", c.sidebarBorder);
  root.style.setProperty("--sandbox-sidebar-active", c.sidebarActive);
  root.style.setProperty("--sandbox-panel-bg", c.panelBg);
}

export { ThemeContext, STORAGE_KEY };
export type { ThemeContextValue, SandboxTheme, TerminalTheme };

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<SandboxTheme>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const t = themeRegistry.get(stored);
      if (t) return t;
    }
    return themeRegistry.defaultTheme();
  });

  useEffect(() => {
    applyThemeCSS(theme);
  }, [theme]);

  const setTheme = useCallback((id: string) => {
    const t = themeRegistry.get(id);
    if (t) {
      setThemeState(t);
      localStorage.setItem(STORAGE_KEY, id);
    }
  }, []);

  const toggleTheme = useCallback(() => {
    const themes = themeRegistry.list();
    const idx = themes.findIndex((t) => t.id === theme.id);
    const next = themes[(idx + 1) % themes.length];
    setTheme(next.id);
  }, [theme.id, setTheme]);

  return (
    <ThemeContext.Provider
      value={{ theme, setTheme, toggleTheme, themes: themeRegistry.list() }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
