// ZOOM and PAN (#180): the view moved by name — a far plot reached by
// Extents and left by Previous — committing nothing and uploading no
// committed geometry.
import { expect, test, type Page } from "@playwright/test";
import { enter, litPixels, logLines, openApp, prompt, rendererStats, settle } from "./support";

const coordinates = (page: Page) => page.getByTestId("coordinates");

test.describe("ZOOM and PAN", () => {
  test("Extents reaches a far plot, Previous returns, and nothing committed moves", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "L", "10000000,10000000", "10000100,10000060", "");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas), "the line is off screen").toBe(0);
    const before = await rendererStats(page);

    await enter(page, "ZOOM", "E");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas), "the line fills the view").toBeGreaterThan(300);
    // The readout at the middle of the canvas is the plot's middle.
    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await expect(coordinates(page)).toHaveText("10000050.0000, 10000030.0000");

    await enter(page, "Z", "P");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas), "back at the origin").toBe(0);

    // Through the toolbar: the same command, logged like typed input.
    await page.getByRole("button", { name: "Extents" }).click();
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas)).toBeGreaterThan(300);
    await expect(logLines(page).filter({ hasText: "Command: ZOOM" })).toHaveCount(2);

    const after = await rendererStats(page);
    expect(after.sceneUploads, "the view moved; the scene did not").toBe(before.sceneUploads);
    // Nothing to undo but the line itself.
    await enter(page, "U");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas), "the line is gone and the view stayed").toBe(0);
    await enter(page, "REDO");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas)).toBeGreaterThan(300);
  });

  test("a middle double-click zooms extents; PAN moves by two points", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "C", "500,500", "40");
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas)).toBe(0);

    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await page.mouse.dblclick(box.x + 200, box.y + 200, { button: "middle" });
    await expect(logLines(page).filter({ hasText: "Command: ZOOM" })).toHaveCount(1);
    await page.mouse.move(5, 5);
    await settle(page);
    expect(await litPixels(canvas), "the circle fills the view").toBeGreaterThan(200);

    // PAN by a displacement the circle's radius wide: it moves, the scene does not.
    const before = await rendererStats(page);
    await enter(page, "PAN", "0,0", "40,0");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await expect(coordinates(page)).toHaveText("460.0000, 500.0000");
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });
});
