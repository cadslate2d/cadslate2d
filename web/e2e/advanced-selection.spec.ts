// Advanced selection (#185): the methods every selection prompt offers,
// typed at the command line, and QSELECT; the Properties panel names the
// method.
import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  crossingPixels,
  downloadFrom,
  enter,
  entities,
  logLines,
  openApp,
  prompt,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

test("a fence selects what it crosses and the panel says so", async ({ page }) => {
  const canvas = await openApp(page);
  await enter(page, "L", "-100,0", "100,0", "", "C", "0,80", "30");
  await enter(page, "L", "150,-50", "150,150", "");
  await enter(page, "E");
  await expect(prompt(page)).toHaveText(
    "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:",
  );
  await enter(page, "F", "-20,-50");
  await expect(prompt(page)).toHaveText("Next fence point or [Undo]:");
  // The rubber band to the pointer is drawn in the crossing style.
  await clickWorld(page, canvas, -20, 120);
  expect(await crossingPixels(canvas)).toBeGreaterThan(0);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("2 found, 2 total");
  await expect(page.getByTestId("selection-headline")).toHaveText("2 objects: 1 line, 1 circle");
  await expect(page.getByTestId("selection-method")).toHaveText("by fence");
  await enter(page, "");
  expect(entities(await exported(page))).toHaveLength(1);
});

test("QSELECT picks objects by kind and property", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "100,0", "", "L", "0,20", "100,20", "", "C", "0,80", "30");
  await enter(page, "COLOR", "red", "L", "0,40", "100,40", "");
  await enter(page, "QS", "line", "C", "= red", "");
  await expect(logLines(page).last()).toHaveText("1 found.");
  await expect(page.getByTestId("selection-headline")).toHaveText("1 line");
  await expect(page.getByTestId("selection-method")).toHaveText("by quick select");
  await enter(page, "QS", "", "");
  await expect(logLines(page).last()).toHaveText("4 found.");
});

// #207: a fence started by name at the idle prompt — the short F is FILLET.
test("FENCE at the idle prompt selects what the fence crosses", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "100,0", "", "L", "0,20", "100,20", "", "L", "0,40", "100,40", "");
  await enter(page, "FENCE");
  await enter(page, "50,-10", "50,30", "", "");
  await expect(logLines(page).last()).toHaveText("2 selected.");
  await expect(page.getByTestId("selection-headline")).toHaveText("2 lines");
  await expect(page.getByTestId("selection-method")).toHaveText("by fence");
  await page.keyboard.press("Escape");
  await enter(page, "F");
  await expect(prompt(page)).toHaveText(/^Select first object/);
});
