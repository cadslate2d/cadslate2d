import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  hotGripPixels,
  hoverWorld,
  lines,
  litPixelsAround,
  logLines,
  openApp,
  polylines,
  prompt,
  rendererStats,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

test.describe("TRIM", () => {
  test("cuts a line back to the cutting edge on the side that is picked", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "40,-60", "40,60", "", "L", "0,0", "100,0", "", "TR");
    await expect(prompt(page)).toHaveText("Select cutting edges <all>:");
    await clickWorld(page, canvas, 40, 30);
    await enter(page, "");
    await expect(prompt(page)).toHaveText("Select object to trim:");
    await clickWorld(page, canvas, 80, 0);
    await enter(page, "");

    const text = await exported(page);
    expect(lines(text)).toContainEqual([0, 0, 40, 0]);
    expect(text.match(/^LINE$/gm)).toHaveLength(2);
  });

  test("opens a circle into an arc, and one undo restores it", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,-80", "0,80", "", "C", "0,0", "50", "TR");
    await clickWorld(page, canvas, 0, 70);
    await enter(page, "");
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
    await enter(page, "");
    await clickWorld(page, canvas, 80, -30);
    await expect(logLines(page).last()).toHaveText("The object does not cross a cutting edge.");
    await enter(page, "");
    expect(lines(await exported(page))).toContainEqual([60, -30, 100, -30]);
  });
});

test.describe("several edges", () => {
  test("Enter makes every object an edge; picks are cut between the edges around them", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(
      page,
      "L",
      "30,-60",
      "30,60",
      "",
      "L",
      "70,-60",
      "70,60",
      "",
      "L",
      "0,0",
      "100,0",
      "",
    );
    await enter(page, "TR", "");
    await expect(logLines(page).last()).toHaveText("Every object is an edge.");
    await clickWorld(page, canvas, 50, 0); // between the two uprights
    await enter(page, "");

    const text = await exported(page);
    expect(text.match(/^LINE$/gm)).toHaveLength(4);
    expect(lines(text)).toContainEqual([0, 0, 30, 0]);
    expect(lines(text)).toContainEqual([70, 0, 100, 0]);
  });

  test("two chosen edges can trim each other", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "-50,0", "50,0", "", "L", "0,-50", "0,50", "", "TR");
    await clickWorld(page, canvas, -30, 0);
    await clickWorld(page, canvas, 0, 30);
    await enter(page, "");
    await clickWorld(page, canvas, 30, 0);
    await clickWorld(page, canvas, 0, -30);
    await enter(page, "");

    const text = await exported(page);
    expect(lines(text)).toContainEqual([-50, 0, 0, 0]);
    expect(lines(text)).toContainEqual([0, 0, 0, 50]);
  });
});

test.describe("polylines", () => {
  test("a rectangle is opened by trimming, and a polyline end is extended", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "REC", "0,0", "100,60", "L", "50,-20", "50,80", "", "TR");
    await clickWorld(page, canvas, 50, 70);
    await enter(page, "");
    await clickWorld(page, canvas, 100, 30); // the right side goes
    await enter(page, "");

    let text = await exported(page);
    expect(polylines(text)[0]?.vertices).toHaveLength(4);
    expect(polylines(text)[0]?.closed).toBe(false); // now open
    const vertices = polylines(text)[0]?.vertices.map(([x, y]) => [x, y]);
    expect(vertices).toContainEqual([50, 60]);
    expect(vertices).toContainEqual([50, 0]);

    // Extend the open polyline's lower end to a wall on the right.
    await enter(page, "L", "150,-40", "150,40", "", "EX", "");
    await clickWorld(page, canvas, 45, 0);
    await enter(page, "");
    text = await exported(page);
    expect(polylines(text)[0]?.vertices.map(([x, y]) => [x, y])).toContainEqual([150, 0]);
  });
});

test.describe("hover feedback", () => {
  test("TRIM shows in red what a click would cut away, without committed uploads", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "40,-60", "40,60", "", "L", "0,0", "100,0", "", "TR");
    await clickWorld(page, canvas, 40, 30);
    await enter(page, "");
    const before = await rendererStats(page);
    await hoverWorld(page, canvas, 150, 100);
    expect(await hotGripPixels(canvas)).toBe(0);

    // Right of the edge: the 60 units from the edge to the end would go.
    await hoverWorld(page, canvas, 80, 1);
    const right = await hotGripPixels(canvas);
    expect(right).toBeGreaterThan(60);
    // Left of it: the shorter 40 units.
    await hoverWorld(page, canvas, 10, 1);
    const left = await hotGripPixels(canvas);
    expect(left).toBeGreaterThan(30);
    expect(left).toBeLessThan(right);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);

    // The click removes exactly what was shown, and the red goes with it.
    await clickWorld(page, canvas, 10, 1);
    await hoverWorld(page, canvas, 150, 100);
    expect(await hotGripPixels(canvas)).toBe(0);
    await enter(page, "");
    expect(lines(await exported(page))).toContainEqual([40, 0, 100, 0]);
  });

  test("EXTEND shows the object as a click would leave it", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "100,-80", "100,80", "", "L", "0,0", "60,0", "", "EX");
    await clickWorld(page, canvas, 100, 40);
    await enter(page, "");
    // The gap between the line's end at x = 60 and the boundary at x = 100
    // is empty until the line is hovered; then the extension is drawn in.
    const gap = { x: 80, y: 0 };
    const size = { width: 12, height: 3 };
    await hoverWorld(page, canvas, 150, 100);
    expect(await litPixelsAround(page, canvas, gap, size)).toBe(0);
    await hoverWorld(page, canvas, 55, 1);
    expect(await litPixelsAround(page, canvas, gap, size)).toBeGreaterThan(15);
  });
});

test.describe("EXTEND", () => {
  test("carries a line on to the boundary edge", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "100,-80", "100,80", "", "L", "0,0", "60,0", "", "EX");
    await expect(prompt(page)).toHaveText("Select boundary edges <all>:");
    await clickWorld(page, canvas, 100, 40);
    await enter(page, "");
    await expect(prompt(page)).toHaveText("Select object to extend:");
    await clickWorld(page, canvas, 55, 0);
    await enter(page, "");
    expect(lines(await exported(page))).toContainEqual([0, 0, 100, 0]);
  });

  test("says when the object would never meet the boundary", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "100,20", "100,80", "", "L", "0,0", "60,0", "", "EX");
    await clickWorld(page, canvas, 100, 50);
    await enter(page, "");
    await clickWorld(page, canvas, 55, 0);
    await expect(logLines(page).last()).toHaveText(
      "The object does not meet a boundary edge when extended.",
    );
    await enter(page, "");
    expect(await exported(page)).toContain(" 11\n60.0\n 21\n0.0\n");
  });
});
