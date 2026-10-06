import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  lines,
  openApp,
  prompt,
  rendererStats,
  trackingPixels,
} from "./support";

const tracking = (page: Page) => page.getByTestId("tracking");
const coordinates = (page: Page) => page.getByTestId("coordinates");
const toggle = (page: Page, name: string) => page.getByRole("button", { name });

test.describe("ORTHO", () => {
  test("constrains clicked points to horizontal or vertical", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F8");
    await enter(page, "L", "0,0");

    await hoverWorld(page, canvas, 80, 20);
    await expect(tracking(page)).toHaveText("Ortho: 80.0000 < 0°");
    await expect(coordinates(page)).toHaveText("80.0000, 0.0000");

    await clickWorld(page, canvas, 80, 20);
    await clickWorld(page, canvas, 95, 70);
    await enter(page, "");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(lines(file.text)).toContainEqual([0, 0, 80, 0]);
    expect(lines(file.text)).toContainEqual([80, 0, 80, 70]);
  });

  test("typed coordinates are not constrained, and F8 does not cancel the command", async ({
    page,
  }) => {
    await openApp(page);
    await enter(page, "L", "0,0");
    await page.keyboard.press("F8");
    await expect(prompt(page)).toHaveText("Specify next point or [Undo]:");
    await enter(page, "30,40", "");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain(" 11\n30.0\n 21\n40.0\n");
  });
});

test.describe("POLAR", () => {
  test("locks onto a tracked direction, shows it, and releases", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F10");
    await enter(page, "L", "0,0");
    const before = await rendererStats(page);

    await hoverWorld(page, canvas, 80, 5);
    await expect(tracking(page)).toHaveText("Polar: 80.0000 < 0°");
    await expect(coordinates(page)).toHaveText("80.0000, 0.0000");
    expect(await trackingPixels(canvas)).toBeGreaterThan(100);

    await hoverWorld(page, canvas, 80, 40);
    await expect(tracking(page)).toHaveCount(0);
    expect(await trackingPixels(canvas)).toBe(0);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("a typed distance runs along the tracked angle", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F10");
    await enter(page, "POLARANG", "45", "L", "0,0");
    await hoverWorld(page, canvas, 72, 69);
    await expect(tracking(page)).toContainText("< 45°");
    await enter(page, "100", "");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toMatch(/ 11\n70\.7106781186\d*\n 21\n70\.7106781186\d*\n/);
  });

  test("ORTHO and POLAR switch each other off", async ({ page }) => {
    await openApp(page);
    await page.keyboard.press("F10");
    await expect(toggle(page, "POLAR")).toHaveAttribute("aria-pressed", "true");

    await toggle(page, "ORTHO").click();
    await expect(toggle(page, "ORTHO")).toHaveAttribute("aria-pressed", "true");
    await expect(toggle(page, "POLAR")).toHaveAttribute("aria-pressed", "false");

    await page.keyboard.press("F10");
    await expect(toggle(page, "POLAR")).toHaveAttribute("aria-pressed", "true");
    await expect(toggle(page, "ORTHO")).toHaveAttribute("aria-pressed", "false");
  });
});
