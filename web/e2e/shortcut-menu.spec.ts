// The drawing area's shortcut menu (AutoCAD's right click): on an object, it
// selects it and offers what to do with it, Properties among them; during a
// command, Enter, Cancel, and the command's options.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { enter, openApp, prompt } from "./support";

const menu = (page: Page) => page.locator(".shortcut-menu");
const headline = (page: Page) => page.getByTestId("selection-headline");

async function rightClickWorld(page: Page, canvas: Locator, x: number, y: number) {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("the canvas is not shown");
  await page.mouse.click(box.x + box.width / 2 + x, box.y + box.height / 2 - y, {
    button: "right",
  });
}

test.describe("the shortcut menu", () => {
  test("a right click on a polyline selects it, and Properties shows it", async ({ page }) => {
    // The Properties panel hidden, as a user may have left it.
    await page.addInitScript(() => localStorage.setItem("cadslate2d.properties", "hidden"));
    const canvas = await openApp(page);
    await enter(page, "PLINE", "0,0", "100,0", "100,50", "");
    await expect(page.getByRole("complementary", { name: "Properties" })).toHaveCount(0);

    // Over nothing: the default menu, nothing selected.
    await rightClickWorld(page, canvas, -150, 120);
    await expect(menu(page).getByRole("menuitem", { name: "Repeat PLINE" })).toBeVisible();
    await expect(menu(page).getByRole("menuitem", { name: "Properties", exact: true })).toHaveCount(
      0,
    );
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);

    await rightClickWorld(page, canvas, 50, 0);
    await expect(menu(page).getByRole("menuitem", { name: "Erase", exact: true })).toBeVisible();
    await menu(page).getByRole("menuitem", { name: "Properties", exact: true }).click();
    await expect(menu(page)).toHaveCount(0);
    await expect(page.getByRole("complementary", { name: "Properties" })).toBeVisible();
    await expect(headline(page)).toHaveText("1 polyline");

    // The edit menu acts on the selection: Erase erases it, one undo.
    await rightClickWorld(page, canvas, 50, 0);
    await menu(page).getByRole("menuitem", { name: "Erase", exact: true }).click();
    await expect(headline(page)).toHaveText("No selection");
    // Nothing is left there to pick.
    await rightClickWorld(page, canvas, 50, 0);
    await expect(menu(page).getByRole("menuitem", { name: "Erase", exact: true })).toHaveCount(0);
    await expect(headline(page)).toHaveText("No selection");
  });

  test("during a command it offers Enter, Cancel, and the command's options", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "LINE", "0,0", "100,0", "100,100");
    await rightClickWorld(page, canvas, 0, 50);
    const names = await menu(page)
      .getByRole("menuitem")
      .evaluateAll((items) => items.map((item) => item.getAttribute("aria-label")));
    expect(names).toEqual(["Enter", "Cancel", "Close", "Undo"]);
    await menu(page).getByRole("menuitem", { name: "Close" }).click();
    await expect(prompt(page)).toHaveText("Command:");
    // Escape with the menu open closes the menu and nothing else.
    await rightClickWorld(page, canvas, 0, 50);
    await expect(menu(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);
  });
});
