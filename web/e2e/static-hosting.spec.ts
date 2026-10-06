// Deployment is a folder of static files (DESIGN §6, ADR-0004).
import { expect, test } from "@playwright/test";
import { NESTED_URL } from "../playwright.config";
import {
  downloadFrom,
  enter,
  logLines,
  prompt,
  recordRequests,
  withoutFilePickers,
} from "./support";

test("the bundle works from any base path", async ({ page }) => {
  const requests = recordRequests(page);
  await page.goto(NESTED_URL);
  await expect(page.locator("header")).toHaveAttribute("data-core-version", /^\d+\.\d+\.\d+$/);

  await enter(page, "L", "0,0", "10,10", "");
  await expect(prompt(page)).toHaveText("Command:");
  // Every asset was requested under the nested path: no absolute URLs in the bundle.
  expect(requests.length).toBeGreaterThan(2);
  for (const request of requests)
    expect(request.url.startsWith(NESTED_URL), request.url).toBe(true);
});

test("once loaded, the app works with no network at all", async ({ page, context }) => {
  // Chromium has the save picker, which a headless test cannot answer: downloads here.
  await withoutFilePickers(page);
  await page.goto("./");
  await expect(page.locator("header")).toHaveAttribute("data-core-version", /.+/);

  await context.setOffline(true);

  await enter(page, "L", "0,0", "100,0", "");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(file.text).toContain("LINE");
  await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");
});

test("the app talks to no third party and sends nothing anywhere", async ({ page }) => {
  const requests = recordRequests(page);
  await page.goto("./");
  await expect(page.locator("header")).toHaveAttribute("data-core-version", /.+/);
  await enter(page, "L", "0,0", "100,0", "");
  await page.waitForTimeout(300);

  const origin = new URL(page.url()).origin;
  expect(requests.length).toBeGreaterThan(0);
  for (const request of requests) {
    expect(new URL(request.url).origin, request.url).toBe(origin);
    expect(request.method, request.url).toBe("GET");
  }
});
