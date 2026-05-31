import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { connectPty } from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  sandboxId: string;
  ptyPid: number;
  onReady?: (cols: number, rows: number) => void;
}

export default function SandboxTerminal({ sandboxId, ptyPid, onReady }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const fitFnRef = useRef<(() => void) | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);

  // Initialize xterm.js
  useEffect(() => {
    if (!terminalRef.current) return;
    if (xtermRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 13,
      fontFamily: '"SF Mono", "Menlo", "Monaco", monospace',
      fontWeight: "400",
      fontWeightBold: "600",
      scrollback: 10000,
      lineHeight: 1.4,
      letterSpacing: 0,
      theme: {
        background: "#1a1a1a",
        foreground: "#cccccc",
        cursor: "#ffffff",
        cursorAccent: "#1a1a1a",
        selectionBackground: "#264f78",
        selectionForeground: "#ffffff",
        black: "#1a1a1a",
        red: "#ff6b6b",
        green: "#69db7c",
        yellow: "#ffd43b",
        blue: "#74c0fc",
        magenta: "#da77f2",
        cyan: "#66d9e8",
        white: "#cccccc",
        brightBlack: "#666666",
        brightRed: "#ff8787",
        brightGreen: "#8ce99a",
        brightYellow: "#ffe066",
        brightBlue: "#a5d8ff",
        brightMagenta: "#e599f7",
        brightCyan: "#99e9f2",
        brightWhite: "#ffffff",
      },
      allowTransparency: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);

    // Fit after a small delay to ensure DOM is ready
    requestAnimationFrame(() => {
      fitAddon.fit();
      onReady?.(term.cols, term.rows);

      // Re-fit after layout settles (flex layout may not be fully computed on first frame)
      setTimeout(() => {
        doFit();
      }, 100);
    });

    term.onData((data) => {
      connRef.current?.sendInput(data);
    });

    const doFit = () => {
      fitAddon.fit();
      connRef.current?.resize(term.cols, term.rows);
    };

    const handleResize = () => {
      doFit();
    };
    window.addEventListener("resize", handleResize);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;
    fitFnRef.current = doFit;

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Connect to PTY WebSocket
  useEffect(() => {
    connRef.current?.close();
    connRef.current = null;

    const conn = connectPty(sandboxId, ptyPid);
    connRef.current = conn;

    const decoder = new TextDecoder();
    conn.onOutput((data) => {
      const term = xtermRef.current;
      if (!term) return;
      const writeData = typeof data === "string" ? data : decoder.decode(data as Uint8Array);
      term.write(writeData);
    });

    // Send initial resize
    const term = xtermRef.current;
    if (term) {
      conn.resize(term.cols, term.rows);
    }

    return () => {
      conn.close();
      connRef.current = null;
    };
  }, [sandboxId, ptyPid]);

  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      requestAnimationFrame(() => fitFnRef.current?.());
    }
  }, []);

  return (
    <div ref={containerRef} className="terminal-container">
      <div ref={terminalRef} style={{ width: "100%", height: "100%" }} />
    </div>
  );
}
