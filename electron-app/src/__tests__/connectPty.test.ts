import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { connectPty, setDaemonPort } from "../renderer/api";
import { MockWebSocket, installMockWebSocket } from "./mocks/websocket";

// Capture the most recently constructed MockWebSocket instance
let lastWs: MockWebSocket | null = null;
const OriginalWebSocket = (globalThis as any).WebSocket;

beforeEach(() => {
  installMockWebSocket();
  // Wrap the mock so we can capture instances
  const OrigMock = (globalThis as any).WebSocket as typeof MockWebSocket;
  (globalThis as any).WebSocket = class extends OrigMock {
    constructor(url: string) {
      super(url);
      lastWs = this;
    }
  };
  // Copy static constants
  (globalThis as any).WebSocket.CONNECTING = MockWebSocket.CONNECTING;
  (globalThis as any).WebSocket.OPEN = MockWebSocket.OPEN;
  (globalThis as any).WebSocket.CLOSING = MockWebSocket.CLOSING;
  (globalThis as any).WebSocket.CLOSED = MockWebSocket.CLOSED;

  lastWs = null;
  setDaemonPort(15801);
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  (globalThis as any).WebSocket = OriginalWebSocket;
});

describe("connectPty", () => {
  it("does not create WebSocket until onOutput is called", () => {
    const conn = connectPty("abc123", 42);
    expect(lastWs).toBeNull();
    conn.close();
  });

  it("creates WebSocket when onOutput registers first listener", () => {
    const conn = connectPty("abc123", 42);
    conn.onOutput(() => {});
    expect(lastWs).not.toBeNull();
    expect(lastWs!.url).toBe("ws://127.0.0.1:15801/box/abc123/pty/ws/42");
    conn.close();
  });

  it("delivers string messages to onOutput callback after WS opens", () => {
    const conn = connectPty("abc123", 42);
    const received: (string | Uint8Array)[] = [];
    conn.onOutput((data: string | Uint8Array) => received.push(data));

    // Advance timers to trigger MockWebSocket's async open
    vi.advanceTimersByTime(1);
    expect(lastWs!.readyState).toBe(MockWebSocket.OPEN);

    // Simulate incoming string message
    lastWs!.simulateMessage("hello world");
    expect(received).toEqual(["hello world"]);
    conn.close();
  });

  it("queues resize and sends on WebSocket open", () => {
    const conn = connectPty("abc123", 42);

    // Call resize before WS is created
    conn.resize(120, 40);
    expect(lastWs).toBeNull();

    // Register listener to create WS
    conn.onOutput(() => {});
    expect(lastWs).not.toBeNull();
    // WS is not yet open (readyState = CONNECTING)
    expect(lastWs!.getSent()).toEqual([]);

    // Advance timers to trigger open
    vi.advanceTimersByTime(1);

    // The pending resize should have been sent on open
    const sent = lastWs!.getSent();
    expect(sent).toHaveLength(1);
    expect(JSON.parse(sent[0] as string)).toEqual({ type: "resize", cols: 120, rows: 40 });
    conn.close();
  });

  it("sends resize immediately if WS already open", () => {
    const conn = connectPty("abc123", 42);
    conn.onOutput(() => {});

    // Advance timers to open WS
    vi.advanceTimersByTime(1);
    expect(lastWs!.readyState).toBe(MockWebSocket.OPEN);

    // Now resize should be sent immediately
    conn.resize(80, 24);
    const sent = lastWs!.getSent();
    expect(sent).toHaveLength(1);
    expect(JSON.parse(sent[0] as string)).toEqual({ type: "resize", cols: 80, rows: 24 });
    conn.close();
  });

  it("sendInput is a no-op before WS opens", () => {
    const conn = connectPty("abc123", 42);

    // sendInput before any WS exists — should not throw
    conn.sendInput("data1");

    // Register listener to create WS but do NOT advance timers (WS still CONNECTING)
    conn.onOutput(() => {});
    conn.sendInput("data2");

    expect(lastWs!.getSent()).toEqual([]);
    conn.close();
  });

  it("sendInput works after WS opens", () => {
    const conn = connectPty("abc123", 42);
    conn.onOutput(() => {});
    vi.advanceTimersByTime(1);

    conn.sendInput("typed text");
    expect(lastWs!.getSent()).toEqual(["typed text"]);
    conn.close();
  });

  it("close is safe before WS is created", () => {
    const conn = connectPty("abc123", 42);
    // Should not throw
    expect(() => conn.close()).not.toThrow();
  });

  it("unsubscribe removes callback", () => {
    const conn = connectPty("abc123", 42);
    const received: string[] = [];
    const unsub = conn.onOutput((data: string | Uint8Array) => {
      if (typeof data === "string") received.push(data);
    });

    vi.advanceTimersByTime(1);
    lastWs!.simulateMessage("before");
    expect(received).toEqual(["before"]);

    unsub();
    lastWs!.simulateMessage("after");
    // "after" should not be received because callback was removed
    expect(received).toEqual(["before"]);
    conn.close();
  });

  it("handles binary ArrayBuffer messages", () => {
    const conn = connectPty("abc123", 42);
    const received: (string | Uint8Array)[] = [];
    conn.onOutput((data: string | Uint8Array) => received.push(data));

    vi.advanceTimersByTime(1);

    const buf = new ArrayBuffer(4);
    new Uint8Array(buf).set([1, 2, 3, 4]);
    lastWs!.simulateMessage(buf);

    expect(received).toHaveLength(1);
    expect(received[0]).toBeInstanceOf(Uint8Array);
    expect([...(received[0] as Uint8Array)]).toEqual([1, 2, 3, 4]);
    conn.close();
  });
});
