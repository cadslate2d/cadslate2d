// Keyboard access to the menus (#248), as in AutoCAD 2007/2008: Alt and a
// letter opens a menu, Alt alone puts the bar in reach with its letters
// underlined, the arrows walk the menus and their cascades, a letter runs
// what it underlines, and Esc closes one level at a time.
import { expect, test, type Page } from "@playwright/test";
import { commandInput, enter, logLines, openApp, prompt } from "./support";

const lists = (page: Page) => page.getByRole("menu");
const underlined = (page: Page) => page.locator(".menubar__mnemonic");

test("Alt+O opens Format, and L runs Layer", async ({ page }) => {
  await openApp(page);
  await page.keyboard.press("Alt+o");
  await expect(lists(page)).toHaveCount(1);
  await expect(underlined(page).first()).toBeVisible();
  await page.keyboard.press("l");
  await expect(lists(page)).toHaveCount(0);
  await expect(logLines(page).filter({ hasText: "Command: LAYER" })).toHaveCount(1);
});

test("Alt, then the arrows to Draw › Circle › 2 Points, then Enter", async ({ page }) => {
  await openApp(page);
  await page.keyboard.press("Alt");
  await expect(underlined(page).first()).toBeVisible();
  await expect(lists(page)).toHaveCount(0);
  for (let step = 0; step < 6; step += 1) await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowDown");
  await expect(lists(page)).toHaveCount(1);
  // Line, Ray, Construction Line, Polyline, Polygon, Rectangle, Arc, Circle:
  // the separators are passed over.
  for (let step = 0; step < 7; step += 1) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowRight");
  await expect(lists(page)).toHaveCount(2);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(lists(page)).toHaveCount(0);
  await expect(prompt(page)).toHaveText("Specify first end point of circle's diameter:");
});

test("Esc closes one level at a time, and typing goes back to the command line", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "L", "0,0");
  await page.keyboard.press("Alt+d");
  await page.keyboard.press("c");
  await expect(lists(page)).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(lists(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(lists(page)).toHaveCount(0);
  await expect(underlined(page).first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(underlined(page)).toHaveCount(0);
  // The menus' Esc cancelled nothing: the line goes on, and what is typed reaches it.
  await expect(prompt(page)).toHaveText("Specify next point or [Undo]:");
  await page.keyboard.type("10,0");
  await expect(commandInput(page)).toHaveValue("10,0");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await expect(prompt(page)).toHaveText("Command:");
});

test("Alt alone twice puts the bar in reach and takes it away again", async ({ page }) => {
  await openApp(page);
  await page.keyboard.press("Alt");
  await expect(underlined(page).first()).toBeVisible();
  await page.keyboard.press("Alt");
  await expect(underlined(page)).toHaveCount(0);
  await page.keyboard.type("LINE");
  await expect(commandInput(page)).toHaveValue("LINE");
});
