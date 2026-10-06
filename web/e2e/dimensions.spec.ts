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
  num,
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

  test("DIMANGULAR, DIMRADIUS, and DIMDIAMETER, each once", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "LINE", "-200,-60", "0,60", "");
    await enter(page, "LINE", "-200,60", "0,-60", "");
    await enter(page, "CIRCLE", "120,0", "40");
    await expect(prompt(page)).toHaveText("Command:");

    // The angle between the lines, on the side the pointer is.
    await enter(page, "DIMANGULAR");
    await expect(prompt(page)).toHaveText("Select arc, circle, line <specify vertex>:");
    await clickWorld(page, canvas, -50, 30);
    await expect(prompt(page)).toHaveText("Select second line:");
    await clickWorld(page, canvas, -50, -30);
    await expect(prompt(page)).toHaveText("Specify dimension arc line location or [Text/Angle]:");
    await enter(page, "-40,0");
    await expect(prompt(page)).toHaveText("Command:");

    // The radius inside the circle, the diameter outside it.
    await enter(page, "DIMRADIUS");
    await clickWorld(page, canvas, 160, 0);
    await expect(prompt(page)).toHaveText("Specify dimension line location or [Text/Angle]:");
    await enter(page, "140,0");
    await enter(page, "DIMDIAMETER");
    await clickWorld(page, canvas, 120, 40);
    await enter(page, "120,70");
    await expect(prompt(page)).toHaveText("Command:");

    // The radius dimension is picked by its leader.
    await clickWorld(page, canvas, 150, 0);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 dimension");
    await page.keyboard.press("Escape");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    const dimensions = entities(file.text, "DIMENSION");
    // Kinds 5, 4, 3, with the block and user-text flags (32 + 128).
    expect(dimensions.map((record) => group(record, 70))).toEqual(["165", "164", "163"]);
    expect(num(dimensions[0], 42)).toBeCloseTo(61.9275, 3);
    expect(group(dimensions[1], 42)).toBe("40.0");
    expect(group(dimensions[2], 42)).toBe("80.0");
  });
});
