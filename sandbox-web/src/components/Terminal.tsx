import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import * as api from "../api";
import { useTheme } from "../themes/ThemeContext";
import type { TerminalTheme } from "../themes/types";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  activePid?: number | null;
  onReady?: (cols: number, rows: number) => void;
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

export default function SandboxTerminal({ activePid = null, onReady }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const wsConnRef = useRef<api.PtyWsConnection | null>(null);
  const activePidRef = useRef(activePid);
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  const { theme } = useTheme();

  // Keep activePidRef in sync so the resize handler (which closes over it)
  // always reads the latest value without recreating the init effect.
  useEffect(() => {
    activePidRef.current = activePid;
  }, [activePid]);

  // Initialize xterm.js once — theme updates in-place
  useEffect(() => {
    if (!terminalRef.current) return;
    if (xtermRef.current) return; // already initialized

    console.log("[Terminal] initializing xterm.js");

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

    // Notify parent of initial terminal size
    onReadyRef.current?.(term.cols, term.rows);

    // Send keyboard input directly to the WebSocket connection
    term.onData((data) => {
      wsConnRef.current?.sendInput(data);
    });

    const handleResize = () => {
      fitAddon.fit();
      const pid = activePidRef.current;
      const conn = wsConnRef.current;
      if (pid && conn) {
        conn.resize(term.cols, term.rows);
      }
    };
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Update terminal theme in-place without disposing
  useEffect(() => {
    if (!xtermRef.current) return;
    const newTheme = buildTerminalTheme(theme.terminal);
    xtermRef.current.options.theme = newTheme;
    console.log(`[Terminal] theme updated to: ${theme.id} (${theme.kind})`);
  }, [theme.id]);

  // PTY WebSocket connection
  useEffect(() => {
    // Clean up previous connection
    wsConnRef.current?.close();
    wsConnRef.current = null;

    if (activePid === null || activePid === undefined) return;

    const conn = api.ptyConnectWs(activePid);
    wsConnRef.current = conn;

    // Pipe PTY output → xterm.js
    conn.onOutput((data) => {
      xtermRef.current?.write(data);
    });

    // Send initial resize so PTY matches xterm container size
    const term = xtermRef.current;
    if (term) {
      conn.resize(term.cols, term.rows);
    }

    return () => {
      conn.close();
      wsConnRef.current = null;
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
