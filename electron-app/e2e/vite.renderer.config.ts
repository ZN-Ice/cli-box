import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

const projectRoot = resolve(__dirname, "..");

export default defineConfig({
  root: resolve(projectRoot, "src/renderer"),
  plugins: [react()],
  resolve: {
    alias: {
      "@": resolve(projectRoot, "src/renderer"),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    open: false,
  },
});
