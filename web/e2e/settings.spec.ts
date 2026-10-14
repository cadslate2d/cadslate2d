// User aliases and settings (#186): an alias defined through ALIASEDIT is
// kept across a reload and used; a conflict is refused by name; the toggles
// come up as last left; the theme chosen in the settings panel stays.
import { expect, test, type Page } from "@playwright/test";
import { enter, logLines, openApp, pixelsAround, prompt } from "./support";

const editor = (page: Page) => page.getByRole("dialog", { name: "Text editor" });
const field = (page: Page) => page.getByRole("textbox", { name: "Paragraph" });
const toggle = (page: Page, label: string) =>
  page.getByRole("button", { name: new RegExp(`^${label}`) });

test("an alias defined in the editor is kept across a reload and used; a conflict is refused", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "ALIASEDIT");
  await expect(editor(page)).toBeVisible();
  await field(page).fill("LL, LAYER\nE, LINE");
  await page.keyboard.press("Control+Enter");
  await expect(logLines(page).last()).toHaveText("1 alias defined.");
  await expect(
    logLines(page).filter({ hasText: 'Line 2: "E" is a built-in command name or alias.' }),
  ).toHaveCount(1);

  await page.reload();
  await openApp(page);
  await enter(page, "LL");
  await expect(prompt(page)).toContainText("Press Enter to finish or [?/New/Set");
  await page.keyboard.press("Escape");
  await enter(page, "E");
  await expect(prompt(page)).toContainText("Select objects");
});

test("the toggles come up as last left", async ({ page }) => {
  await openApp(page);
  await expect(toggle(page, "GRID")).toHaveAttribute("aria-pressed", "true");
  await expect(toggle(page, "ORTHO")).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("F7");
  await page.keyboard.press("F8");
  await expect(toggle(page, "GRID")).toHaveAttribute("aria-pressed", "false");
  await expect(toggle(page, "ORTHO")).toHaveAttribute("aria-pressed", "true");

  await page.reload();
  await openApp(page);
  await expect(toggle(page, "GRID")).toHaveAttribute("aria-pressed", "false");
  await expect(toggle(page, "ORTHO")).toHaveAttribute("aria-pressed", "true");
  await expect(toggle(page, "OSNAP")).toHaveAttribute("aria-pressed", "true");
});

// The drawing area follows the theme chosen, not the one left: the canvas
// reads its colours from the page, which must have the new theme first.
test("the drawing area takes the theme chosen, at once and after a reload", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  const background = async () => {
    // A patch of empty canvas, away from the crosshair.
    await page.mouse.move(2, 2);
    const light = await pixelsAround(
      page,
      canvas,
      { x: 150, y: 120 },
      { width: 3, height: 3 },
      ([red, green, blue]) => red > 200 && green > 200 && blue > 200,
    );
    const dark = await pixelsAround(
      page,
      canvas,
      { x: 150, y: 120 },
      { width: 3, height: 3 },
      ([red, green, blue]) => red < 60 && green < 60 && blue < 60,
    );
    return light === 36 ? "light" : dark === 36 ? "dark" : "mixed";
  };
  await expect.poll(background).toBe("dark");
  const choose = async (theme: "light" | "dark") => {
    await enter(page, "SETTINGS");
    const panel = page.getByRole("dialog", { name: "Settings" });
    await panel.getByLabel("Theme").selectOption(theme);
    await panel.getByRole("button", { name: "Close" }).click();
  };
  await choose("light");
  await expect.poll(background).toBe("light");
  await choose("dark");
  await expect.poll(background).toBe("dark");
  await choose("light");
  await page.reload();
  await openApp(page);
  await expect.poll(background).toBe("light");
});

test("the settings panel sets the theme, the unit, and the autosave delay, and they stay", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "SETTINGS");
  const panel = page.getByRole("dialog", { name: "Settings" });
  await expect(panel).toBeVisible();
  await panel.getByLabel("Theme").selectOption("light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await panel.getByLabel("Drawing unit").selectOption("mm");
  await expect(logLines(page).last()).toContainText("The drawing is in millimetres.");
  await panel.getByLabel("Autosave after (seconds)").fill("3");
  await panel.getByRole("button", { name: "Close" }).click();
  await expect(panel).toHaveCount(0);

  await page.reload();
  await openApp(page);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(panel.getByLabel("Drawing unit")).toHaveValue("mm");
  await expect(panel.getByLabel("Autosave after (seconds)")).toHaveValue("3");
  // The restored unit is the drawing's: a typed length converts.
  await panel.getByRole("button", { name: "Close" }).click();
  await enter(page, "L", "0,0", "2cm,0", "");
  await expect(prompt(page)).toHaveText("Command:");
});
