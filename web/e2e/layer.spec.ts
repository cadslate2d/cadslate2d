import { expect, test } from "@playwright/test";
import {
  clickWorld,
  commandInput,
  enter,
  highlightPixels,
  litPixelsAround,
  logLines,
  openApp,
  prompt,
} from "./support";

const OPTIONS =
  "Press Enter to finish or [?/New/Set/Color/LWeight/LType/ON/OFF/Freeze/Thaw/Lock/Unlock/Rename/Delete]:";

test.describe("LAYER", () => {
  test("makes a layer whose name has a space, and draws on it", async ({ page }) => {
    await openApp(page);
    await enter(page, "LA");
    await expect(prompt(page)).toHaveText(OPTIONS);
    await enter(page, "N");
    await expect(prompt(page)).toHaveText("Enter name(s) for new layer(s):");

    // At a name prompt Space is a character, not Enter.
    await page.keyboard.type("Ground floor");
    await expect(commandInput(page)).toHaveValue("Ground floor");
    await page.keyboard.press("Enter");
    await expect(logLines(page).last()).toHaveText("1 layer created.");

    await enter(page, "S");
    await page.keyboard.type("ground floor");
    await page.keyboard.press("Enter");
    await expect(logLines(page).last()).toHaveText('Current layer: "Ground floor"');

    // Back at the options, Space submits again.
    await page.keyboard.type("?");
    await page.keyboard.press("Space");
    await expect(logLines(page).last()).toHaveText('"Ground floor": colour 7, on, current');
    await enter(page, "");
    await expect(prompt(page)).toHaveText("Command:");
  });

  test("a layer that is off hides its objects; one that is locked protects them", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7"); // no grid: only the line lights pixels
    await enter(page, "LA", "N", "walls", "S", "walls", "");
    await enter(page, "L", "0,0", "100,0", "");
    await page.mouse.move(5, 5);
    const drawn = await litPixelsAround(page, canvas, { x: 50, y: 0 }, { width: 60, height: 10 });
    expect(drawn).toBeGreaterThan(80);

    // Off: not drawn, and a click finds nothing.
    await enter(page, "LA", "S", "0", "OFF", "walls", "");
    await page.mouse.move(5, 5);
    expect(await litPixelsAround(page, canvas, { x: 50, y: 0 }, { width: 60, height: 10 })).toBe(0);
    await clickWorld(page, canvas, 50, 0);
    await page.keyboard.press("Escape");
    expect(await highlightPixels(canvas)).toBe(0);

    // On again and locked: drawn, but not selected, and the click says why.
    await enter(page, "LA", "ON", "walls", "L", "walls", "");
    await page.mouse.move(5, 5);
    expect(await litPixelsAround(page, canvas, { x: 50, y: 0 }, { width: 60, height: 10 })).toBe(
      drawn,
    );
    await clickWorld(page, canvas, 50, 0);
    await expect(logLines(page).last()).toHaveText("1 was on a locked layer.");
    expect(await highlightPixels(canvas)).toBe(0);

    // Unlocked: selected like anything else.
    await enter(page, "LA", "U", "walls", "");
    await clickWorld(page, canvas, 50, 0);
    expect(await highlightPixels(canvas)).toBeGreaterThan(80);
  });

  test("refuses what the layer table does not allow, and says why", async ({ page }) => {
    await openApp(page);
    await enter(page, "LA", "N", "walls", "N", "WALLS");
    await expect(logLines(page).last()).toHaveText('There is already a layer "walls".');
    await enter(page, "D", "0");
    await expect(logLines(page).last()).toHaveText('Layer "0" cannot be renamed or deleted.');
    await enter(page, "OFF", "0");
    await expect(logLines(page).last()).toHaveText(
      'Layer "0" is the current layer: it cannot be switched off, frozen, or deleted.',
    );
    await expect(prompt(page)).toHaveText(OPTIONS);
  });
});
