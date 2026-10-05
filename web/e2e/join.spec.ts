import { expect, test, type Page } from "@playwright/test";
import { clickWorld, downloadFrom, enter, logLines, openApp, prompt } from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

const count = (text: string, entity: string) =>
  text.match(new RegExp(`^${entity}$`, "gm"))?.length ?? 0;

test.describe("JOIN", () => {
  test("puts a broken line back together, across the gap", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    await enter(page, "BR");
    await clickWorld(page, canvas, 80, 0);
    await enter(page, "F", "20,0", "60,0");
    expect(count(await exported(page), "LINE")).toBe(2);

    await enter(page, "J");
    await expect(prompt(page)).toHaveText("Select objects to join or [Close]:");
    await clickWorld(page, canvas, 10, 0);
    await clickWorld(page, canvas, 80, 0);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText("2 objects joined into 1 line.");
    await expect(prompt(page)).toHaveText("Command:");

    const text = await exported(page);
    expect(count(text, "LINE")).toBe(1);
    expect(text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n");

    await page.keyboard.press("Control+z");
    expect(count(await exported(page), "LINE")).toBe(2);
  });

  test("turns lines and an arc that meet end to end into one polyline", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "60,0", "");
    // An arc from (60, 0) up to (60, 40), bulging to the right.
    await enter(page, "A", "60,0", "80,20", "60,40");
    await enter(page, "L", "60,40", "0,40", "");

    await enter(page, "J");
    await clickWorld(page, canvas, 30, 0);
    await clickWorld(page, canvas, 80, 20);
    await clickWorld(page, canvas, 30, 40);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText("3 objects joined into 1 polyline.");

    const text = await exported(page);
    expect(count(text, "LINE")).toBe(0);
    expect(count(text, "ARC")).toBe(0);
    expect(count(text, "POLYLINE")).toBe(1);
    expect(count(text, "VERTEX")).toBe(4);
    // The half circle is the polyline's one arc segment: a bulge of 1,
    // starting at (60, 0).
    const arc = /VERTEX\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 42\n(.+)\n/.exec(text);
    const [x, y, bulge] = arc?.slice(1).map(Number) ?? [];
    expect(x).toBeCloseTo(60, 9);
    expect(y).toBeCloseTo(0, 9);
    expect(bulge).toBeCloseTo(1, 9);
  });

  test("Close makes the circle an arc is part of", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "A", "50,0", "0,50", "-50,0");
    await enter(page, "J");
    await clickWorld(page, canvas, 0, 50);
    await enter(page, "C");
    await expect(logLines(page).last()).toHaveText("1 arc closed into a circle.");

    const text = await exported(page);
    expect(count(text, "ARC")).toBe(0);
    expect(count(text, "CIRCLE")).toBe(1);
  });

  test("says so when nothing meets, and changes nothing", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "60,0", "", "L", "0,30", "60,30", "");
    await enter(page, "J");
    await clickWorld(page, canvas, 30, 0);
    await clickWorld(page, canvas, 30, 30);
    await enter(page, "");
    await expect(logLines(page).last()).toHaveText(
      "Nothing to join: none of these objects meet end to end.",
    );
    expect(count(await exported(page), "LINE")).toBe(2);
  });
});
