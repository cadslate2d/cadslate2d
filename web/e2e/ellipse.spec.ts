// ELLIPSE (#181): drawn by its axes, on screen as a closed curve, picked on
// it, named in the Properties panel, and saved as an ELLIPSE.
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

test("an ellipse is drawn, picked, described, and saved", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "ELLIPSE", "-100,0", "100,0", "40");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  // Lit at the top of the minor axis and at the right end of the major; dark inside.
  expect(
    await litPixelsAround(page, canvas, { x: 0, y: 40 }, { width: 4, height: 4 }),
  ).toBeGreaterThan(2);
  expect(
    await litPixelsAround(page, canvas, { x: 100, y: 0 }, { width: 4, height: 4 }),
  ).toBeGreaterThan(2);
  expect(await litPixelsAround(page, canvas, { x: 0, y: 0 }, { width: 10, height: 10 })).toBe(0);

  await clickWorld(page, canvas, 0, 40);
  await expect(page.getByTestId("selection-headline")).toContainText("ellipse");
  await page.keyboard.press("Escape");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const ellipses = entities(file.text, "ELLIPSE");
  expect(ellipses).toHaveLength(1);
  expect([group(ellipses[0], 11), group(ellipses[0], 21), group(ellipses[0], 40)]).toEqual([
    "100.0",
    "0.0",
    "0.4",
  ]);
});
