// The Properties panel (#141): what is selected and what it shares, and a
// change made there is CHPROP through the command line.
import { expect, test, type Page } from "@playwright/test";
import { clickWorld, downloadFrom, enter, entities, group, logLines, openApp } from "./support";

const panel = (page: Page) => page.getByRole("complementary", { name: "Properties" });
const headline = (page: Page) => page.getByTestId("selection-headline");
/** The button showing a property's shared value; its label names it. */
const value = (page: Page, property: string) =>
  panel(page).getByRole("button", { name: property, exact: true });

test.describe("Properties panel", () => {
  test("shows the selection and changes its layer through CHPROP", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "LAYER", "N", "walls", "", "L", "0,0", "100,0", "", "C", "50,50", "20");
    await expect(headline(page)).toHaveText("No selection");

    await clickWorld(page, canvas, 50, 0);
    await expect(headline(page)).toHaveText("1 line");
    await expect(value(page, "Layer")).toHaveText("0");
    await expect(value(page, "Color")).toHaveText("ByLayer");
    // The end "100.0000, 0.0000" and the length "100.0000".
    await expect(panel(page).getByText("100.0000", { exact: true })).toBeVisible();

    // A second click adds the circle: two kinds, the layer still shared.
    await clickWorld(page, canvas, 70, 50);
    await expect(headline(page)).toHaveText("2 objects: 1 line, 1 circle");
    await expect(panel(page).getByText("100.0000")).toHaveCount(0);

    await value(page, "Layer").click();
    const field = page.getByRole("combobox", { name: "Layer" });
    await field.fill("walls");
    await page.keyboard.press("Enter");
    await expect(
      logLines(page).filter({ hasText: '2 objects changed to layer "walls".' }),
    ).toHaveCount(1);
    await expect(value(page, "Layer")).toHaveText("walls");

    await value(page, "Color").click();
    await page.getByRole("combobox", { name: "Color" }).fill("red");
    await page.keyboard.press("Enter");
    await expect(logLines(page).filter({ hasText: "2 objects changed to red." })).toHaveCount(1);

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    for (const kind of ["LINE", "CIRCLE"]) {
      const entity = entities(file.text, kind)[0];
      expect([group(entity, 8), group(entity, 62)]).toEqual(["walls", "1"]);
    }

    // Give one of them another colour: the panel says they vary.
    await page.keyboard.press("Escape");
    await clickWorld(page, canvas, 50, 0);
    await enter(page, "CHPROP", "C", "blue", "");
    await page.keyboard.press("Escape");
    await clickWorld(page, canvas, 50, 0);
    await clickWorld(page, canvas, 70, 50);
    await expect(headline(page)).toHaveText("2 objects: 1 line, 1 circle");
    await expect(value(page, "Color")).toHaveText("*Varies*");
    await expect(value(page, "Layer")).toHaveText("walls");
  });
});
