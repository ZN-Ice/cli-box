import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("daemon port polling", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("polls getDaemonPort every 1s when port is 0", async () => {
    const mockGetPort = vi.fn();
    mockGetPort.mockResolvedValue(0);

    let cancelled = false;
    async function poll() {
      while (!cancelled) {
        const port = await mockGetPort();
        if (port > 0) return;
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    const pollPromise = poll();
    await vi.advanceTimersByTimeAsync(3000);
    cancelled = true;
    // Allow the in-flight setTimeout to resolve so the loop can exit
    await vi.advanceTimersByTimeAsync(1000);
    await pollPromise;

    expect(mockGetPort).toHaveBeenCalledTimes(4); // initial + 3 ticks
  });
});
