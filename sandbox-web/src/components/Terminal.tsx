import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import * as api from "../api";
import "xterm/css/xterm.css";

interface TerminalProps {
  /** Callback when terminal receives input */
  onInput?: (data: string) => void;
  /** Whether the terminal is connected to a PTY */
  connected?: boolean;
  /** The tracked PID of the active PTY process (null = none) */
  activePid?: number | null;
}

export default function SandboxTerminal({
  onInput,
  connected = false,
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
      fontSize: 14,
      fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
      theme: {
        background: "#0d1117",
        foreground: "#c9d1d9",
        cursor: "#58a6ff",
        selectionBackground: "#264f78",
        black: "#484f58",
        red: "#ff7b72",
        green: "#3fb950",
        yellow: "#d29922",
        blue: "#58a6ff",
        magenta: "#bc8cff",
        cyan: "#39c5d6",
        white: "#b1bac4",
        brightBlack: "#6e7681",
        brightRed: "#ffa198",
        brightGreen: "#56d364",
        brightYellow: "#e3b341",
        brightBlue: "#79c0ff",
        brightMagenta: "#d2a8ff",
        brightCyan: "#56d4dd",
        brightWhite: "#f0f6fc",
      },
      allowProposedApi: true,
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

  // PTY output polling — runs while activePid is set
  useEffect(() => {
    // Clear any existing poll
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
        // Process may have exited; stop polling
        if (pollRef.current) {
          clearInterval(pollRef.current);
          pollRef.current = null;
        }
      }
    }, 200);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [activePid]);

  // Refit on window resize
  const containerRef = useCallback((node: HTMLDivElement | null) => {
    if (node) {
      // Trigger fit after layout
      requestAnimationFrame(() => fitAddonRef.current?.fit());
    }
  }, []);

  return (
    <div className="flex flex-col h-full" ref={containerRef}>
      <div className="flex items-center justify-between px-3 py-1.5 bg-gray-800 border-b border-gray-700">
        <span className="text-xs text-gray-400 font-medium">Terminal</span>
        <span
          className={`inline-block w-2 h-2 rounded-full ${connected ? "bg-green-500" : "bg-gray-500"}`}
          title={connected ? "Connected" : "Disconnected"}
        />
      </div>
      <div ref={terminalRef} className="flex-1" />
    </div>
  );
}
