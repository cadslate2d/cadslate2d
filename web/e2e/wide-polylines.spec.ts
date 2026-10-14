// Wide polylines (#204): a polyline given a width with PEDIT and a donut,
// drawn as filled bands, written with their width.
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

test("a wide polyline and a donut are drawn as bands and saved with their width", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "PL", "-80,-40", "0,-40", "A", "40,0", "L", "40,40", "");
  await enter(page, "PEDIT");
  await expect(prompt(page)).toHaveText("Select polyline:");
  await clickWorld(page, canvas, -40, -40);
  await enter(page, "W", "8", "");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "DONUT", "20", "40", "-40,30", "");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
  await page.mouse.move(2, 2);

  if (browserName === "chromium") {
    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await expect(page).toHaveScreenshot("wide-polyline-and-donut.png", {
      clip: {
        x: box.x + box.width / 2 - 110,
        y: box.y + box.height / 2 - 70,
        width: 190,
        height: 130,
      },
      maxDiffPixels: 80,
    });
  }

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const polylines = entities(file.text, "LWPOLYLINE");
  expect(polylines.map((polyline) => group(polyline, 43))).toEqual(["8.0", "10.0"]);
  expect(group(polylines[1], 70)).toBe("1");
});
