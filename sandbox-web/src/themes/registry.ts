import type { SandboxTheme } from "./types";
import { tokyoNight } from "./tokyo-night";
import { vscodeLight } from "./vscode-light";

class ThemeRegistry {
  private themes = new Map<string, SandboxTheme>();

  constructor() {
    this.register(tokyoNight);
    this.register(vscodeLight);
  }

  register(theme: SandboxTheme): void {
    this.themes.set(theme.id, theme);
  }

  get(id: string): SandboxTheme | undefined {
    return this.themes.get(id);
  }

  list(): SandboxTheme[] {
    return Array.from(this.themes.values());
  }

  defaultTheme(): SandboxTheme {
    return tokyoNight;
  }
}

export const themeRegistry = new ThemeRegistry();
