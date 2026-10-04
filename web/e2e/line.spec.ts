import { expect, test } from "@playwright/test";
import {
  centerOf,
  commandInput,
  enter,
  litPixels,
  logLines,
  openApp,
  prompt,
  rendererStats,
  settle,
} from "./support";

test.describe("LINE from the command line", () => {
  test("draws a line from the keyboard alone", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0); // keep the crosshair off the canvas
    const empty = await litPixels(canvas);
    const before = await rendererStats(page);

    await enter(page, "L");
    await expect(prompt(page)).toHaveText("Specify first point:");
    await enter(page, "0,0");
    await expect(prompt(page)).toHaveText("Specify next point:");
    await enter(page, "@200<45", "");

    await expect(prompt(page)).toHaveText("Command:");
    await expect(logLines(page).last()).toHaveText("Specify next point:");
    // The committed scene was uploaded exactly once more, for the one new line.
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads + 1);
    expect(await litPixels(canvas)).toBeGreaterThan(empty + 150);
  });

  test("L and LINE start the same command", async ({ page }) => {
    await openApp(page);
    await enter(page, "LINE");
    await expect(prompt(page)).toHaveText("Specify first point:");
    await page.keyboard.press("Escape");
    await enter(page, "l");
    await expect(prompt(page)).toHaveText("Specify first point:");
  });

  test("Space submits like Enter", async ({ page }) => {
    await openApp(page);
    await page.keyboard.type("L");
    await page.keyboard.press("Space");
    await expect(prompt(page)).toHaveText("Specify first point:");
    await expect(commandInput(page)).toHaveValue("");
  });

  test("typing works without clicking the command line first", async ({ page }) => {
    await openApp(page);
    await page.locator("header").click();
    await expect(commandInput(page)).not.toBeFocused();

    await page.keyboard.type("L");
    await expect(commandInput(page)).toBeFocused();
    await expect(commandInput(page)).toHaveValue("L");
    await page.keyboard.press("Enter");
    await expect(prompt(page)).toHaveText("Specify first point:");
  });

  test("clicking the canvas keeps keyboard focus on the command line", async ({ page }) => {
    const canvas = await openApp(page);
    await canvas.click();
    await expect(commandInput(page)).toBeFocused();
  });

  test("pointer and typed points feed the same command", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L");
    await page.mouse.click(x - 100, y);
    await expect(prompt(page)).toHaveText("Specify next point:");
    await enter(page, "@200,0", "");

    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(0, 0);
    await settle(page);
    // A 200px horizontal line.
    expect(await litPixels(canvas)).toBeGreaterThan(150);
  });

  test("invalid input is explained and the command stays active", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "banana");
    await expect(logLines(page).last()).toHaveText("A point is required, as x,y.");
    await expect(prompt(page)).toHaveText("Specify first point:");
  });

  test("unknown commands are reported", async ({ page }) => {
    await openApp(page);
    await enter(page, "FROB");
    await expect(logLines(page).last()).toHaveText('Unknown command "FROB".');
    await expect(prompt(page)).toHaveText("Command:");
  });

  test("Escape cancels and clears the input", async ({ page }) => {
    await openApp(page);
    await enter(page, "L");
    await page.keyboard.type("12,");
    await page.keyboard.press("Escape");
    await expect(prompt(page)).toHaveText("Command:");
    await expect(commandInput(page)).toHaveValue("");
    await expect(logLines(page).last()).toHaveText("*Cancel*");
  });

  test("Up and Down recall submitted input", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0");
    await page.keyboard.press("ArrowUp");
    await expect(commandInput(page)).toHaveValue("0,0");
    await page.keyboard.press("ArrowUp");
    await expect(commandInput(page)).toHaveValue("L");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(commandInput(page)).toHaveValue("");
  });

  test("Ctrl+Z and Ctrl+Y undo and redo", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);
    await enter(page, "L", "-100,0", "100,0", "");
    const drawn = await litPixels(canvas);
    expect(drawn).toBeGreaterThan(empty + 150);

    await page.keyboard.press("Control+z");
    await settle(page);
    expect(await litPixels(canvas)).toBe(empty);

    await page.keyboard.press("Control+y");
    await settle(page);
    expect(await litPixels(canvas)).toBe(drawn);
  });
});

test.describe("transient overlay", () => {
  // DESIGN §26.3 criteria 5, 6, and 9.
  test("the rubber band updates only overlay buffers", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "0,0");
    await page.mouse.move(x + 10, y + 10);
    await settle(page);
    const before = await rendererStats(page);

    for (let step = 1; step <= 20; step += 1) {
      await page.mouse.move(x + step * 8, y - step * 4);
      await settle(page);
    }

    const after = await rendererStats(page);
    expect(after.overlayUploads).toBeGreaterThan(before.overlayUploads);
    expect(after.sceneUploads).toBe(before.sceneUploads);
    expect(after.bufferReallocations).toBe(before.bufferReallocations);
  });

  test("the rubber band is visible and disappears on cancel", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "L", "0,0");
    await page.mouse.move(x + 200, y - 100);
    await settle(page);
    const withRubberBand = await litPixels(canvas);

    await page.keyboard.press("Escape");
    await settle(page);
    const crosshairOnly = await litPixels(canvas);

    expect(withRubberBand).toBeGreaterThan(crosshairOnly + 100);
    expect(crosshairOnly).toBeGreaterThan(0);
  });

  test("the status bar shows the cursor in world coordinates", async ({ page }) => {
    const canvas = await openApp(page);
    const readout = page.getByTestId("coordinates");
    await expect(readout).toHaveText("—");

    const { x, y } = await centerOf(canvas);
    await page.mouse.move(x + 25, y - 10);
    await expect(readout).toHaveText(/^-?\d+\.\d{4}, -?\d+\.\d{4}$/);
    const [worldX, worldY] = ((await readout.textContent()) ?? "").split(", ").map(Number);
    expect(worldX).toBeCloseTo(25, 0);
    expect(worldY).toBeCloseTo(10, 0);

    await page.mouse.move(2, 2);
    await expect(readout).toHaveText("—");
  });
});
