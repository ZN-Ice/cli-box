import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import * as api from "../api";
import { useTheme } from "../themes/ThemeContext";
import type { TerminalTheme } from "../themes/types";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  onInput?: (data: string) => void;
  activePid?: number | null;
}

function buildTerminalTheme(t: TerminalTheme): Record<string, string> {
  return {
    background: t.background,
    foreground: t.foreground,
    cursor: t.cursor,
    cursorAccent: t.cursorAccent,
    selectionBackground: t.selectionBackground,
    selectionForeground: t.selectionForeground,
    black: t.black,
    red: t.red,
    green: t.green,
    yellow: t.yellow,
    blue: t.blue,
    magenta: t.magenta,
    cyan: t.cyan,
    white: t.white,
    brightBlack: t.brightBlack,
    brightRed: t.brightRed,
    brightGreen: t.brightGreen,
    brightYellow: t.brightYellow,
    brightBlue: t.brightBlue,
    brightMagenta: t.brightMagenta,
    brightCyan: t.brightCyan,
    brightWhite: t.brightWhite,
  };
}

export default function SandboxTerminal({
  onInput,
  activePid = null,
}: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const { theme } = useTheme();

  // Initialize xterm.js — recreate on theme change
  useEffect(() => {
    if (!terminalRef.current) return;

    // Dispose previous terminal
    if (xtermRef.current) {
      xtermRef.current.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    }

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
      theme: buildTerminalTheme(theme.terminal),
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
      xtermRef.current = null;
    };
  }, [theme.id]); // eslint-disable-line react-hooks/exhaustive-deps

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
    }, 100);

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
