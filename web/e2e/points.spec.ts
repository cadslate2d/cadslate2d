// Points (#198): placed, styled, put along a curve, snapped to, and saved.
import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
} from "./support";

test("points are placed, styled, divided along a line, and written as POINT records", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "POINT", "20,60", "");
  await enter(page, "PTYPE");
  await expect(prompt(page)).toHaveText(
    "Enter point mode, as a number or [Dot/Cross/X/Tick/Blank/CIrcle/SQuare/Plain] <0>:",
  );
  await enter(page, "X", "CI", "", "8");
  await expect(logLines(page).last()).toHaveText("Point style: X in a circle (35), size 8.");
  await enter(page, "L", "-80,-40", "80,-40", "", "DIVIDE");
  await expect(prompt(page)).toHaveText("Select object to divide:");
  await clickWorld(page, canvas, 0, -40);
  await expect(prompt(page)).toHaveText("Enter the number of segments:");
  await enter(page, "4");
  await expect(logLines(page).last()).toHaveText("Divided into 4 segments: 3 points placed.");

  // The Node snap finds a point: a line from one.
  await enter(page, "L");
  await page.mouse.move(0, 0);
  await clickWorld(page, canvas, 21, 59);
  await expect(page.getByTestId("snap")).toHaveText("Node");
  await enter(page, "60,60", "");

  if (browserName === "chromium") {
    // The markers as drawn: an X in a circle at each point.
    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await expect(page).toHaveScreenshot("point-markers.png", {
      clip: {
        x: box.x + box.width / 2 - 100,
        y: box.y + box.height / 2 - 80,
        width: 200,
        height: 140,
      },
      maxDiffPixels: 60,
    });
  }

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const points = entities(file.text, "POINT");
  expect(points.map((point) => [group(point, 10), group(point, 20)])).toEqual([
    ["20.0", "60.0"],
    ["-40.0", "-40.0"],
    ["0.0", "-40.0"],
    ["40.0", "-40.0"],
  ]);
  expect(file.text).toContain("\n  9\n$PDMODE\n 70\n35\n  9\n$PDSIZE\n 40\n8.0\n");
  const lines = entities(file.text, "LINE");
  expect(group(lines[1], 10)).toBe("20.0");
  expect(group(lines[1], 20)).toBe("60.0");
});
