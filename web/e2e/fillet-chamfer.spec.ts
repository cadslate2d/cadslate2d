import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  arcs,
  circles,
  clickWorld,
  downloadFrom,
  enter,
  entitiesSection,
  lines,
  logLines,
  openApp,
  polylines,
  prompt,
} from "./support";

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
    await expect(prompt(page)).toHaveText("Select first object or [Radius/Polyline]:");
    await enter(page, "R");
    await expect(prompt(page)).toHaveText("Specify fillet radius <0>:");
    await enter(page, "20");
    await clickWorld(page, canvas, 50, 0);
    await expect(prompt(page)).toHaveText("Select second object:");
    await clickWorld(page, canvas, 0, 50);
    await expect(prompt(page)).toHaveText("Command:");

    const text = await exported(page);
    expect(lines(text)).toContainEqual([20, 0, 100, 0]);
    expect(lines(text)).toContainEqual([0, 20, 0, 80]);
    // The arc: centre (20, 20), radius 20, from 180° to 270°.
    const arc = arcs(text)[0] ?? [];
    const expected = [20, 20, 20, 180, 270];
    expect(arc.length).toBe(expected.length);
    arc.forEach((value, index) => expect(value).toBeCloseTo(expected[index] ?? NaN, 9));
  });

  test("with radius zero makes a sharp corner, and one undo takes it back", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "F");
    await pickBoth(page, canvas);
    let text = await exported(page);
    expect(lines(text)).toContainEqual([0, 0, 100, 0]);
    expect(text.match(/^ARC$/gm)).toBeNull();

    await page.keyboard.press("Control+z");
    text = await exported(page);
    expect(lines(text)).toContainEqual([10, 0, 100, 0]);
  });

  test("refuses a radius that is too large", async ({ page }) => {
    const canvas = await lShape(page);
    await enter(page, "F", "R", "500");
    await pickBoth(page, canvas);
    await expect(logLines(page).last()).toHaveText("The radius is too large for these lines.");
    await expect(prompt(page)).toHaveText("Select first object or [Radius/Polyline]:");
  });
});

test.describe("FILLET with curves", () => {
  test("joins a line to a circle with a tangent arc and leaves the circle whole", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "-200,0", "200,0", "", "C", "0,40", "30", "F", "R", "10");
    await clickWorld(page, canvas, 60, 0);
    await clickWorld(page, canvas, 30, 40);
    await expect(prompt(page)).toHaveText("Command:");

    const text = await exported(page);
    expect(circles(text)).toContainEqual([0, 40, 30]);
    // The fillet's centre: 10 above the line, 40 from the circle's centre.
    const [cx, cy, radius] = arcs(text)[0] ?? [];
    expect(cx).toBeCloseTo(Math.sqrt(700), 9);
    expect(cy).toBeCloseTo(10, 9);
    expect(radius).toBeCloseTo(10, 9);
  });

  test("says when no arc of the radius fits", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "C", "-50,0", "30", "C", "50,0", "30", "F", "R", "15");
    await clickWorld(page, canvas, -50, 30);
    await clickWorld(page, canvas, 50, 30);
    await expect(logLines(page).last()).toHaveText("No fillet of that radius fits these objects.");
  });
});

test.describe("corners of a polyline", () => {
  test("two picks round one corner; the Polyline option rounds them all", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "REC", "0,0", "100,60", "F", "R", "10");
    await clickWorld(page, canvas, 50, 0);
    await clickWorld(page, canvas, 100, 30);
    let text = await exported(page);
    expect(text.match(/^LWPOLYLINE$/gm)).toHaveLength(1);
    expect(polylines(text)[0]?.vertices).toHaveLength(5);
    expect(entitiesSection(text).match(/^ 42$/gm)).toHaveLength(1);

    await page.keyboard.press("Control+z");
    await enter(page, "F", "P");
    await expect(prompt(page)).toHaveText("Select polyline:");
    await clickWorld(page, canvas, 50, 0);
    await expect(logLines(page).last()).toHaveText("4 corners filleted, 0 skipped.");
    text = await exported(page);
    expect(polylines(text)[0]?.vertices).toHaveLength(8);
    expect(entitiesSection(text).match(/^ 42$/gm)).toHaveLength(4);
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
    expect(lines(text)).toContainEqual([30, 0, 0, 5]);
  });

  test("refuses parallel lines", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "", "L", "0,40", "100,40", "", "CHA");
    await clickWorld(page, canvas, 50, 0);
    await clickWorld(page, canvas, 50, 40);
    await expect(logLines(page).last()).toHaveText("The lines are parallel.");
  });
});
