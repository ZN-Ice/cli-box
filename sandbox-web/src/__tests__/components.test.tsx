/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import Dashboard from "../components/Dashboard";
import Sidebar from "../components/Sidebar";
import { ThemeProvider } from "../themes/ThemeContext";

// ── Dashboard ──────────────────────────────────────────────────

describe("Dashboard", () => {
  const defaultProps = {
    command: "claude",
    connected: true,
    activePid: null as number | null,
    onScreenshot: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders command name in the card header", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    expect(screen.getByText(/claude \(Sandboxed\)/)).toBeDefined();
  });

  it("renders Dashboard title in header", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    expect(screen.getByText("Dashboard")).toBeDefined();
  });

  it("renders Screenshot button", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    const btn = screen.getByTitle("Take Screenshot");
    expect(btn).toBeDefined();
  });

  it("calls onScreenshot when Screenshot button is clicked", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByTitle("Take Screenshot"));
    expect(defaultProps.onScreenshot).toHaveBeenCalledOnce();
  });

  it("renders 'Create New Sandbox' button", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    expect(screen.getByText("Create New Sandbox")).toBeDefined();
  });

  it("shows -- stats when not connected", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} connected={false} />
      </ThemeProvider>,
    );
    const dashes = screen.getAllByText("--");
    expect(dashes.length).toBeGreaterThanOrEqual(3);
  });

  it("shows percentage stats when connected", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} connected={true} />
      </ThemeProvider>,
    );
    expect(screen.getByText("12%")).toBeDefined();
    expect(screen.getByText("180MB")).toBeDefined();
  });

  it("renders stat labels", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps} />
      </ThemeProvider>,
    );
    expect(screen.getByText("CPU")).toBeDefined();
    expect(screen.getByText("Memory")).toBeDefined();
    expect(screen.getByText("Network")).toBeDefined();
  });

  it("renders children when provided", () => {
    render(
      <ThemeProvider>
        <Dashboard {...defaultProps}>
          <div data-testid="child">Test Child</div>
        </Dashboard>
      </ThemeProvider>,
    );
    expect(screen.getByTestId("child").textContent).toBe("Test Child");
  });
});

// ── Sidebar ────────────────────────────────────────────────────

describe("Sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the command name", () => {
    render(
      <ThemeProvider>
        <Sidebar command="claude" />
      </ThemeProvider>,
    );
    expect(screen.getByText("claude")).toBeDefined();
  });

  it("renders 'Sandbox' title", () => {
    render(
      <ThemeProvider>
        <Sidebar command="echo" />
      </ThemeProvider>,
    );
    expect(screen.getByText("Sandbox")).toBeDefined();
  });

  it("renders 'Instances' section", () => {
    render(
      <ThemeProvider>
        <Sidebar command="zsh" />
      </ThemeProvider>,
    );
    expect(screen.getByText("Instances")).toBeDefined();
  });

  it("renders theme toggle button in dark mode", () => {
    render(
      <ThemeProvider>
        <Sidebar command="test" />
      </ThemeProvider>,
    );
    // Default is dark mode, so it should show "Light Mode"
    expect(screen.getByText("Light Mode")).toBeDefined();
  });

  it("toggle button has correct title", () => {
    render(
      <ThemeProvider>
        <Sidebar command="test" />
      </ThemeProvider>,
    );
    const btn = screen.getByTitle("Switch to light theme");
    expect(btn).toBeDefined();
  });

  it("toggles to light mode on click", () => {
    render(
      <ThemeProvider>
        <Sidebar command="test" />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByTitle("Switch to light theme"));
    expect(screen.getByText("Dark Mode")).toBeDefined();
    expect(screen.getByTitle("Switch to dark theme")).toBeDefined();
  });
});
