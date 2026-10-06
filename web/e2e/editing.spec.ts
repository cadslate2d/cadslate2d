import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  circles,
  crossingPixels,
  downloadFrom,
  enter,
  entities,
  highlightPixels,
  lines,
  litPixels,
  logLines,
  openApp,
  prompt,
  rendererStats,
  settle,
} from "./support";

/** A line across the middle and a circle above it. */
async function scene(page: Page): Promise<{ canvas: Locator; x: number; y: number }> {
  const canvas = await openApp(page);
  await enter(page, "L", "-100,0", "100,0", "", "C", "0,80", "30");
  const { x, y } = await centerOf(canvas);
  return { canvas, x, y };
}

test.describe("selection sets", () => {
  test("clicks accumulate and Shift+click removes", async ({ page }) => {
    const { canvas, x, y } = await scene(page);
    await page.mouse.click(x, y);
    await settle(page);
    const one = await highlightPixels(canvas);

    await page.mouse.click(x + 30, y - 80);
    await settle(page);
    const two = await highlightPixels(canvas);
    expect(two).toBeGreaterThan(one + 200);

    await page.keyboard.down("Shift");
    await page.mouse.click(x, y);
    await page.keyboard.up("Shift");
    await settle(page);
    expect(await highlightPixels(canvas)).toBeLessThan(two - 200);
  });

  test("a window box selects what is entirely inside; a crossing box what it touches", async ({
    page,
  }) => {
    const { canvas, x, y } = await scene(page);

    // Left to right over the circle and only the middle of the line: a window.
    await page.mouse.click(x - 50, y - 130);
    await page.mouse.move(x + 50, y + 20);
    await settle(page);
    expect(await crossingPixels(canvas)).toBe(0);
    await page.mouse.click(x + 50, y + 20);
    await settle(page);
    const circleOnly = await highlightPixels(canvas);
    expect(circleOnly).toBeGreaterThan(300);

    await page.keyboard.press("Escape");
    // The same box right to left: a crossing box, drawn in green, takes the line too.
    await page.mouse.click(x + 50, y + 20);
    await page.mouse.move(x - 50, y - 130);
    await settle(page);
    expect(await crossingPixels(canvas)).toBeGreaterThan(200);
    await page.mouse.click(x - 50, y - 130);
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(circleOnly + 300);
    expect(await crossingPixels(canvas)).toBe(0);
  });

  test("dragging draws a box too, without re-uploading committed geometry", async ({ page }) => {
    const { canvas, x, y } = await scene(page);
    const before = await rendererStats(page);
    await page.mouse.move(x - 150, y - 130);
    await page.mouse.down();
    await page.mouse.move(x + 150, y + 30, { steps: 6 });
    await page.mouse.up();
    await settle(page);

    expect(await highlightPixels(canvas)).toBeGreaterThan(600);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });
});

test.describe("ERASE", () => {
  test("select, then Delete; undo restores and reselects", async ({ page }) => {
    const { canvas, x, y } = await scene(page);
    await page.mouse.move(0, 0);
    const both = await litPixels(canvas);

    await page.mouse.click(x, y);
    await page.keyboard.press("Delete");
    await page.mouse.move(0, 0);
    await settle(page);
    expect(await litPixels(canvas)).toBeLessThan(both - 150);
    await expect(logLines(page).last()).toHaveText("1 erased.");

    await page.keyboard.press("Control+z");
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(200);
  });

  test("ERASE asks for objects when nothing is selected", async ({ page }) => {
    const { x, y } = await scene(page);
    await enter(page, "E");
    await expect(prompt(page)).toHaveText("Select objects:");
    await page.mouse.click(x, y);
    await expect(logLines(page).last()).toHaveText("1 found, 1 total");
    await page.mouse.click(x + 30, y - 80);
    await expect(logLines(page).last()).toHaveText("1 found, 2 total");
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText("2 erased.");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(entities(file.text)).toHaveLength(0);
  });

  test("Delete while typing edits the text instead", async ({ page }) => {
    const { x, y } = await scene(page);
    await page.mouse.click(x, y);
    await page.keyboard.type("LX");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Delete");
    await page.keyboard.press("Enter");
    await expect(prompt(page)).toHaveText("Specify first point:");
  });
});

test.describe("MOVE and COPY", () => {
  test("MOVE shifts the selection by an exact typed vector", async ({ page }) => {
    const { x, y } = await scene(page);
    await page.mouse.click(x, y);
    await enter(page, "M");
    await expect(prompt(page)).toHaveText("Specify base point:");
    await enter(page, "0,0");
    await expect(prompt(page)).toHaveText("Specify second point:");
    await enter(page, "@100,50");
    await expect(prompt(page)).toHaveText("Command:");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(lines(file.text)).toContainEqual([0, 50, 200, 50]);
    // The circle was not selected and did not move.
    expect(circles(file.text)).toContainEqual([0, 80, 30]);
  });

  test("the MOVE preview is transient: overlay uploads only", async ({ page }) => {
    const { x, y } = await scene(page);
    await page.mouse.click(x, y);
    await enter(page, "M", "0,0");
    const before = await rendererStats(page);
    for (let step = 1; step <= 10; step += 1) {
      await page.mouse.move(x + step * 15, y - step * 6);
      await settle(page);
    }
    const during = await rendererStats(page);
    expect(during.overlayUploads).toBeGreaterThan(before.overlayUploads);
    expect(during.sceneUploads).toBe(before.sceneUploads);

    await page.keyboard.press("Escape");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(lines(file.text)).toContainEqual([-100, 0, 100, 0]);
  });

  test("COPY places copies until Enter and one undo removes them all", async ({ page }) => {
    const { canvas, x, y } = await scene(page);
    await page.mouse.move(0, 0);
    const original = await litPixels(canvas);

    await page.mouse.click(x, y);
    await enter(page, "CO", "0,0", "0,-60", "0,-120", "");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(0, 0);
    await settle(page);
    expect(await litPixels(canvas)).toBeGreaterThan(original + 400);

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text.match(/^LINE$/gm)).toHaveLength(3);

    await page.keyboard.press("Control+z");
    await page.mouse.move(0, 0);
    await settle(page);
    expect(await litPixels(canvas)).toBe(original);
  });
});
