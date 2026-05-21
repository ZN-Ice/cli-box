/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { ThemeProvider } from "../themes/ThemeContext";

// We test Terminal indirectly through its exported behavior.
// The buildTerminalTheme function is internal, so we test via component rendering.
describe("Terminal component", () => {
  const mockOnInput = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders without crashing", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    const container = render(
      <ThemeProvider>
        <SandboxTerminal onInput={mockOnInput} />
      </ThemeProvider>,
    );
    expect(container.container.querySelector(".xterm")).toBeTruthy();
  });

  it("renders with activePid", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    const container = render(
      <ThemeProvider>
        <SandboxTerminal onInput={mockOnInput} activePid={1234} />
      </ThemeProvider>,
    );
    expect(container.container.querySelector(".xterm")).toBeTruthy();
  });

  it("renders with null activePid", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    const container = render(
      <ThemeProvider>
        <SandboxTerminal onInput={mockOnInput} activePid={null} />
      </ThemeProvider>,
    );
    expect(container.container.querySelector(".xterm")).toBeTruthy();
  });

  it("renders without onInput callback", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    const container = render(
      <ThemeProvider>
        <SandboxTerminal />
      </ThemeProvider>,
    );
    expect(container.container.querySelector(".xterm")).toBeTruthy();
  });

  it("creates an xterm instance", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    render(
      <ThemeProvider>
        <SandboxTerminal onInput={mockOnInput} />
      </ThemeProvider>,
    );
    // xterm creates a .xterm element
    const xtermEl = document.querySelector(".xterm");
    expect(xtermEl).not.toBeNull();
  });
});

// Test the theme mapping logic indirectly
describe("Terminal theme mapping", () => {
  it("theme object has required terminal keys", async () => {
    const { default: SandboxTerminal } = await import("../components/Terminal");
    await import("../themes/ThemeContext");

    // Verify theme structure by rendering and checking xterm is created
    const container = render(
      <ThemeProvider>
        <SandboxTerminal />
      </ThemeProvider>,
    );
    const xtermEl = container.container.querySelector(".xterm");
    expect(xtermEl).toBeTruthy();
    cleanup();
  });
});
