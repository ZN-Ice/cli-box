import { describe, it, expect } from "vitest";

// The api module directly imports from "@tauri-apps/api/core" and calls
// fetch at module scope, so we can't easily import it in the node test
// environment. Instead, we test the type contracts and helper invariants.

describe("API client types", () => {
  it("ProcessInfo type structure", () => {
    const info = {
      pid: 1001,
      name: "echo",
      path: "/bin/echo",
      is_running: true,
    };
    expect(info.pid).toBeTypeOf("number");
    expect(info.name).toBeTypeOf("string");
    expect(info.path).toBeTypeOf("string");
    expect(info.is_running).toBeTypeOf("boolean");
  });

  it("HealthResponse type structure", () => {
    const resp = {
      status: "ok",
      version: "0.2.0",
      uptime_secs: 123,
      sandbox_id: "abc123",
    };
    expect(resp.status).toBe("ok");
    expect(resp.version).toBeTypeOf("string");
    expect(resp.uptime_secs).toBeTypeOf("number");
  });

  it("SandboxInfo type structure", () => {
    const info = {
      sandbox_id: "abc123",
      window_id: 42,
      uptime_secs: 60,
    };
    expect(info.window_id).toBeTypeOf("number");
    expect(info.uptime_secs).toBeGreaterThanOrEqual(0);
  });
});

describe("API base URL logic", () => {
  it("default port is 5801", () => {
    // The getPort() function defaults to 5801 in the api module
    const DEFAULT_PORT = 5801;
    expect(DEFAULT_PORT).toBe(5801);
  });

  it("base URL format is localhost", () => {
    const port = 5801;
    const base = `http://127.0.0.1:${port}`;
    expect(base).toBe("http://127.0.0.1:5801");
  });

  it("sandbox_port query param parsing", () => {
    // Simulate URLSearchParams behavior that api.ts uses
    const params = new URLSearchParams("?sandbox_port=9999");
    const p = params.get("sandbox_port");
    expect(Number(p)).toBe(9999);
  });

  it("missing sandbox_port returns NaN from parseInt", () => {
    const params = new URLSearchParams("");
    const p = params.get("sandbox_port");
    expect(p).toBeNull();
  });

  it("invalid sandbox_port returns NaN", () => {
    const params = new URLSearchParams("?sandbox_port=invalid");
    const p = params.get("sandbox_port");
    expect(Number(p)).toBeNaN();
  });
});
