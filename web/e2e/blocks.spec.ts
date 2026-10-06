// Blocks through the shell (#148): draw, BLOCK, INSERT twice, redefine,
// and every reference changes; EXPLODE takes one apart again.
import { expect, test } from "@playwright/test";
import { clickWorld, enter, litPixelsAround, logLines, openApp, prompt } from "./support";

test.describe("blocks", () => {
  test("BLOCK, INSERT twice, redefine: both references change", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    const lit = (x: number, y: number) =>
      litPixelsAround(page, canvas, { x, y }, { width: 6, height: 6 });

    // A chair: an L of two lines at the origin.
    await enter(page, "L", "0,0", "40,0", "");
    await enter(page, "L", "0,0", "0,40", "");
    await page.mouse.move(5, 5);
    const seat = await lit(20, 0);
    expect(seat).toBeGreaterThan(5);

    // BLOCK: a name, a base point, and the two lines.
    await enter(page, "B");
    await expect(prompt(page)).toHaveText("Enter block name:");
    await enter(page, "Chair", "0,0");
    await expect(prompt(page)).toHaveText("Select objects:");
    await clickWorld(page, canvas, 20, 0);
    await clickWorld(page, canvas, 0, 20);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText('Block "Chair" defined from 2 objects.');
    await expect(prompt(page)).toHaveText("Command:");
    // The reference draws where the lines were, and is one object.
    await page.mouse.move(5, 5);
    expect(await lit(20, 0)).toBeGreaterThan(5);
    await clickWorld(page, canvas, 20, 0);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 block reference");
    await page.keyboard.press("Escape");

    // INSERT twice: as it is at (100, 0), and twice the size at (100, 100).
    await enter(page, "I");
    await expect(prompt(page)).toHaveText("Enter block name:");
    await enter(page, "Chair", "100,0");
    await expect(prompt(page)).toHaveText("Specify scale factor <1>:");
    await enter(page, "");
    await expect(prompt(page)).toHaveText("Specify rotation angle <0>:");
    await enter(page, "");
    await enter(page, "I", "Chair", "100,100", "2", "0");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    expect(await lit(120, 0)).toBeGreaterThan(5);
    expect(await lit(160, 100)).toBeGreaterThan(5);
    expect(await lit(160, 0)).toBe(0);

    // Redefine the chair as a tall stool: a single upright, 60 long.
    await enter(page, "L", "200,0", "200,60", "");
    await enter(page, "B", "Chair");
    await expect(prompt(page)).toHaveText(
      'Block "Chair" already exists. Redefine it? [Yes/No] <No>:',
    );
    await enter(page, "Yes", "200,0");
    await clickWorld(page, canvas, 200, 30);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText('Block "Chair" redefined from 1 object.');
    await page.mouse.move(5, 5);
    // Every reference follows: no seats, uprights 60 and 120 tall.
    expect(await lit(20, 0)).toBe(0);
    expect(await lit(120, 0)).toBe(0);
    expect(await lit(0, 50)).toBeGreaterThan(5);
    expect(await lit(100, 50)).toBeGreaterThan(5);
    expect(await lit(100, 200)).toBeGreaterThan(5);

    // One undo puts the chairs back everywhere.
    await page.keyboard.press("Control+z");
    await page.mouse.move(5, 5);
    expect(await lit(20, 0)).toBeGreaterThan(5);
    expect(await lit(100, 200)).toBe(0);
  });

  test("EXPLODE takes a reference apart into its lines", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "40,0", "");
    await enter(page, "L", "0,0", "0,40", "");
    await enter(page, "B", "Chair", "0,0");
    await clickWorld(page, canvas, 20, 0);
    await clickWorld(page, canvas, 0, 20);
    await enter(page, "");

    await enter(page, "X");
    await expect(prompt(page)).toHaveText("Select objects:");
    await clickWorld(page, canvas, 20, 0);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText("1 block reference exploded into 2 objects.");
    await clickWorld(page, canvas, 20, 0);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 line");
  });
});
