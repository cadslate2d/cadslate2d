import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // Relative base so the static bundle works from any path (ADR-0004).
  base: "./",
  plugins: [react()],
  build: { target: "es2022" },
  test: { include: ["src/**/*.test.ts", "src/**/*.test.tsx"], passWithNoTests: true },
});
