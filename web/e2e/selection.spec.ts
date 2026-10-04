import { expect, test } from "@playwright/test";
import { centerOf, enter, highlightPixels, openApp, rendererStats, settle } from "./support";

test.describe("selection", () => {
  test("clicking a line highlights it; Escape clears it", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "-150,0", "150,0", "");
    expect(await highlightPixels(canvas)).toBe(0);

    await page.mouse.click(x + 40, y + 2);
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(200);

    await page.keyboard.press("Escape");
    await settle(page);
    expect(await highlightPixels(canvas)).toBe(0);
  });

  test("clicking empty space selects nothing", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "-150,0", "150,0", "");
    await page.mouse.click(x, y + 60);
    await settle(page);
    expect(await highlightPixels(canvas)).toBe(0);
  });

  test("selecting does not re-upload committed geometry", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "-150,0", "150,0", "");
    const before = await rendererStats(page);

    await page.mouse.click(x, y);
    await page.keyboard.press("Escape");
    await page.mouse.click(x, y);
    await settle(page);

    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("the highlight follows undo", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "-150,0", "150,0", "");
    await page.mouse.click(x, y);
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(200);

    await page.keyboard.press("Control+z");
    await settle(page);
    expect(await highlightPixels(canvas)).toBe(0);
  });
});
