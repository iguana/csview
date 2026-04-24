import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

export default defineConfig({
  plugins: [react()],
  root: ".",
  clearScreen: false,
  server: { port: 1523, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    outDir: "dist-ai",
    target: "safari13",
    rollupOptions: {
      input: resolve(__dirname, "index-ai.html"),
    },
  },
  resolve: {
    alias: {
      "@shared": resolve(__dirname, "src"),
      "@ai": resolve(__dirname, "src-ai"),
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    include: ["src-ai/**/*.test.{ts,tsx}", "src/**/*.test.{ts,tsx}"],
  },
});
