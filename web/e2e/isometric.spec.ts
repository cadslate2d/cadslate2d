// Isometric drafting (#336): the isometric snap from Tools › Drafting
// Settings, the plane shown in the status bar and switched by F5, the
// grid drawn on the isometric axes, and an isocircle from Draw › Ellipse,
// one undo.
import { expect, test } from "@playwright/test";
import {
  chooseMenu,
  downloadFrom,
  enter,
  entities,
  gridPixels,
  logLines,
  num,
  openApp,
  prompt,
} from "./support";

test("isometric snap, F5, and an isocircle", async ({ page }) => {
  await openApp(page);
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  await chooseMenu(page, "Tools", "Drafting Settings", "Isometric Snap");
  await expect(prompt(page)).toHaveText("Specify vertical spacing <10>:");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("Snap style: Isometric, 10 up. Isoplane: Top");
  const plane = page.getByTestId("isoplane");
  await expect(plane).toHaveText("ISO TOP");
  await page.keyboard.press("F5");
  await expect(plane).toHaveText("ISO RIGHT");
  await plane.click();
  await expect(plane).toHaveText("ISO LEFT");
  await page.keyboard.press("F5");
  await expect(plane).toHaveText("ISO TOP");

  // The grid on the isometric axes.
  await page.keyboard.press("F7");
  await expect.poll(() => gridPixels(canvas)).toBeGreaterThan(0);

  await chooseMenu(page, "Draw", "Ellipse", "Isocircle");
  await expect(prompt(page)).toHaveText("Specify center of isocircle:");
  await enter(page, "0,0", "20");
  await expect(prompt(page)).toHaveText("Command:");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const [ellipse] = entities(file.text, "ELLIPSE");
  // Its long axis level and 20·√(3/2) long, its ratio 1/√3.
  expect(num(ellipse, 11)).toBeCloseTo(20 * Math.sqrt(1.5), 6);
  expect(num(ellipse, 21)).toBeCloseTo(0, 6);
  expect(num(ellipse, 40)).toBeCloseTo(1 / Math.sqrt(3), 6);
  await enter(page, "U");
  const after = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(after.text, "ELLIPSE")).toHaveLength(0);

  // Back to the standard style: the plane leaves the status bar.
  await enter(page, "SNAP", "S", "S");
  await expect(plane).toHaveCount(0);
});
