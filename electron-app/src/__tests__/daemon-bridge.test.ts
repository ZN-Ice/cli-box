// @vitest-environment node
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { existsSync, readFileSync } from "fs";

// Mock fs before importing daemon-bridge
vi.mock("fs", async () => {
  const actual = await vi.importActual<typeof import("fs")>("fs");
  return {
    ...actual,
    existsSync: vi.fn(),
    readFileSync: vi.fn(),
  };
});

// Mock electron app module
vi.mock("electron", () => ({
  app: {
    getPath: () => "/tmp/test",
  },
}));

import { waitForDaemon } from "../main/daemon-bridge";

describe("waitForDaemon", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Stub process.kill so the test's fake PID appears alive
    vi.spyOn(process, "kill").mockImplementation(() => true);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("returns port when daemon.json appears within 1s", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First call: no daemon.json, second call: daemon.json exists
    mockExists.mockReturnValueOnce(false).mockReturnValueOnce(true);
    mockRead.mockReturnValueOnce(
      JSON.stringify({ port: 15801, pid: 12345, started_at: "2026-01-01" })
    );

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(1000);
    const port = await portPromise;

    expect(port).toBe(15801);
  });

  it("keeps polling until daemon appears", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First 3 calls: no daemon, 4th call: exists.
    // mockReturnValueOnce is FIFO, so queue each value in order.
    mockExists
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(true);
    mockRead.mockReturnValueOnce(
      JSON.stringify({ port: 15801, pid: 12345, started_at: "2026-01-01" })
    );

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(3000);
    const port = await portPromise;

    expect(port).toBe(15801);
    expect(mockExists).toHaveBeenCalledTimes(4);
  });

  it("skips invalid daemon.json content and retries", async () => {
    const mockExists = vi.mocked(existsSync);
    const mockRead = vi.mocked(readFileSync);

    // First: invalid JSON, then: valid
    mockExists.mockReturnValue(true);
    mockRead
      .mockReturnValueOnce("invalid json")
      .mockReturnValueOnce(JSON.stringify({ port: 15801, pid: 12345, started_at: "" }));

    const portPromise = waitForDaemon();
    await vi.advanceTimersByTimeAsync(2000);
    const port = await portPromise;

    expect(port).toBe(15801);
  });
});
