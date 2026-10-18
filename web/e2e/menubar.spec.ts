// The menu bar (#246, ADR-0034): menus opened with the pointer, moved
// across while open, a cascade, closing by Esc and by a click elsewhere, the
// panels shown and hidden from View, and every item a command typed.
import { expect, test, type Page } from "@playwright/test";
import {
  centerOf,
  chooseMenu,
  commandInput,
  downloadFrom,
  enter,
  logLines,
  menuEntry,
  openApp,
  prompt,
} from "./support";

const title = (page: Page, name: string) =>
  page.getByRole("menubar", { name: "Menus" }).getByRole("menuitem", { name, exact: true });
const lists = (page: Page) => page.getByRole("menu");

test("File › Save saves through the command line, as typing SAVE does", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "10,0", "");
  const file = await downloadFrom(page, () => chooseMenu(page, "File", "Save"));
  expect(file.text).toContain("AcDbLine");
  await expect(logLines(page).filter({ hasText: "Command: SAVE" })).toHaveCount(1);
  await expect(lists(page)).toHaveCount(0);
});

test("an open menu follows the pointer across the bar, and a cascade opens on hover", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await title(page, "File").click();
  await expect(title(page, "File")).toHaveAttribute("aria-expanded", "true");
  await title(page, "View").hover();
  await expect(title(page, "File")).toHaveAttribute("aria-expanded", "false");
  await expect(title(page, "View")).toHaveAttribute("aria-expanded", "true");
  await menuEntry(page, 0, "Zoom").hover();
  await expect(lists(page)).toHaveCount(2);
  await menuEntry(page, 1, "Window").hover();
  if (browserName === "chromium") {
    const center = await centerOf(canvas);
    await expect(page).toHaveScreenshot("menubar-view-zoom.png", {
      clip: { x: 0, y: 44, width: 520, height: Math.min(300, center.y) },
      maxDiffPixels: 60,
    });
  }
  await menuEntry(page, 1, "Window").click();
  await expect(lists(page)).toHaveCount(0);
  await expect(prompt(page)).toContainText("Specify corner of window");
  // Focus stayed on the command line: typing goes there.
  await expect(commandInput(page)).toBeFocused();
});

test("Esc closes one level at a time, and a click elsewhere closes the menu", async ({ page }) => {
  const canvas = await openApp(page);
  await enter(page, "L", "0,0");
  await title(page, "View").click();
  await menuEntry(page, 0, "Zoom").hover();
  await expect(lists(page)).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(lists(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(lists(page)).toHaveCount(0);
  // The menu's Esc, not the command line's: the line goes on.
  await expect(prompt(page)).toHaveText("Specify next point or [Undo]:");

  await title(page, "File").click();
  const center = await centerOf(canvas);
  await page.mouse.click(center.x + 200, center.y + 100);
  await expect(lists(page)).toHaveCount(0);
});

test("View shows and hides the panels, checked while each shows; Tools › Options opens the settings", async ({
  page,
}) => {
  await openApp(page);
  const properties = page.getByRole("complementary", { name: "Properties" });
  await expect(properties).toBeVisible();
  await title(page, "View").click();
  await expect(menuEntry(page, 0, "Properties Panel")).toHaveAttribute("aria-checked", "true");
  await menuEntry(page, 0, "Properties Panel").click();
  await expect(properties).toHaveCount(0);
  await chooseMenu(page, "View", "Properties Panel");
  await expect(properties).toBeVisible();

  await chooseMenu(page, "Tools", "Options...");
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
});

test("the quick actions at the row's right run their commands", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "10,0", "");
  const file = await downloadFrom(page, () =>
    page
      .getByRole("group", { name: "Quick actions" })
      .getByRole("button", { name: "Export" })
      .click(),
  );
  expect(file.text).toContain("AcDbLine");
  await expect(logLines(page).filter({ hasText: "Command: EXPORT" })).toHaveCount(1);
});
