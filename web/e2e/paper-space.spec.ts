// Paper space (#193): a layout made and drafted on, the model untouched;
// the tabs go between spaces and keep each one's view; MSPACE works the
// model through a viewport and PSPACE hands it the view.
import { expect, test, type Page } from "@playwright/test";
import { downloadFrom, enter, entities, logLines, openApp, prompt } from "./support";

const tab = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Layouts" }).getByRole("button", { name, exact: true });
const space = (page: Page) => page.getByTestId("space");

test("drafting on a layout leaves the model as it was, and the tabs go between them", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "L", "0,0", "100,0", "");
  await expect(tab(page, "Model")).toHaveAttribute("aria-pressed", "true");
  await expect(space(page)).toHaveText("MODEL");

  await page.getByRole("button", { name: "New layout" }).click();
  await expect(prompt(page)).toHaveText("Enter name for the new layout:");
  await enter(page, "Sheet");
  await expect(logLines(page).last()).toHaveText('Layout "Sheet" made.');
  await expect(tab(page, "Sheet")).toHaveAttribute("aria-pressed", "true");
  await expect(tab(page, "Model")).toHaveAttribute("aria-pressed", "false");
  await expect(space(page)).toHaveText("PAPER");

  // Drafted on the sheet: a title line and a viewport onto the model.
  await enter(page, "L", "20,20", "400,20", "");
  await enter(page, "MVIEW", "20,40", "400,280");
  await expect(logLines(page).last()).toContainText("Viewport at 1:");

  // Back on the model: its one line, and the sheet's are not there.
  await tab(page, "Model").click();
  await expect(space(page)).toHaveText("MODEL");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("1 selected.");
  await page.keyboard.press("Escape");

  // Through the viewport, the model is worked; PSPACE returns.
  await tab(page, "Sheet").click();
  await expect(space(page)).toHaveText("PAPER");
  await space(page).click();
  await expect(logLines(page).filter({ hasText: "Command: MSPACE" })).toHaveCount(1);
  await expect(space(page)).toHaveText("MODEL");
  await enter(page, "L", "0,50", "100,50", "");
  await space(page).click();
  await expect(space(page)).toHaveText("PAPER");

  // Saved: two lines in the model, the sheet's line and viewport in its block.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "LINE")).toHaveLength(2);
  expect(entities(file.text, "LINE", "BLOCKS")).toHaveLength(1);
  expect(entities(file.text, "VIEWPORT", "BLOCKS")).toHaveLength(1);
});

test("a layout with two viewports at different scales is drawn, the model through each", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "baselines are compared in Chromium only");
  await openApp(page);
  await page.keyboard.press("F7");
  // The model: a square with a cross, 100 across.
  await enter(page, "REC", "0,0", "100,100");
  await enter(page, "L", "0,0", "100,100", "", "L", "0,100", "100,0", "");
  await enter(page, "C", "50,50", "20");
  await enter(page, "LAYOUT", "N", "Sheet");
  // Two viewports on the sheet: one fitted, one at half the size.
  await enter(page, "MVIEW", "20,20", "220,220", "MVIEW", "250,20", "350,120");
  await enter(page, "L", "20,250", "350,250", "");
  await enter(page, "ZOOM", "E");
  await page.mouse.move(5, 5);
  await expect(page).toHaveScreenshot("layout-two-viewports.png", {
    clip: await clipOfCanvas(page),
    maxDiffPixelRatio: 0.002,
  });
});

/** The canvas's rectangle on the page, for a screenshot of it alone. */
async function clipOfCanvas(
  page: Page,
): Promise<{ x: number; y: number; width: number; height: number }> {
  const box = await page.locator("canvas").boundingBox();
  if (!box) throw new Error("no canvas");
  return { x: box.x, y: box.y, width: box.width, height: box.height };
}
