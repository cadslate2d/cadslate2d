import { expect, test, type Page } from "@playwright/test";
import { clickWorld, downloadFrom, enter, logLines, openApp, prompt } from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

/** Every LINE in a DXF export, as [x0, y0, x1, y1]. */
function lines(text: string): number[][] {
  const pattern = /LINE\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 11\n(.+)\n 21\n(.+)\n/g;
  return [...text.matchAll(pattern)].map((match) => match.slice(1).map(Number));
}

test.describe("BREAK", () => {
  test("removes the part of a line between two points, in one undo step", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    await enter(page, "BR");
    await expect(prompt(page)).toHaveText("Select object:");
    await clickWorld(page, canvas, 80, 0);
    await expect(prompt(page)).toHaveText("Specify second break point or [First]:");
    await enter(page, "F");
    await expect(prompt(page)).toHaveText("Specify first break point:");
    await enter(page, "20,0", "60,0");
    await expect(prompt(page)).toHaveText("Command:");

    expect(lines(await exported(page))).toEqual([
      [0, 0, 20, 0],
      [60, 0, 100, 0],
    ]);

    await page.keyboard.press("Control+z");
    expect(lines(await exported(page))).toEqual([[0, 0, 100, 0]]);
  });

  test("@ breaks a line in two where it was picked", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    await enter(page, "BR");
    await clickWorld(page, canvas, 40, 0);
    await enter(page, "@");
    await expect(prompt(page)).toHaveText("Command:");

    const [first, second] = lines(await exported(page));
    expect(first?.slice(0, 2)).toEqual([0, 0]);
    expect(second?.slice(2)).toEqual([100, 0]);
    // The two pieces meet where the line was picked.
    expect(first?.[2]).toBeCloseTo(40, 0);
    expect(second?.[0]).toBe(first?.[2]);
    expect(second?.[1]).toBeCloseTo(0, 12);
  });

  test("turns a circle into an arc, and refuses to break it at one point", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "C", "0,0", "50");
    await enter(page, "BR");
    await clickWorld(page, canvas, 50, 0);
    await enter(page, "@");
    await expect(logLines(page).last()).toHaveText(
      "A circle has no ends: it needs two different break points.",
    );
    await expect(prompt(page)).toHaveText("Specify second break point or [First]:");
    await enter(page, "F", "50,0", "0,50");

    const text = await exported(page);
    expect(text.match(/^CIRCLE$/gm)).toBeNull();
    const arc = /ARC\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 40\n(.+)\n 50\n(.+)\n 51\n(.+)\n/.exec(text);
    // What is left runs counter-clockwise from 90° round to 0°.
    const [x, y, radius, start, end] = arc?.slice(1).map(Number) ?? [];
    expect([x, y, radius]).toEqual([0, 0, 50]);
    expect(start).toBeCloseTo(90, 9);
    expect(Math.cos(((end ?? NaN) * Math.PI) / 180)).toBeCloseTo(1, 9);
  });

  test("refuses to break a line at its end", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    await enter(page, "BR");
    await clickWorld(page, canvas, 50, 0);
    await enter(page, "F", "100,0", "@");
    await expect(logLines(page).last()).toHaveText(
      "That is an end of the object: there is nothing to break off.",
    );
    await page.keyboard.press("Escape");
    expect(lines(await exported(page))).toEqual([[0, 0, 100, 0]]);
  });
});
