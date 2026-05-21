import { describe, it, expect } from "vitest";
import { themeRegistry } from "../themes/registry";
import type { SandboxTheme } from "../themes/types";

describe("ThemeRegistry", () => {
  it("has at least two themes registered", () => {
    const themes = themeRegistry.list();
    expect(themes.length).toBeGreaterThanOrEqual(2);
  });

  it("default theme is tokyo-night (dark)", () => {
    const t = themeRegistry.defaultTheme();
    expect(t.id).toBe("tokyo-night");
    expect(t.kind).toBe("dark");
    expect(t.name).toBe("Tokyo Night");
  });

  it("has a light theme available", () => {
    const themes = themeRegistry.list();
    const light = themes.find((t) => t.kind === "light");
    expect(light).toBeDefined();
    expect(light!.id).toBe("vscode-light");
  });

  it("get returns undefined for unknown id", () => {
    expect(themeRegistry.get("nonexistent")).toBeUndefined();
  });

  it("get returns correct theme for valid id", () => {
    const t = themeRegistry.get("tokyo-night");
    expect(t).toBeDefined();
    expect(t!.id).toBe("tokyo-night");
  });

  it("list returns unique themes (no duplicates)", () => {
    const themes = themeRegistry.list();
    const ids = themes.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("both themes have kind set", () => {
    const themes = themeRegistry.list();
    for (const t of themes) {
      expect(t.kind).toMatch(/^(dark|light)$/);
    }
  });
});

describe("Theme color contract", () => {
  const requiredColorKeys: (keyof SandboxTheme["colors"])[] = [
    "bgPrimary",
    "bgSecondary",
    "bgTertiary",
    "fgPrimary",
    "fgSecondary",
    "fgTertiary",
    "border",
    "accent",
    "scrollbarBg",
    "scrollbarFg",
    "success",
    "error",
    "titlebarBg",
    "titlebarFg",
    "sidebarBg",
    "sidebarFg",
    "sidebarBorder",
    "sidebarActive",
    "panelBg",
  ];

  const requiredTerminalKeys: (keyof SandboxTheme["terminal"])[] = [
    "background",
    "foreground",
    "cursor",
    "cursorAccent",
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
  ];

  themeRegistry.list().forEach((theme) => {
    describe(theme.name, () => {
      it("has all required color tokens as non-empty strings", () => {
        for (const key of requiredColorKeys) {
          expect(theme.colors[key]).toBeTypeOf("string");
          expect(theme.colors[key].length).toBeGreaterThan(0);
        }
      });

      it("has all required terminal theme keys as non-empty strings", () => {
        for (const key of requiredTerminalKeys) {
          expect(theme.terminal[key]).toBeTypeOf("string");
          expect(theme.terminal[key].length).toBeGreaterThan(0);
        }
      });

      it("has a valid id matching theme registration", () => {
        const registered = themeRegistry.get(theme.id);
        expect(registered).toBeDefined();
        expect(registered!.name).toBe(theme.name);
      });

      it("light theme is vscode-light, dark is tokyo-night", () => {
        if (theme.kind === "dark") {
          expect(theme.id).toBe("tokyo-night");
        } else {
          expect(theme.id).toBe("vscode-light");
        }
      });
    });
  });
});
