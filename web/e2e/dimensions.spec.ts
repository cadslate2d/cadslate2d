// Dimensions (ADR-0020, #164): a line dimensioned both ways, picked by
// its dimension line, redrawn by a style change, and written to DXF.
import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  litPixels,
  litPixelsAround,
  openApp,
  prompt,
} from "./support";

test.describe("dimensions", () => {
  test("DIMLINEAR and DIMALIGNED dimension a line; a style change redraws both", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "LINE", "-100,0", "100,0", "");
    await page.mouse.move(5, 5);
    const lineOnly = await litPixels(canvas);

    await enter(page, "DIMLINEAR", "-100,0", "100,0", "0,40");
    await expect(prompt(page)).toHaveText("Command:");
    await enter(page, "DIMALIGNED", "-100,0", "100,0", "0,-40");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    expect(await litPixels(canvas)).toBeGreaterThan(lineOnly + 300);

    // Picked by its dimension line; the panel says what it is.
    await clickWorld(page, canvas, 50, 40);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 dimension");
    await page.keyboard.press("Escape");

    // The text of each sits just above its dimension line. Taller text
    // lights more pixels there — in both, from one change to the style.
    const around = (y: number) =>
      litPixelsAround(page, canvas, { x: 0, y }, { width: 40, height: 14 });
    const upperBefore = await around(48);
    const lowerBefore = await around(-32);
    await enter(page, "DIMSTYLE", "H", "10", "");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    expect(await around(48)).toBeGreaterThan(upperBefore);
    expect(await around(-32)).toBeGreaterThan(lowerBefore);

    // Both reach the file as DIMENSION records in the current style.
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    const dimensions = entities(file.text, "DIMENSION");
    expect(dimensions).toHaveLength(2);
    expect(dimensions.map((record) => group(record, 3))).toEqual(["Standard", "Standard"]);
    expect(dimensions.map((record) => group(record, 42))).toEqual(["200.0", "200.0"]);
  });
});
