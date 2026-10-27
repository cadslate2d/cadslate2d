// SKETCH (#326): a freehand stroke drawn with the mouse — a click puts the
// pen down, the path is recorded as the pointer moves, a click lifts it —
// kept as a polyline on Enter, in one undo.
import { expect, test } from "@playwright/test";
import { centerOf, downloadFrom, enter, entities, group, openApp, prompt } from "./support";

test("a freehand stroke is recorded as the mouse moves and kept as a polyline", async ({
  page,
}) => {
  await openApp(page);
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  const center = await centerOf(canvas);
  await enter(page, "SKETCH");
  await expect(prompt(page)).toHaveText(
    "Click to put the pen down, or press Enter to keep the sketch or [Increment]:",
  );
  await page.mouse.move(center.x - 120, center.y);
  await page.mouse.click(center.x - 120, center.y);
  await expect(prompt(page)).toHaveText(/^Sketching: click to lift the pen/);
  // A wave, in many small moves.
  for (let i = 1; i <= 40; i += 1) {
    await page.mouse.move(center.x - 120 + i * 6, center.y - Math.sin(i / 4) * 40);
  }
  await page.mouse.click(center.x + 120, center.y);
  await page.keyboard.press("Enter");
  await expect(prompt(page)).toHaveText("Command:");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const strokes = entities(file.text, "LWPOLYLINE");
  expect(strokes).toHaveLength(1);
  // Its vertex count: many, one per increment of the path.
  expect(Number(group(strokes[0], 90))).toBeGreaterThan(20);
  await enter(page, "U");
  const after = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(after.text, "LWPOLYLINE")).toHaveLength(0);
});
