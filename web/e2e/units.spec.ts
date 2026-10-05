import { expect, test, type Page } from "@playwright/test";
import { chooseFile, downloadFrom, enter, logLines, openApp, prompt } from "./support";

const exported = async (page: Page) => await downloadFrom(page, () => enter(page, "DXFOUT"));

test.describe("UNITS", () => {
  test("a typed length with a unit is converted into the drawing's unit", async ({ page }) => {
    await openApp(page);
    await enter(page, "UNITS");
    await expect(prompt(page)).toHaveText("Drawing unit? [Unitless/MM/CM/M/IN/FT] <Unitless>:");
    await enter(page, "M");
    await expect(logLines(page).last()).toContainText("The drawing is in metres.");

    // 25 mm in a metre drawing is 0.025; a bare number is still metres.
    await enter(page, "L", "0,0", "25mm,0", "@0,2", "");
    const file = await exported(page);
    expect(file.text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n0.025\n 21\n0.0\n");
    expect(file.text).toContain(" 11\n0.025\n 21\n2.0\n");
    expect(file.text).toContain("$INSUNITS\n 70\n6\n");

    // The unit is part of the drawing: it comes back when the file is reopened.
    await page.reload();
    await openApp(page);
    await chooseFile(page, () => enter(page, "OPEN"), {
      name: file.name,
      mimeType: "application/dxf",
      buffer: Buffer.from(file.text),
    });
    await expect(logLines(page).last()).toHaveText("Drawing unit: metres.");
    await enter(page, "UNITS");
    await expect(prompt(page)).toHaveText("Drawing unit? [Unitless/MM/CM/M/IN/FT] <M>:");
    await enter(page, "");
    expect((await exported(page)).text).toBe(file.text);
  });

  test("a unit in a unitless drawing is refused with the reason, and undo restores the unit", async ({
    page,
  }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "25mm,0");
    await expect(logLines(page).last()).toHaveText(
      'This drawing has no unit, so "25mm" cannot be converted. Set one with UNITS.',
    );
    await page.keyboard.press("Escape");

    await enter(page, "UNITS", "MM");
    await enter(page, "C", "0,0", "1in");
    expect((await exported(page)).text).toContain(
      "CIRCLE\n  8\n0\n 10\n0.0\n 20\n0.0\n 40\n25.4\n",
    );

    await page.keyboard.press("Control+z"); // the circle
    await page.keyboard.press("Control+z"); // the unit
    await enter(page, "UNITS");
    await expect(prompt(page)).toHaveText("Drawing unit? [Unitless/MM/CM/M/IN/FT] <Unitless>:");
    await enter(page, ""); // keep it
    expect((await exported(page)).text).not.toContain("$INSUNITS");
  });
});
