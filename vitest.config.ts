import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    globals: true,
    include: ["src/**/*.{test,spec}.{ts,tsx}", "tools/**/*.{test,spec}.{ts,tsx,mjs}"],
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
});
