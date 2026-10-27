// Hatch (ADR-0021, #167): a rectangle with a circle in it, filled solid
// from a point inside; the fill is drawn, picked, and written to the file.
import { expect, test, type Page } from "@playwright/test";
import {
  centerOf,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  litPixelsAround,
  openApp,
  prompt,
} from "./support";

/** Compares a clip around the canvas centre with its baseline. */
async function matches(page: Page, name: string, clip: { width: number; height: number }) {
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  const center = await centerOf(canvas);
  await expect(page).toHaveScreenshot(name, {
    clip: {
      x: Math.round(center.x) - clip.width / 2,
      y: Math.round(center.y) - clip.height / 2,
      width: clip.width,
      height: clip.height,
    },
    // A misplaced family moves hundreds of pixels; driver antialiasing a handful.
    maxDiffPixels: 60,
  });
}

test.describe("hatch", () => {
  test("ANSI31 in a rectangle with a hole (visual baseline)", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
    await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "PLINE", "-70,-40", "70,-40", "70,40", "-70,40", "C");
    await enter(page, "CIRCLE", "10,0", "18");
    await enter(page, "HATCH", "P", "ANSI31", "SC", "2", "-50,0");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    await matches(page, "hatch-ansi31.png", { width: 160, height: 100 });
  });

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
    await expect(prompt(page)).toHaveText("Pick internal point or [Select/Pattern/Angle/SCale]:");
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

// #346: Draw › Hatch... opens a dialog of every pattern as a swatch, with
// the angle and the scale; Pick Points runs HATCH with them.
test("the hatch dialog chooses the pattern, angle, and scale", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "PLINE", "-120,-60", "120,-60", "120,60", "-120,60", "C");
  await page
    .getByRole("menubar", { name: "Menus" })
    .getByRole("menuitem", { name: "Draw" })
    .click();
  await page.getByRole("menuitem", { name: "Hatch...", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Hatch" });
  await expect(dialog).toBeVisible();
  const patterns = dialog.getByRole("radio");
  expect(await patterns.count()).toBeGreaterThan(15);
  await dialog.getByRole("radio", { name: "ANSI31" }).click();
  await expect(dialog.getByRole("radio", { name: "ANSI31" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await dialog.getByLabel("Angle").fill("15");
  await dialog.getByLabel("Scale").fill("3");
  await dialog.getByRole("button", { name: "Pick Points" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(prompt(page)).toHaveText("Pick internal point or [Select/Pattern/Angle/SCale]:");
  await clickWorld(page, canvas, -70, 0);
  await expect(prompt(page)).toHaveText("Command:");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const hatches = entities(file.text, "HATCH");
  expect(hatches).toHaveLength(1);
  expect(group(hatches[0], 2)).toBe("ANSI31");
  // The pattern's angle and scale, as DXF writes them.
  expect(Number(group(hatches[0], 52))).toBeCloseTo(15);
  expect(Number(group(hatches[0], 41))).toBeCloseTo(3);

  // Opened again, it offers what HATCH last used.
  await page
    .getByRole("menubar", { name: "Menus" })
    .getByRole("menuitem", { name: "Draw" })
    .click();
  await page.getByRole("menuitem", { name: "Hatch...", exact: true }).click();
  await expect(dialog.getByRole("radio", { name: "ANSI31" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(dialog.getByLabel("Scale")).toHaveValue("3");
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});
