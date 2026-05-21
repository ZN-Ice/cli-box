/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import {
  ThemeProvider,
  useTheme,
  ThemeContext,
  STORAGE_KEY,
} from "../themes/ThemeContext";

function Consumer() {
  const { theme, setTheme, toggleTheme, themes } = useTheme();
  return (
    <div>
      <span data-testid="theme-id">{theme.id}</span>
      <span data-testid="theme-kind">{theme.kind}</span>
      <span data-testid="theme-count">{themes.length}</span>
      <button data-testid="toggle" onClick={toggleTheme} />
      <button data-testid="set-dark" onClick={() => setTheme("tokyo-night")} />
      <button
        data-testid="set-light"
        onClick={() => setTheme("vscode-light")}
      />
      <button
        data-testid="set-invalid"
        onClick={() => setTheme("nonexistent")}
      />
    </div>
  );
}

describe("ThemeContext", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-theme-kind");
  });

  afterEach(() => {
    cleanup();
  });

  it("provides default theme (tokyo-night)", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
    expect(screen.getByTestId("theme-kind").textContent).toBe("dark");
  });

  it("restores theme from localStorage", () => {
    localStorage.setItem(STORAGE_KEY, "vscode-light");
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-id").textContent).toBe("vscode-light");
    expect(screen.getByTestId("theme-kind").textContent).toBe("light");
  });

  it("falls back to default if stored id is invalid", () => {
    localStorage.setItem(STORAGE_KEY, "nonexistent");
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
  });

  it("setTheme changes the active theme", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
    fireEvent.click(screen.getByTestId("set-light"));
    expect(screen.getByTestId("theme-id").textContent).toBe("vscode-light");
  });

  it("setTheme persists to localStorage", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByTestId("set-light"));
    expect(localStorage.getItem(STORAGE_KEY)).toBe("vscode-light");
  });

  it("setTheme with invalid id is a no-op", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByTestId("set-invalid"));
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
  });

  it("toggleTheme cycles through themes", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
    fireEvent.click(screen.getByTestId("toggle"));
    expect(screen.getByTestId("theme-id").textContent).toBe("vscode-light");
    fireEvent.click(screen.getByTestId("toggle"));
    expect(screen.getByTestId("theme-id").textContent).toBe("tokyo-night");
  });

  it("exposes list of all themes", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme-count").textContent).toBe("2");
  });

  it("applies CSS custom properties on mount", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    const root = document.documentElement;
    expect(root.getAttribute("data-theme")).toBe("tokyo-night");
    expect(root.getAttribute("data-theme-kind")).toBe("dark");
    // Check a few CSS variables are set
    expect(root.style.getPropertyValue("--sandbox-bg-primary")).toBeTruthy();
    expect(root.style.getPropertyValue("--sandbox-fg-primary")).toBeTruthy();
  });

  it("updates CSS custom properties on theme change", () => {
    render(
      <ThemeProvider>
        <Consumer />
      </ThemeProvider>,
    );
    const root = document.documentElement;
    const darkBg = root.style.getPropertyValue("--sandbox-bg-primary");
    fireEvent.click(screen.getByTestId("set-light"));
    const lightBg = root.style.getPropertyValue("--sandbox-bg-primary");
    // Light and dark themes should have different backgrounds
    expect(darkBg).not.toBe(lightBg);
    expect(root.getAttribute("data-theme")).toBe("vscode-light");
    expect(root.getAttribute("data-theme-kind")).toBe("light");
  });

  it("useTheme throws when used outside ThemeProvider", () => {
    // Suppress console.error from React
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    function BadConsumer() {
      useTheme();
      return null;
    }
    expect(() => render(<BadConsumer />)).toThrow(
      "useTheme must be used within ThemeProvider",
    );
    spy.mockRestore();
  });

  it("ThemeContext exports correct STORAGE_KEY", () => {
    expect(STORAGE_KEY).toBe("sandbox-theme");
  });

  it("ThemeContext default value is null (no provider)", () => {
    // @ts-expect-error Accessing React internal for test
    expect(ThemeContext._currentValue).toBeNull();
  });
});
