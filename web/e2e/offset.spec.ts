import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  litPixels,
  logLines,
  openApp,
  prompt,
  rendererStats,
} from "./support";

test.describe("OFFSET", () => {
  test("offsets a line to the picked side and a circle outwards, repeating until Enter", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "", "C", "0,100", "40", "O");
    await expect(prompt(page)).toHaveText("Specify offset distance:");
    await enter(page, "10");
    await expect(prompt(page)).toHaveText("Select object to offset:");

    await clickWorld(page, canvas, 50, 0);
    await expect(prompt(page)).toHaveText("Specify point on side to offset:");
    await clickWorld(page, canvas, 50, 30);
    await clickWorld(page, canvas, 40, 100);
    await clickWorld(page, canvas, 120, 100);
    await enter(page, "");
    await expect(prompt(page)).toHaveText("Command:");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n10.0\n 11\n100.0\n 21\n10.0\n");
    expect(file.text).toContain("CIRCLE\n  8\n0\n 10\n0.0\n 20\n100.0\n 40\n50.0\n");
  });

  test("previews the result on the side of the pointer without committed uploads", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "", "O", "20");
    await clickWorld(page, canvas, 50, 0);
    const before = await rendererStats(page);
    const withoutPreview = await litPixels(canvas);

    await hoverWorld(page, canvas, 50, 60);
    expect(await litPixels(canvas)).toBeGreaterThan(withoutPreview + 60);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("declines arcs by name and explains an empty pick", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "A", "50,0", "0,50", "-50,0", "O", "5");
    await clickWorld(page, canvas, 0, 50);
    await expect(logLines(page).last()).toHaveText("OFFSET does not support arcs yet.");
    await clickWorld(page, canvas, 200, 200);
    await expect(logLines(page).last()).toHaveText("No object there.");
    await expect(prompt(page)).toHaveText("Select object to offset:");
  });
});
