// The Layers panel (#140): a second way to do what LAYER does. Every action
// goes through the command line, so the log shows it and the export has it.
import { expect, test, type Page } from "@playwright/test";
import { LAYER_COLOR_CHOICES, LAYER_LINEWEIGHT_CHOICES } from "../src/ui/layer-actions";
import {
  chooseMenu,
  downloadFrom,
  enter,
  expectOffers,
  group,
  menuEntry,
  logLines,
  openApp,
  prompt,
  tableRow,
  clickWorld,
} from "./support";

const panel = (page: Page) => page.getByRole("complementary", { name: "Layers" });
const row = (page: Page, name: string) =>
  panel(page)
    .getByRole("row")
    .filter({ has: page.getByRole("button", { name: `Make ${name} current` }) });

test.describe("Layers panel", () => {
  test("creates, sets current, locks, and renames a layer, and the export shows it", async ({
    page,
  }) => {
    await openApp(page);
    await expect(panel(page).getByRole("row")).toHaveCount(2); // the header and layer 0
    await expect(row(page, "0")).toHaveAttribute("aria-current", "true");

    await panel(page).getByRole("button", { name: "New layer" }).click();
    await page.getByRole("textbox", { name: "Name of the new layer" }).fill("walls");
    await page.keyboard.press("Enter");
    await expect(logLines(page).filter({ hasText: "1 layer created." })).toHaveCount(1);
    await expect(row(page, "walls")).toBeVisible();

    await panel(page).getByRole("button", { name: "Make walls current" }).click();
    await expect(logLines(page).filter({ hasText: 'Current layer: "walls"' })).toHaveCount(1);
    await expect(row(page, "walls")).toHaveAttribute("aria-current", "true");
    await expect(row(page, "0")).not.toHaveAttribute("aria-current", "true");

    await panel(page).getByRole("button", { name: "Lock walls" }).click();
    await expect(logLines(page).filter({ hasText: "1 layer locked." })).toHaveCount(1);
    await expect(panel(page).getByRole("button", { name: "Unlock walls" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await panel(page).getByRole("button", { name: "Switch off 0" }).click();
    await expect(logLines(page).filter({ hasText: "1 layer switched off." })).toHaveCount(1);
    await expect(panel(page).getByRole("button", { name: "Switch on 0" })).toBeVisible();

    await panel(page).getByRole("button", { name: "Rename walls" }).click();
    const field = page.getByRole("textbox", { name: "New name for walls" });
    await field.fill("partitions");
    await page.keyboard.press("Enter");
    await expect(
      logLines(page).filter({ hasText: 'Layer "walls" renamed "partitions".' }),
    ).toHaveCount(1);
    await expect(row(page, "partitions")).toHaveAttribute("aria-current", "true");

    // The command line is still the drawing's: the panel typed for us.
    await expect(prompt(page)).toHaveText("Command:");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain("  9\n$CLAYER\n  8\npartitions\n");
    const partitions = tableRow(file.text, "LAYER", "partitions");
    expect([group(partitions, 70), group(partitions, 62)]).toEqual(["4", "7"]);
    expect(group(tableRow(file.text, "LAYER", "0"), 62)).toBe("-7");

    // A refusal is explained, like a typed one.
    await panel(page).getByRole("button", { name: "Delete partitions" }).click();
    await expect(
      logLines(page).filter({
        hasText:
          'Layer "partitions" is the current layer: it cannot be switched off, frozen, or deleted.',
      }),
    ).toHaveCount(1);
    // Layer 0 is permanent: nothing offers to delete or rename it.
    await expect(panel(page).getByRole("button", { name: "Delete 0" })).toBeDisabled();
    await expect(panel(page).getByRole("button", { name: "Rename 0" })).toBeDisabled();
  });

  test("waits while a command is in progress, and can be hidden", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0");
    await expect(panel(page).getByRole("button", { name: "New layer" })).toBeDisabled();
    await expect(panel(page).getByRole("button", { name: "Switch off 0" })).toBeDisabled();
    for (const column of ["Color", "Linetype", "Lineweight"]) {
      await expect(panel(page).getByRole("button", { name: `${column} of 0` })).toBeDisabled();
    }
    await expect(
      panel(page).getByText("Finish the current command to change layers."),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel(page).getByRole("button", { name: "New layer" })).toBeEnabled();

    // Escape in the name field gives up without typing anything.
    await panel(page).getByRole("button", { name: "New layer" }).click();
    await page.getByRole("textbox", { name: "Name of the new layer" }).fill("doors");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("textbox", { name: "Name of the new layer" })).toHaveCount(0);
    await expect(logLines(page).filter({ hasText: "created" })).toHaveCount(0);

    // View › Layers Panel, checked while the panel shows.
    await page
      .getByRole("menubar", { name: "Menus" })
      .getByRole("menuitem", { name: "View" })
      .click();
    await expect(menuEntry(page, 0, "Layers Panel")).toHaveAttribute("aria-checked", "true");
    await menuEntry(page, 0, "Layers Panel").click();
    await expect(panel(page)).toHaveCount(0);
    await page.reload();
    await openApp(page);
    await expect(panel(page)).toHaveCount(0);
    await chooseMenu(page, "View", "Layers Panel");
    await expect(panel(page)).toBeVisible();
  });

  test("every column of a layer's row changes it: colour, linetype, and lineweight too", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "LINETYPE", "L", "Dashed", "");
    await enter(page, "L", "0,0", "100,0", "");
    const cell = (column: string) => panel(page).getByRole("button", { name: `${column} of 0` });
    const field = (column: string) => page.getByRole("combobox", { name: `${column} of 0` });
    const said = (text: string) => expect(logLines(page).filter({ hasText: text })).toHaveCount(1);

    await expect(cell("Color")).toHaveText("7");
    await cell("Color").click();
    await expectOffers(field("Color"), "7", LAYER_COLOR_CHOICES);
    await field("Color").fill("red");
    await page.keyboard.press("Enter");
    await said("1 layer set to colour 1.");
    await expect(cell("Color")).toHaveText("1");

    await expect(cell("Linetype")).toHaveText("Continuous");
    await cell("Linetype").click();
    await expectOffers(field("Linetype"), "Continuous", ["Continuous", "Dashed"]);
    await field("Linetype").fill("Dashed");
    await page.keyboard.press("Enter");
    await said("1 layer set to linetype Dashed.");
    await expect(cell("Linetype")).toHaveText("Dashed");

    await expect(cell("Lineweight")).toHaveText("Default");
    await cell("Lineweight").click();
    await expectOffers(field("Lineweight"), "Default", LAYER_LINEWEIGHT_CHOICES);
    await field("Lineweight").fill("0.35");
    await page.keyboard.press("Enter");
    await said("1 layer set to lineweight 0.35 mm.");
    await expect(cell("Lineweight")).toHaveText("0.35 mm");

    // A built-in linetype the drawing has not loaded is loaded with it (#356).
    await cell("Linetype").click();
    await field("Linetype").fill("HIDDEN");
    await page.keyboard.press("Enter");
    await expect(cell("Linetype")).toHaveText(/hidden/i);

    // A value the layer cannot take is refused in the log, nothing changes,
    // and the command line is free again: the layer's name, the panel's next
    // answer, is not taken for a lineweight.
    await cell("Lineweight").click();
    await field("Lineweight").fill("ByLayer");
    await page.keyboard.press("Enter");
    await said("A layer needs a lineweight of its own, not ByLayer or ByBlock.");
    await expect(prompt(page)).toHaveText("Command:");
    await expect(cell("Lineweight")).toHaveText("0.35 mm");

    // The line drawn on layer 0 follows it, and the export has the layer so.
    await clickWorld(page, canvas, 50, 0);
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    const zero = tableRow(file.text, "LAYER", "0");
    expect([group(zero, 62), group(zero, 370)]).toEqual(["1", "35"]);
    expect(group(zero, 6)?.toUpperCase()).toBe("HIDDEN");
  });
});
