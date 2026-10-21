// The M18 exit scenario (#249): a plate drafted through the menu bar alone —
// every command started from a menu, by the pointer or by Alt and the
// letters, only the answers typed — then measured, zoomed, and saved.
import { expect, test } from "@playwright/test";
import {
  chooseMenu,
  circles,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("the M18 scenario: a plate drafted through the menus alone, then saved", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F3");

  // 1. A new drawing, and a layer for it, from File and Format.
  await chooseMenu(page, "File", "New...");
  await expect(page).toHaveTitle("CadSlate2D");
  await chooseMenu(page, "Format", "Layer...");
  await enter(page, "N", "PLATE", "S", "PLATE", "");

  // 2. The outline and a hole by the pointer; a second hole by the keyboard.
  await chooseMenu(page, "Draw", "Rectangle");
  await enter(page, "0,0", "100,60");
  await chooseMenu(page, "Draw", "Circle", "Center, Radius");
  await enter(page, "30,30", "8");
  await page.keyboard.press("Alt+d");
  await page.keyboard.press("c");
  await page.keyboard.press("2");
  await expect(prompt(page)).toHaveText("Specify first end point of circle's diameter:");
  await enter(page, "62,30", "78,30");

  // 3. A dimension across the bottom, by the keyboard: Alt+N, L.
  await page.keyboard.press("Alt+n");
  await page.keyboard.press("l");
  await enter(page, "0,0", "100,0", "50,-15");

  // 4. A bracket beside it, its corner filleted from Modify.
  await chooseMenu(page, "Draw", "Line");
  await enter(page, "160,0", "120,0", "120,40", "");
  await chooseMenu(page, "Modify", "Fillet");
  await enter(page, "R", "5");
  await clickWorld(page, canvas, 150, 0);
  await clickWorld(page, canvas, 120, 30);
  await expect(prompt(page)).toHaveText("Command:");

  // 5. A distance from Tools, and the whole drawing from View.
  await chooseMenu(page, "Tools", "Inquiry", "Distance");
  await enter(page, "0,0", "100,60");
  await expect(logLines(page).filter({ hasText: "Distance = 116.6190" })).toHaveCount(1);
  await chooseMenu(page, "View", "Zoom", "Extents");

  // 6. Saved from File: the outline, two holes, the dimension, the fillet's
  // arc, all on the layer made from Format.
  // Save As… asks the format first (#319); Enter keeps the drawing's, DXF.
  await chooseMenu(page, "File", "Save As...");
  await expect(prompt(page)).toHaveText("Save as or [DXF/DWG2000/DWG2004/DWG2007] <DXF>:");
  const file = await downloadFrom(page, () => enter(page, ""));
  expect(entities(file.text, "LWPOLYLINE")).toHaveLength(1);
  expect(circles(file.text)).toEqual([
    [30, 30, 8],
    [70, 30, 8],
  ]);
  expect(entities(file.text, "DIMENSION")).toHaveLength(1);
  expect(entities(file.text, "ARC")).toHaveLength(1);
  expect(tableRow(file.text, "LAYER", "PLATE")).toBeDefined();
  expect(group(entities(file.text, "CIRCLE")[0], 8)).toBe("PLATE");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
  await expect(logLines(page).filter({ hasText: "is required" })).toHaveCount(0);
});
