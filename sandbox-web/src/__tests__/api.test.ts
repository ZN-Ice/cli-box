/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

// Helper to create a mock Response
function mockResponse(
  ok: boolean,
  status: number,
  body: string | Blob,
  contentType = "application/json",
) {
  const resp = {
    ok,
    status,
    headers: new Headers({ "content-type": contentType }),
    json: () => Promise.resolve(JSON.parse(body as string)),
    text: () => Promise.resolve(body as string),
    blob: () => Promise.resolve(body instanceof Blob ? body : new Blob([body])),
    clone() {
      return resp;
    },
  };
  return resp as unknown as Response;
}

// Import after mocks are set up — api.ts uses fetch at module level via functions
import * as api from "../api";

describe("API client", () => {
  beforeEach(() => {
    mockFetch.mockReset();
    // Reset URL to no search params
    window.history.replaceState({}, "", "/");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ── Port resolution ───────────────────────────────────

  describe("port resolution", () => {
    it("uses default port 5801 when no query param", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"status":"ok","version":"0.2.0","uptime_secs":0,"sandbox_id":null}',
        ),
      );
      await api.health();
      expect(mockFetch).toHaveBeenCalledWith("http://127.0.0.1:5801/health");
    });

    it("uses sandbox_port query parameter when present", async () => {
      window.history.replaceState({}, "", "/?sandbox_port=9999");
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"status":"ok","version":"0.2.0","uptime_secs":0,"sandbox_id":null}',
        ),
      );
      await api.health();
      expect(mockFetch).toHaveBeenCalledWith("http://127.0.0.1:9999/health");
    });
  });

  // ── Health ────────────────────────────────────────────

  describe("health()", () => {
    it("returns HealthResponse on success", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"status":"ok","version":"0.2.0","uptime_secs":42,"sandbox_id":"abc123"}',
        ),
      );
      const result = await api.health();
      expect(result.status).toBe("ok");
      expect(result.version).toBe("0.2.0");
      expect(result.uptime_secs).toBe(42);
      expect(result.sandbox_id).toBe("abc123");
    });
  });

  // ── Sandbox Info ──────────────────────────────────────

  describe("sandboxInfo()", () => {
    it("returns SandboxInfo on success", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"sandbox_id":"abc123","window_id":42,"uptime_secs":60}',
        ),
      );
      const result = await api.sandboxInfo();
      expect(result.sandbox_id).toBe("abc123");
      expect(result.window_id).toBe(42);
      expect(result.uptime_secs).toBe(60);
    });
  });

  // ── request() helper error handling ──────────────────

  describe("request() error handling", () => {
    it("throws on non-ok response with JSON error", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(false, 400, '{"error":"bad request"}'),
      );
      await expect(api.click(0, 0)).rejects.toThrow("HTTP 400: bad request");
    });

    it("throws on non-ok response with plain text body", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(false, 500, "internal server error"),
      );
      await expect(api.click(0, 0)).rejects.toThrow(
        "HTTP 500: internal server error",
      );
    });

    it("throws on non-ok response with invalid JSON body", async () => {
      mockFetch.mockResolvedValueOnce(mockResponse(false, 502, "{invalid"));
      await expect(api.click(0, 0)).rejects.toThrow("HTTP 502");
    });
  });

  // ── Screenshot ────────────────────────────────────────

  describe("takeScreenshot()", () => {
    it("returns blob URL on success", async () => {
      const blob = new Blob(["fake-png-data"], { type: "image/png" });
      mockFetch.mockResolvedValueOnce(mockResponse(true, 200, blob));
      const result = await api.takeScreenshot();
      expect(result).toMatch(/^blob:/);
    });

    it("throws on failure", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(false, 500, "capture failed"),
      );
      await expect(api.takeScreenshot()).rejects.toThrow("Screenshot failed");
    });

    it("throws when sandbox window is not available (400)", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(false, 400, "Sandbox window not available"),
      );
      await expect(api.takeScreenshot()).rejects.toThrow("Screenshot failed");
    });
  });

  describe("takeScreenshotRegion()", () => {
    it("returns blob URL on success", async () => {
      const blob = new Blob(["fake-png-data"], { type: "image/png" });
      mockFetch.mockResolvedValueOnce(mockResponse(true, 200, blob));
      const result = await api.takeScreenshotRegion(10, 20, 100, 200);
      expect(result).toMatch(/^blob:/);
      expect(mockFetch).toHaveBeenCalledWith(
        "http://127.0.0.1:5801/screenshot/region?x=10&y=20&width=100&height=200",
      );
    });

    it("throws on failure", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(false, 500, "region failed"),
      );
      await expect(api.takeScreenshotRegion(0, 0, 1, 1)).rejects.toThrow(
        "Screenshot region failed",
      );
    });
  });

  // ── Input methods ─────────────────────────────────────

  describe("click()", () => {
    it("sends POST with click data", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"clicked":{"x":100,"y":200,"button":"left"}}',
        ),
      );
      await api.click(100, 200, "left");
      const call = mockFetch.mock.calls[0];
      expect(call[1]?.method).toBe("POST");
      expect(JSON.parse(call[1]?.body as string)).toEqual({
        x: 100,
        y: 200,
        button: "left",
      });
    });

    it("defaults to left button", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"clicked":{"x":0,"y":0,"button":"left"}}'),
      );
      await api.click(0, 0);
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body.button).toBe("left");
    });
  });

  describe("typeText()", () => {
    it("sends POST with text", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"typed":"hello"}'),
      );
      await api.typeText("hello");
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body.text).toBe("hello");
    });
  });

  describe("pressKey()", () => {
    it("sends POST with key and modifiers", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(
          true,
          200,
          '{"pressed":{"key":"return","modifiers":["cmd"]}}',
        ),
      );
      await api.pressKey("return", ["cmd"]);
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body.key).toBe("return");
      expect(body.modifiers).toEqual(["cmd"]);
    });

    it("defaults to empty modifiers", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"pressed":{"key":"escape","modifiers":[]}}'),
      );
      await api.pressKey("escape");
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body.modifiers).toEqual([]);
    });
  });

  describe("scroll()", () => {
    it("sends POST with scroll data", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"scrolled":true}'),
      );
      await api.scroll(50, 50, "down", 5);
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body).toEqual({ x: 50, y: 50, direction: "down", amount: 5 });
    });
  });

  describe("drag()", () => {
    it("sends POST with drag coordinates", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"dragged":true}'),
      );
      await api.drag(0, 0, 100, 200);
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body).toEqual({
        from_x: 0,
        from_y: 0,
        to_x: 100,
        to_y: 200,
      });
    });
  });

  // ── Process management ────────────────────────────────

  describe("spawnApp()", () => {
    it("returns ProcessInfo on success", async () => {
      const info = {
        pid: 1001,
        name: "Safari",
        path: "/Apps/Safari.app",
        is_running: true,
      };
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, JSON.stringify(info)),
      );
      const result = await api.spawnApp("/Apps/Safari.app");
      expect(result.pid).toBe(1001);
      expect(result.name).toBe("Safari");
    });

    it("throws on failure", async () => {
      mockFetch.mockResolvedValueOnce(mockResponse(false, 500, "not found"));
      await expect(api.spawnApp("/bad")).rejects.toThrow("spawnApp failed");
    });
  });

  describe("spawnCli()", () => {
    it("returns ProcessInfo on success", async () => {
      const info = { pid: 1002, name: "echo", path: null, is_running: true };
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, JSON.stringify(info)),
      );
      const result = await api.spawnCli("echo", ["hello"]);
      expect(result.pid).toBe(1002);
      const call = mockFetch.mock.calls[0];
      expect(call[1]?.method).toBe("POST");
      const body = JSON.parse(call[1]?.body as string);
      expect(body.command).toBe("echo");
      expect(body.args).toEqual(["hello"]);
    });

    it("throws on failure", async () => {
      mockFetch.mockResolvedValueOnce(mockResponse(false, 500, "failed"));
      await expect(api.spawnCli("bad", [])).rejects.toThrow("spawnCli failed");
    });
  });

  describe("listProcesses()", () => {
    it("returns ProcessInfo array", async () => {
      const procs = [
        { pid: 1, name: "a", path: null, is_running: true },
        { pid: 2, name: "b", path: null, is_running: false },
      ];
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, JSON.stringify(procs)),
      );
      const result = await api.listProcesses();
      expect(result).toHaveLength(2);
      expect(result[0].pid).toBe(1);
    });
  });

  describe("killProcess()", () => {
    it("sends POST with pid", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"killed":123}'),
      );
      await api.killProcess(123);
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body.pid).toBe(123);
    });
  });

  // ── PTY ───────────────────────────────────────────────

  describe("ptyWrite()", () => {
    it("sends POST with pid and data", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"written":true}'),
      );
      await api.ptyWrite(42, "hello\n");
      const body = JSON.parse(mockFetch.mock.calls[0][1]?.body as string);
      expect(body).toEqual({ pid: 42, data: "hello\n" });
    });
  });

  describe("ptyRead()", () => {
    it("returns output on success", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"output":"hello world"}'),
      );
      const result = await api.ptyRead(42);
      expect(result.output).toBe("hello world");
    });

    it("returns null output", async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, '{"output":null}'),
      );
      const result = await api.ptyRead(42);
      expect(result.output).toBeNull();
    });
  });

  // ── Windows ───────────────────────────────────────────

  describe("listWindows()", () => {
    it("returns window array", async () => {
      const windows = [
        [42, "Terminal"],
        [43, "Safari"],
      ];
      mockFetch.mockResolvedValueOnce(
        mockResponse(true, 200, JSON.stringify(windows)),
      );
      const result = await api.listWindows();
      expect(result).toHaveLength(2);
      expect(result[0]).toEqual([42, "Terminal"]);
    });
  });
});
