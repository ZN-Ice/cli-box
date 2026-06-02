import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: "http://localhost:5173",
    screenshot: "on",
  },
  webServer: {
    command: "npx electron-vite dev --rendererOnly",
    port: 5173,
    reuseExistingServer: !process.env.CI,
    timeout: 15000,
  },
});
