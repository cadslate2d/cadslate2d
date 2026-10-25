// Constraints on the canvas (#269, ADR-0038): a bar of glyphs beside each
// constrained object and each placed dimensional constraint as a dimension
// labelled with its name and value — overlay content, in its own colour —
// and hidden from the View menu.
import { expect, test } from "@playwright/test";
import { centerOf, chooseMenu, clickWorld, enter, logLines, openApp, settle } from "./support";

test("constraint bars and a constraint dimension", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
  const canvas = await openApp(page);
  await enter(page, "L", "-30,-20", "30,-20", "30,20", "");
  await enter(page, "GCHORIZONTAL");
  await clickWorld(page, canvas, 0, -20);
  await enter(page, "GCVERTICAL");
  await clickWorld(page, canvas, 30, 0);
  await enter(page, "GCCOINCIDENT", "30,-20", "30,-20");
  await enter(page, "DCHORIZONTAL", "-30,-20", "30,-20", "0,-32", "");
  await expect(logLines(page).last()).toHaveText("d1 = 60");
  // Close enough to read the dimension; the bars keep their size.
  await enter(page, "ZOOM", "W", "-45,-45", "55,30");
  // Away from the bars, so none is hot.
  await page.mouse.move(5, 5);
  await settle(page);

  const center = await centerOf(canvas);
  const clip = { x: center.x - 220, y: center.y - 150, width: 440, height: 300 };
  await expect(page).toHaveScreenshot("constraint-bars.png", { clip, maxDiffPixels: 60 });

  // Hidden from the View menu: the drawing alone.
  await chooseMenu(page, "View", "Constraints", "Hide Constraint Bars");
  await chooseMenu(page, "View", "Constraints", "Hide Dimensional Constraints");
  await page.mouse.move(5, 5);
  await settle(page);
  await expect(page).toHaveScreenshot("constraint-bars-hidden.png", { clip, maxDiffPixels: 60 });
});
