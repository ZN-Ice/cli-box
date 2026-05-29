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
  onWsError?: (msg: string) => void;
  onWsClose?: (code: number, reason: string) => void;
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

/**
 * Bypass xterm.js WriteBuffer's setTimeout-based scheduling which stalls in
 * Tauri's WKWebView. Instead, call InputHandler.parse() directly and then
 * fire the write-parsed event to trigger rendering.
 */
function writeDirect(term: Terminal, data: string | Uint8Array): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const core = (term as any)._core ?? (term as any).core;
  if (!core) return;
  const ih = core._inputHandler;
  if (!ih || typeof ih.parse !== "function") return;

  // 防止单次解析过大数据导致主线程冻结
  const CHUNK_SIZE = 32 * 1024; // 32KB
  if (typeof data === "string" && data.length > CHUNK_SIZE) {
    for (let i = 0; i < data.length; i += CHUNK_SIZE) {
      ih.parse(data.slice(i, i + CHUNK_SIZE), true);
    }
  } else if (data instanceof Uint8Array && data.length > CHUNK_SIZE) {
    for (let i = 0; i < data.length; i += CHUNK_SIZE) {
      ih.parse(data.slice(i, i + CHUNK_SIZE), true);
    }
  } else {
    ih.parse(data, true);
  }

  if (core._writeBuffer?._onWriteParsed) {
    core._writeBuffer._onWriteParsed.fire();
  }
}

export default function SandboxTerminal({ activePid = null, onReady, onWsError, onWsClose }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const wsConnRef = useRef<api.PtyWsConnection | null>(null);
  const activePidRef = useRef(activePid);
  const onReadyRef = useRef(onReady);
  const onWsErrorRef = useRef(onWsError);
  const onWsCloseRef = useRef(onWsClose);
  onReadyRef.current = onReady;
  onWsErrorRef.current = onWsError;
  onWsCloseRef.current = onWsClose;
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

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 14,
      fontFamily:
        '"SF Mono", "Menlo", "Monaco", "Cascadia Code", "JetBrains Mono", monospace',
      fontWeight: "400",
      fontWeightBold: "600",
      scrollback: 10000,
      theme: buildTerminalTheme(theme.terminal),
      allowProposedApi: true,
      allowTransparency: true,
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
  }, [theme.id]);

  // PTY WebSocket connection
  useEffect(() => {
    // Clean up previous connection
    wsConnRef.current?.close();
    wsConnRef.current = null;

    if (activePid === null || activePid === undefined) return;

    const conn = api.ptyConnectWs(activePid);
    wsConnRef.current = conn;

    const decoder = new TextDecoder();
    conn.onOutput((data) => {
      const term = xtermRef.current;
      if (!term) return;
      const writeData = typeof data === "string" ? data : decoder.decode(data as Uint8Array);
      writeDirect(term, writeData);
    });

    // Notify parent of WebSocket errors and closures
    conn.onError((msg) => {
      onWsErrorRef.current?.(msg);
    });
    conn.onClose((code, reason) => {
      onWsCloseRef.current?.(code, reason);
    });

    // Send initial resize so PTY matches xterm container size
    const term = xtermRef.current;
    if (term) {
      const ws = conn.ws;
      const sendResize = () => {
        if (ws.readyState === WebSocket.OPEN) {
          conn.resize(term.cols, term.rows);
        } else {
          setTimeout(sendResize, 100);
        }
      };
      sendResize();
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
    <div ref={containerRef} className="w-full h-full relative">
      <div ref={terminalRef} className="w-full h-full" />
    </div>
  );
}
