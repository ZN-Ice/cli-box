import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import * as api from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  onInput?: (data: string) => void;
  activePid?: number | null;
}

// Tokyo Night inspired color scheme — clean, readable, macOS-like
const TERM_THEME = {
  background: "#1a1b26",
  foreground: "#a9b1d6",
  cursor: "#c0caf5",
  cursorAccent: "#1a1b26",
  selectionBackground: "rgba(122, 162, 247, 0.3)",
  selectionForeground: "#c0caf5",
  black: "#15161e",
  red: "#f7768e",
  green: "#9ece6a",
  yellow: "#e0af68",
  blue: "#7aa2f7",
  magenta: "#bb9af7",
  cyan: "#7dcfff",
  white: "#a9b1d6",
  brightBlack: "#414868",
  brightRed: "#f7768e",
  brightGreen: "#9ece6a",
  brightYellow: "#e0af68",
  brightBlue: "#7aa2f7",
  brightMagenta: "#bb9af7",
  brightCyan: "#7dcfff",
  brightWhite: "#c0caf5",
};

export default function SandboxTerminal({
  onInput,
  activePid = null,
}: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Initialize xterm.js once
  useEffect(() => {
    if (!terminalRef.current || xtermRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 14,
      lineHeight: 1.35,
      fontFamily:
        '"SF Mono", "Menlo", "Monaco", "Cascadia Code", "JetBrains Mono", monospace',
      fontWeight: "400",
      fontWeightBold: "600",
      letterSpacing: 0,
      scrollback: 10000,
      theme: TERM_THEME,
      allowProposedApi: true,
      drawBoldTextInBrightColors: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();

    term.onData((data) => {
      onInput?.(data);
    });

    const handleResize = () => fitAddon.fit();
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // PTY output polling
  useEffect(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }

    if (activePid === null || activePid === undefined) return;

    pollRef.current = setInterval(async () => {
      try {
        const result = await api.ptyRead(activePid);
        if (result.output) {
          xtermRef.current?.write(result.output);
        }
      } catch {
        if (pollRef.current) {
          clearInterval(pollRef.current);
          pollRef.current = null;
        }
      }
    }, 100); // 100ms polling for smooth streaming

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [activePid]);

  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitAddonRef.current?.fit());
    }
  }, []);

  return (
    <div ref={containerRef} className="w-full h-full">
      <div ref={terminalRef} className="w-full h-full" />
    </div>
  );
}
