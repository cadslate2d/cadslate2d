// Hatch (ADR-0021, #167): a rectangle with a circle in it, filled solid
// from a point inside; the fill is drawn, picked, and written to the file.
import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  litPixelsAround,
  openApp,
  prompt,
} from "./support";

test.describe("hatch", () => {
  test("a rectangle with a circle in it is hatched solid from a point inside", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "PLINE", "-120,-60", "120,-60", "120,60", "-120,60", "C");
    await enter(page, "CIRCLE", "0,0", "25");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    const between = { x: -70, y: 0 };
    const inCircle = { x: 0, y: 0 };
    const half = { width: 12, height: 12 };
    const betweenBefore = await litPixelsAround(page, canvas, between, half);
    const circleBefore = await litPixelsAround(page, canvas, inCircle, half);
    expect(betweenBefore).toBeLessThan(20);

    await enter(page, "HATCH");
    await expect(prompt(page)).toHaveText("Pick internal point or [Select]:");
    await clickWorld(page, canvas, -70, 0);
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    // Between the rectangle and the circle every pixel is lit; inside the
    // circle, the hole, none more than before.
    expect(await litPixelsAround(page, canvas, between, half)).toBeGreaterThan(500);
    expect(await litPixelsAround(page, canvas, inCircle, half)).toBeLessThan(circleBefore + 20);

    // Picked by its fill.
    await clickWorld(page, canvas, -70, 0);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 hatch");
    await page.keyboard.press("Escape");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    const hatches = entities(file.text, "HATCH");
    expect(hatches).toHaveLength(1);
    expect(group(hatches[0], 2)).toBe("SOLID");
    expect(group(hatches[0], 91)).toBe("2");
  });
});
