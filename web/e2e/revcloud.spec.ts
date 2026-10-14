// Revision clouds (#215): a cloud round a rectangle and one in place of a
// circle, drawn as arcs and written as closed bulged polylines.
import { expect, test } from "@playwright/test";
import { clickWorld, downloadFrom, enter, entities, group, logLines, openApp } from "./support";

test("REVCLOUD clouds a rectangle and a circle and writes them as closed polylines", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "REVCLOUD", "-80,-40", "20,40");
  await expect(logLines(page).last()).toHaveText("Revision cloud made: 26 arcs.");
  await enter(page, "C", "60,0", "25");
  await enter(page, "REVCLOUD", "O");
  await clickWorld(page, canvas, 85, 0);
  await expect(logLines(page).last()).toHaveText("Revision cloud made: 12 arcs.");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
  await page.mouse.move(2, 2);

  if (browserName === "chromium") {
    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await expect(page).toHaveScreenshot("revision-cloud.png", {
      clip: {
        x: box.x + box.width / 2 - 100,
        y: box.y + box.height / 2 - 55,
        width: 200,
        height: 110,
      },
      maxDiffPixels: 80,
    });
  }

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const clouds = entities(file.text, "LWPOLYLINE");
  expect(clouds.map((cloud) => [group(cloud, 90), group(cloud, 70)])).toEqual([
    ["26", "1"],
    ["12", "1"],
  ]);
  expect(entities(file.text, "CIRCLE")).toHaveLength(0);
});
