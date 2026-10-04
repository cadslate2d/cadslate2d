import { expect, test, type Page } from "@playwright/test";
import { clickWorld, downloadFrom, enter, logLines, openApp, prompt } from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

test.describe("TRIM", () => {
  test("cuts a line back to the cutting edge on the side that is picked", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "40,-60", "40,60", "", "L", "0,0", "100,0", "", "TR");
    await expect(prompt(page)).toHaveText("Select cutting edge:");
    await clickWorld(page, canvas, 40, 30);
    await expect(prompt(page)).toHaveText("Select object to trim:");
    await clickWorld(page, canvas, 80, 0);
    await enter(page, "");

    const text = await exported(page);
    expect(text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n40.0\n 21\n0.0\n");
    expect(text.match(/^LINE$/gm)).toHaveLength(2);
  });

  test("opens a circle into an arc, and one undo restores it", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,-80", "0,80", "", "C", "0,0", "50", "TR");
    await clickWorld(page, canvas, 0, 70);
    await clickWorld(page, canvas, 50, 0);
    await enter(page, "");

    let text = await exported(page);
    expect(text.match(/^ARC$/gm)).toHaveLength(1);
    expect(text.match(/^CIRCLE$/gm)).toBeNull();

    await page.keyboard.press("Control+z");
    text = await exported(page);
    expect(text.match(/^CIRCLE$/gm)).toHaveLength(1);
    expect(text.match(/^ARC$/gm)).toBeNull();
  });

  test("says why a pick cannot be trimmed and changes nothing", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "40,-60", "40,60", "", "L", "60,-30", "100,-30", "", "TR");
    await clickWorld(page, canvas, 40, 30);
    await clickWorld(page, canvas, 80, -30);
    await expect(logLines(page).last()).toHaveText("The object does not cross the cutting edge.");
    await enter(page, "");
    expect(await exported(page)).toContain(" 10\n60.0\n 20\n-30.0\n 11\n100.0\n 21\n-30.0\n");
  });
});

test.describe("EXTEND", () => {
  test("carries a line on to the boundary edge", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "100,-80", "100,80", "", "L", "0,0", "60,0", "", "EX");
    await expect(prompt(page)).toHaveText("Select boundary edge:");
    await clickWorld(page, canvas, 100, 40);
    await expect(prompt(page)).toHaveText("Select object to extend:");
    await clickWorld(page, canvas, 55, 0);
    await enter(page, "");
    expect(await exported(page)).toContain(
      "LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n",
    );
  });

  test("says when the object would never meet the boundary", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "100,20", "100,80", "", "L", "0,0", "60,0", "", "EX");
    await clickWorld(page, canvas, 100, 50);
    await clickWorld(page, canvas, 55, 0);
    await expect(logLines(page).last()).toHaveText(
      "The object does not meet the boundary edge when extended.",
    );
    await enter(page, "");
    expect(await exported(page)).toContain(" 11\n60.0\n 21\n0.0\n");
  });
});
