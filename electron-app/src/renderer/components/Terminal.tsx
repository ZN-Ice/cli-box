import { useEffect, useRef, useCallback, forwardRef, useImperativeHandle } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { connectPty } from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  sandboxId: string;
  ptyPid: number;
  onReady?: (cols: number, rows: number) => void;
}

export interface SandboxTerminalHandle {
  captureToPng(): Promise<string>;
}

const SandboxTerminal = forwardRef<SandboxTerminalHandle, TerminalProps>(function SandboxTerminal(
  { sandboxId, ptyPid, onReady },
  ref
) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const fitFnRef = useRef<(() => void) | null>(null);
  const connRef = useRef<ReturnType<typeof connectPty> | null>(null);

  useImperativeHandle(ref, () => ({
    async captureToPng(): Promise<string> {
      const term = xtermRef.current;
      if (!term) throw new Error("Terminal not initialized");

      // Try canvas first (works for active/visible tabs)
      const canvasEl = term.element?.querySelector("canvas");
      if (canvasEl) {
        const dataUrl = canvasEl.toDataURL("image/png");
        return dataUrl.split(",")[1];
      }

      // Fallback: render xterm buffer to canvas (works for hidden/offscreen tabs)
      const cols = term.cols;
      const rows = term.rows;
      const fontSize = 13;
      const lineHeight = Math.ceil(fontSize * 1.4);
      const charWidth = Math.ceil(fontSize * 0.6);
      const canvas = document.createElement("canvas");
      canvas.width = cols * charWidth;
      canvas.height = rows * lineHeight;
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#1a1a1a";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.font = `${fontSize}px "SF Mono", "Menlo", "Monaco", monospace`;
      ctx.textBaseline = "top";

      const buffer = term.buffer.active;
      for (let y = 0; y < rows; y++) {
        const line = buffer.getLine(y);
        if (!line) continue;
        for (let x = 0; x < line.length; x++) {
          const char = line.getCell(x)?.getChars() || " ";
          const fg = line.getCell(x)?.getFgColor();
          // Use white for default text, or specific colors
          if (fg && fg !== 0) {
            ctx.fillStyle = `rgb(${(fg >> 16) & 0xff},${(fg >> 8) & 0xff},${fg & 0xff})`;
          } else {
            ctx.fillStyle = "#cccccc";
          }
          ctx.fillText(char, x * charWidth, y * lineHeight);
        }
      }
      return canvas.toDataURL("image/png").split(",")[1];
    },
  }), []);

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
});

export default SandboxTerminal;
