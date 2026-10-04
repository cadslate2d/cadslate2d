import { expect, test, type Locator, type Page } from "@playwright/test";
import { clickWorld, downloadFrom, enter, logLines, openApp, prompt } from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

/** Two lines that would meet at the origin at a right angle. */
async function lShape(page: Page): Promise<Locator> {
  const canvas = await openApp(page);
  await enter(page, "L", "10,0", "100,0", "", "L", "0,10", "0,80", "");
  return canvas;
}

async function pickBoth(page: Page, canvas: Locator): Promise<void> {
  await clickWorld(page, canvas, 50, 0);
  await clickWorld(page, canvas, 0, 50);
}

test.describe("FILLET", () => {
  test("joins two lines with a tangent arc of the set radius", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "F");
    await expect(prompt(page)).toHaveText("Select first line or [Radius]:");
    await enter(page, "R");
    await expect(prompt(page)).toHaveText("Specify fillet radius <0>:");
    await enter(page, "20");
    await clickWorld(page, canvas, 50, 0);
    await expect(prompt(page)).toHaveText("Select second line:");
    await clickWorld(page, canvas, 0, 50);
    await expect(prompt(page)).toHaveText("Command:");

    const text = await exported(page);
    expect(text).toContain("LINE\n  8\n0\n 10\n20.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n");
    expect(text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n20.0\n 11\n0.0\n 21\n80.0\n");
    // The arc: centre (20, 20), radius 20, from 180° to 270°.
    const arc = /ARC\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 40\n(.+)\n 50\n(.+)\n 51\n(.+)\n/.exec(text);
    const expected = [20, 20, 20, 180, 270];
    expect(arc?.slice(1).map(Number).length).toBe(expected.length);
    arc
      ?.slice(1)
      .forEach((value, index) => expect(Number(value)).toBeCloseTo(expected[index] ?? NaN, 9));
  });

  test("with radius zero makes a sharp corner, and one undo takes it back", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "F");
    await pickBoth(page, canvas);
    let text = await exported(page);
    expect(text).toContain(" 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n");
    expect(text.match(/^ARC$/gm)).toBeNull();

    await page.keyboard.press("Control+z");
    text = await exported(page);
    expect(text).toContain(" 10\n10.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n");
  });

  test("refuses a radius that is too large", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "F", "R", "500");
    await pickBoth(page, canvas);
    await expect(logLines(page).last()).toHaveText("The radius is too large for these lines.");
    await expect(prompt(page)).toHaveText("Select first line or [Radius]:");
  });
});

test.describe("CHAMFER", () => {
  test("cuts both lines back by their distances and joins them", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "CHA", "D");
    await expect(prompt(page)).toHaveText("Specify first chamfer distance <0>:");
    await enter(page, "30");
    await expect(prompt(page)).toHaveText("Specify second chamfer distance <30>:");
    await enter(page, "5");
    await pickBoth(page, canvas);

    const text = await exported(page);
    expect(text.match(/^LINE$/gm)).toHaveLength(3);
    expect(text).toContain("LINE\n  8\n0\n 10\n30.0\n 20\n0.0\n 11\n0.0\n 21\n5.0\n");
  });

  test("refuses parallel lines", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "", "L", "0,40", "100,40", "", "CHA");
    await clickWorld(page, canvas, 50, 0);
    await clickWorld(page, canvas, 50, 40);
    await expect(logLines(page).last()).toHaveText("The lines are parallel.");
  });
});
