// SPLINE (#182): drawn through points, on screen as a curve through them,
// picked on it, named in the Properties panel, and saved as a SPLINE with
// its fit points.
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

test("a spline is drawn through its points, picked, described, and saved", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "SPLINE", "-100,-40", "-40,40", "40,40", "100,-40", "");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  // Lit at a fit point and above the middle; dark well below the arch.
  expect(
    await litPixelsAround(page, canvas, { x: -40, y: 40 }, { width: 4, height: 4 }),
  ).toBeGreaterThan(2);
  expect(
    await litPixelsAround(page, canvas, { x: 0, y: 45 }, { width: 3, height: 6 }),
  ).toBeGreaterThan(2);
  expect(await litPixelsAround(page, canvas, { x: 0, y: 0 }, { width: 10, height: 10 })).toBe(0);

  await clickWorld(page, canvas, -40, 40);
  await expect(page.getByTestId("selection-headline")).toContainText("spline");
  await page.keyboard.press("Escape");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const splines = entities(file.text, "SPLINE");
  expect(splines).toHaveLength(1);
  expect([group(splines[0], 71), group(splines[0], 73), group(splines[0], 74)]).toEqual([
    "3",
    "4",
    "4",
  ]);
});
