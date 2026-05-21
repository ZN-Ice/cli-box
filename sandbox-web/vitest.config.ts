import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environmentMatchGlobs: [
      ["src/__tests__/**/*.{test,spec}.{ts,tsx}", "jsdom"],
    ],
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.d.ts",
        "src/main.tsx",
        "src/__tests__/**",
        "src/themes/types.ts",
      ],
    },
    setupFiles: ["src/__tests__/setup.ts"],
  },
});
