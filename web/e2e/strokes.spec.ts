// Strokes where they are hardest (#174): a polyline's joins — sharp,
// shallow, and reversing — at light and heavy lineweights, a dashed
// polyline turning corners, a hatch of dots, and a selected polyline. Each
// has a baseline, and a join may draw no darker than the stroke: with an
// opaque colour, two capsules over one another are the colour once.
import { expect, test, type Page } from "@playwright/test";
import { centerOf, clickWorld, enter, openApp, pixelsAround, prompt } from "./support";

/** A polyline with a sharp join (37°), a shallow one (15°), and a reversal. */
const JOINS = ["-80,-20", "0,-20", "-40,10", "-90,30", "-65,20"];
/** Where the joins polyline is, in world units, and how big. */
const AROUND = { x: -45, y: 5 };
const CLIP = { width: 200, height: 120 };
/** The entity colour of the dark theme (`--canvas-entity`). */
const ENTITY = [241, 244, 248] as const;

const lwt = (page: Page) => page.getByRole("button", { name: "LWT", exact: true });

/** Compares a clip of `size` around the world point `at` with its baseline. */
async function matches(
  page: Page,
  name: string,
  at: { x: number; y: number },
  size: { width: number; height: number },
): Promise<void> {
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  const center = await centerOf(canvas);
  await page.mouse.move(5, 5);
  await expect(page).toHaveScreenshot(name, {
    clip: {
      x: Math.round(center.x + at.x) - size.width / 2,
      y: Math.round(center.y - at.y) - size.height / 2,
      width: size.width,
      height: size.height,
    },
    // A join drawn differently moves hundreds of pixels; driver
    // antialiasing noise a handful.
    maxDiffPixels: 60,
  });
}

/** Pixels around the joins polyline brighter than its colour: a join drawn twice would be. */
function brighterThanTheStroke(page: Page): Promise<number> {
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  return pixelsAround(
    page,
    canvas,
    AROUND,
    { width: CLIP.width / 2, height: CLIP.height / 2 },
    ([red, green, blue]) => red > ENTITY[0] + 2 || green > ENTITY[1] + 2 || blue > ENTITY[2] + 2,
  );
}

test.describe("strokes", () => {
  test.beforeEach(({ browserName }) => {
    test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
  });

  for (const weight of ["0.25", "0.70", "2.11"]) {
    test(`joins at ${weight} mm`, async ({ page }) => {
      await openApp(page);
      await page.keyboard.press("F7");
      await lwt(page).click();
      await enter(page, "LWEIGHT", weight);
      await enter(page, "PL", ...JOINS, "");
      await expect(prompt(page)).toHaveText("Command:");
      await matches(page, `joins-${weight.replace(".", "")}mm.png`, AROUND, CLIP);
      expect(await brighterThanTheStroke(page), "no join is drawn twice").toBe(0);
    });
  }

  test("a dashed polyline turning corners", async ({ page }) => {
    await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "LINETYPE", "L", "Dashed", "S", "Dashed", "", "LTSCALE", "20");
    await enter(page, "PL", "-70,-40", "70,-40", "70,40", "-70,40", "");
    await expect(prompt(page)).toHaveText("Command:");
    await matches(page, "joins-dashed.png", { x: 0, y: 0 }, { width: 170, height: 110 });
  });

  test("a hatch of dots", async ({ page }) => {
    await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "PL", "-70,-40", "70,-40", "70,40", "-70,40", "C");
    await enter(page, "HATCH", "P", "DOTS", "SC", "4", "0,0");
    await expect(prompt(page)).toHaveText("Command:");
    await matches(page, "hatch-dots.png", { x: 0, y: 0 }, { width: 160, height: 100 });
  });

  test("a selected polyline", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await lwt(page).click();
    await enter(page, "LWEIGHT", "0.70");
    await enter(page, "PL", ...JOINS, "");
    await expect(prompt(page)).toHaveText("Command:");
    await clickWorld(page, canvas, -40, -20);
    await matches(page, "joins-selected.png", AROUND, CLIP);
  });
});
