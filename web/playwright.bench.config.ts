import { defineConfig } from "@playwright/test";
import base from "./playwright.config";

// Measurements, not tests: run on demand with `pnpm bench:transport`.
// Needs a production bundle (`pnpm build`) and generated drawings
// (see benches/generated-drawings/README.md).
export default defineConfig({
  ...base,
  testDir: "bench",
  testMatch: "**/*.bench.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 900_000,
  reporter: "list",
  // Rendering probes run in Chromium; `bench:persistence` runs in both.
  projects: [
    { name: "firefox", use: { browserName: "firefox" } },
    {
      name: "chromium",
      use: {
        launchOptions: {
          args: [
            "--enable-unsafe-swiftshader",
            "--ignore-gpu-blocklist",
            "--enable-precise-memory-info",
          ],
        },
      },
    },
  ],
});
