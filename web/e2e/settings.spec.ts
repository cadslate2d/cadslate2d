// User aliases and settings (#186): an alias defined through ALIASEDIT is
// kept across a reload and used; a conflict is refused by name; the toggles
// come up as last left; the theme chosen in the settings panel stays.
import { expect, test, type Page } from "@playwright/test";
import { enter, logLines, openApp, prompt } from "./support";

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
