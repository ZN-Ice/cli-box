import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("screenshot WebSocket reconnection", () => {
  let wsInstances: MockWebSocket[];

  class MockWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;

    readyState = MockWebSocket.CONNECTING;
    onopen: (() => void) | null = null;
    onclose: (() => void) | null = null;
    onerror: ((err: any) => void) | null = null;
    onmessage: ((event: { data: string }) => void) | null = null;
    url: string;

    constructor(url: string) {
      this.url = url;
      wsInstances.push(this);
      setTimeout(() => {
        this.readyState = MockWebSocket.OPEN;
        this.onopen?.();
      }, 0);
    }

    send(_data: string) {}
    close() {
      this.readyState = MockWebSocket.CLOSED;
      this.onclose?.();
    }
  }

  beforeEach(() => {
    wsInstances = [];
    vi.useFakeTimers();
    (globalThis as any).WebSocket = MockWebSocket;
  });

  afterEach(() => {
    vi.useRealTimers();
    delete (globalThis as any).WebSocket;
  });

  it("creates WebSocket on mount", () => {
    const connect = () => new MockWebSocket("ws://127.0.0.1:15801/screenshot/ws");
    connect();
    expect(wsInstances).toHaveLength(1);
    expect(wsInstances[0].url).toBe("ws://127.0.0.1:15801/screenshot/ws");
  });

  it("reconnects after close with delay", () => {
    let reconnectDelay = 1000;
    const MAX_RECONNECT_DELAY = 30000;

    const connect = () => {
      const ws = new MockWebSocket("ws://127.0.0.1:15801/screenshot/ws");
      ws.onclose = () => {
        setTimeout(() => {
          reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
          connect();
        }, reconnectDelay);
      };
      return ws;
    };

    connect();
    expect(wsInstances).toHaveLength(1);

    wsInstances[0].close();
    expect(wsInstances).toHaveLength(1);

    vi.advanceTimersByTime(1000);
    expect(wsInstances).toHaveLength(2);
  });

  it("exponential backoff increases delay", () => {
    let reconnectDelay = 1000;
    const MAX_RECONNECT_DELAY = 30000;
    const delays: number[] = [];

    const connect = () => {
      const ws = new MockWebSocket("ws://127.0.0.1:15801/screenshot/ws");
      ws.onclose = () => {
        delays.push(reconnectDelay);
        setTimeout(() => {
          reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
          connect();
        }, reconnectDelay);
      };
      return ws;
    };

    connect();

    wsInstances[0].close();
    vi.advanceTimersByTime(1000);
    expect(delays).toEqual([1000]);

    wsInstances[1].close();
    vi.advanceTimersByTime(2000);
    expect(delays).toEqual([1000, 2000]);

    wsInstances[2].close();
    vi.advanceTimersByTime(4000);
    expect(delays).toEqual([1000, 2000, 4000]);
  });

  it("resets backoff on successful connection", () => {
    let reconnectDelay = 1000;
    const MAX_RECONNECT_DELAY = 30000;

    const connect = () => {
      const ws = new MockWebSocket("ws://127.0.0.1:15801/screenshot/ws");
      ws.onopen = () => {
        reconnectDelay = 1000;
      };
      ws.onclose = () => {
        setTimeout(() => {
          reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
          connect();
        }, reconnectDelay);
      };
      return ws;
    };

    connect();
    vi.advanceTimersByTime(1);

    wsInstances[0].close();
    vi.advanceTimersByTime(1000);
    vi.advanceTimersByTime(1);

    wsInstances[1].close();
    vi.advanceTimersByTime(1000);
    expect(wsInstances).toHaveLength(3);
  });
});
