// Construction lines (#206; ADR-0031): drawn endless, trimmed into rays,
// left out of the extents, and written as XLINE and RAY.
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

test("XLINE and RAY draw construction lines that TRIM turns into rays and ZOOM Extents ignores", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "L", "-40,-40", "-40,40", "", "L", "40,-40", "40,40", "");
  await enter(page, "XLINE");
  await expect(prompt(page)).toHaveText("Specify a point or [Hor/Ver/Ang/Bisect/Offset]:");
  await enter(page, "H", "0,0", "");
  await enter(page, "RAY", "0,-60", "0,-30", "");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  // TRIM: both lines as edges, the xline picked between them.
  await enter(page, "TRIM");
  await clickWorld(page, canvas, -40, 20);
  await clickWorld(page, canvas, 40, 20);
  await enter(page, "");
  await clickWorld(page, canvas, 10, 0);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Command:");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "XLINE")).toHaveLength(0);
  const rays = entities(file.text, "RAY");
  expect(rays).toHaveLength(3);
  const starts = rays.map((ray) => [group(ray, 10), group(ray, 20)]);
  expect(starts).toContainEqual(["-40.0", "0.0"]);
  expect(starts).toContainEqual(["40.0", "0.0"]);
  // The extents are the two lines'.
  expect(file.text).toContain("\n  9\n$EXTMAX\n 10\n40.0\n 20\n40.0\n");
});
