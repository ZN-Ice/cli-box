import { defineConfig } from "vitest/config";
import { resolve } from "path";

export default defineConfig({
  test: {
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    environment: "jsdom",
    setupFiles: ["src/__tests__/setup.ts"],
    globals: true,
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "src/renderer"),
    },
  },
});
