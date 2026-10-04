import { defineConfig, devices } from "@playwright/test";

const PORT = 4173;

// End-to-end tests run against the production bundle in web/dist, served by a
// plain static file server. Build first: `pnpm build`.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}/`,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // Software WebGL2 so the suite runs on machines and CI runners without a GPU.
        launchOptions: { args: ["--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
      },
    },
  ],
  webServer: {
    command: `node e2e/static-server.mjs dist ${PORT}`,
    url: `http://localhost:${PORT}/`,
    reuseExistingServer: !process.env.CI,
  },
});
