import { defineConfig, devices } from "@playwright/test";

const PORT = 4173;
/** A second copy of the bundle under a sub-path, to prove it is base-path independent. */
export const NESTED_URL = "http://localhost:4174/nested/app/";

const BROWSERS = [
  {
    name: "chromium",
    use: {
      ...devices["Desktop Chrome"],
      // Software WebGL2 so the suite runs on machines and CI runners without a GPU.
      launchOptions: { args: ["--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
    },
  },
  {
    name: "firefox",
    use: {
      ...devices["Desktop Firefox"],
      launchOptions: {
        firefoxUserPrefs: {
          "webgl.force-enabled": true,
          "webgl.disabled": false,
          "gfx.webrender.software": true,
        },
      },
    },
  },
  // Needs system libraries: `playwright install --with-deps webkit` (root).
  { name: "webkit", use: { ...devices["Desktop Safari"] } },
];

// Which browsers to run: E2E_BROWSERS=chromium,firefox,webkit. The default is
// what works on a developer machine without root. CI runs Chromium only:
// headless Firefox on the hosted runner provides no WebGL2 (tracked in the
// cross-browser issue).
const requested = (process.env.E2E_BROWSERS ?? "chromium,firefox").split(",");

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
  projects: BROWSERS.filter((project) => requested.includes(project.name)),
  webServer: [
    {
      command: `node e2e/static-server.mjs dist ${PORT}`,
      url: `http://localhost:${PORT}/`,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: "node e2e/static-server.mjs dist 4174 /nested/app/",
      url: NESTED_URL,
      reuseExistingServer: !process.env.CI,
    },
  ],
});
