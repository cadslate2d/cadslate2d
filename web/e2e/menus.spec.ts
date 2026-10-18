// The menus of AutoCAD 2007/2008's classic workspace (#247): a sample of
// items run through the menu bar — a cascade with answers, one that pauses
// for the user, a layer tool, an inquiry, Help › About — and the keys the
// menus show.
import { expect, test } from "@playwright/test";
import {
  chooseMenu,
  circles,
  clickWorld,
  downloadFrom,
  enter,
  logLines,
  menuEntry,
  openApp,
  prompt,
} from "./support";

test("Draw › Circle › 2 Points types CIRCLE then 2P", async ({ page }) => {
  await openApp(page);
  await chooseMenu(page, "Draw", "Circle", "2 Points");
  await expect(prompt(page)).toHaveText("Specify first end point of circle's diameter:");
  await enter(page, "0,0", "20,0");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(circles(file.text)).toEqual([[10, 0, 10]]);
});

test("Draw › Arc › Start, Center, End waits for the start point, then types C", async ({
  page,
}) => {
  await openApp(page);
  await chooseMenu(page, "Draw", "Arc", "Start, Center, End");
  await expect(prompt(page)).toHaveText("Specify start point of arc:");
  await enter(page, "10,0");
  await expect(prompt(page)).toHaveText("Specify center point of arc:");
  await enter(page, "0,0", "0,10");
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "is required" })).toHaveCount(0);
});

test("Format › Layer Tools › Layer Off, and Tools › Inquiry › Distance", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F3");
  await enter(page, "LAYER", "N", "WALLS", "S", "WALLS", "", "L", "0,40", "60,40", "");
  await enter(page, "LAYER", "S", "0", "");
  await chooseMenu(page, "Format", "Layer Tools", "Layer Off");
  await clickWorld(page, canvas, 30, 40);
  await expect(logLines(page).filter({ hasText: 'Layer "WALLS" is off.' })).toHaveCount(1);

  await chooseMenu(page, "Tools", "Inquiry", "Distance");
  await enter(page, "0,0", "30,40");
  await expect(logLines(page).filter({ hasText: "Distance = 50" })).toHaveCount(1);
});

test("Edit › Undo is greyed with nothing to undo; Help › About names the core", async ({
  page,
}) => {
  await openApp(page);
  await page
    .getByRole("menubar", { name: "Menus" })
    .getByRole("menuitem", { name: "Edit" })
    .click();
  await expect(menuEntry(page, 0, "Undo")).toHaveAttribute("aria-disabled", "true");
  await expect(menuEntry(page, 0, "Paste")).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape");
  await enter(page, "L", "0,0", "10,0", "");
  await chooseMenu(page, "Edit", "Undo");
  await expect(logLines(page).filter({ hasText: "Command: U" })).toHaveCount(1);

  await chooseMenu(page, "Help", "About CadSlate2D...");
  const about = page.getByRole("dialog", { name: "About CadSlate2D" });
  await expect(about).toContainText("Core version");
  await about.getByRole("button", { name: "Close" }).click();
  await expect(about).toHaveCount(0);
});

test("the menus' keys: Ctrl+A selects all, Ctrl+Shift+S saves as, Ctrl+P plots", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "L", "0,0", "10,0", "", "C", "0,0", "5");
  await page.keyboard.press("Control+a");
  await expect(page.getByTestId("selection-headline")).toHaveText("2 objects: 1 line, 1 circle");
  await page.keyboard.press("Escape");

  const saved = await downloadFrom(page, () => page.keyboard.press("Control+Shift+S"));
  expect(saved.text).toContain("AcDbCircle");
  // PLOT asks for the sheet before it makes the file.
  await page.keyboard.press("Control+p");
  await expect(prompt(page)).toContainText("Paper size");
});
