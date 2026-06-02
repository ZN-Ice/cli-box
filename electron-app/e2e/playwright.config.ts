import { defineConfig } from "@playwright/test";
import { resolve } from "path";

export default defineConfig({
  testDir: ".",
  timeout: 30000,
  retries: process.env.CI ? 2 : 0,
  // CI runs on Linux, local dev on macOS — snapshots are platform-specific.
  // On CI, auto-accept missing snapshots (first run creates baseline).
  updateSnapshots: process.env.CI ? "missing" : "missing",
  use: {
    baseURL: "http://localhost:5173",
    screenshot: "on",
  },
  webServer: {
    command: "npx vite --config e2e/vite.renderer.config.ts",
    cwd: resolve(__dirname, ".."),
    port: 5173,
    reuseExistingServer: !process.env.CI,
    timeout: 15000,
  },
});
