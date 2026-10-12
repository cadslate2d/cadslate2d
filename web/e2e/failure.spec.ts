// A panic in the engine (#176): the user is told, the drawing as last kept
// is untouched, and a reload offers it back (ADR-0019).
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, lines, logLines, openApp, prompt } from "./support";

test("a panic is told, and a reload recovers the drawing as last kept", async ({ page }) => {
  const canvas = await openApp(page);
  await enter(page, "L", "0,0", "100,50", "");
  await expect(prompt(page)).toHaveText("Command:");
  // The autosave follows the last change by a pause.
  await page.waitForTimeout(1500);
  // A change the pause has not kept yet.
  await enter(page, "C", "10,10", "5");
  await expect(prompt(page)).toHaveText("Command:");

  await page.evaluate(() => window.__cadslate?.crash());
  const dialog = page.getByTestId("failure");
  await expect(dialog).toContainText("The drawing engine stopped and cannot continue.");
  await expect(dialog).toContainText("asked to fail, for the test");
  await expect(dialog).toContainText("is safe: reloading offers it back");

  // Nothing reaches the engine: typing, clicking, and the autosave's pause do nothing.
  const logged = await logLines(page).count();
  await enter(page, "L", "0,0", "1,1", "");
  // The dialog covers the middle of the canvas; a corner of it is still the canvas.
  const box = await canvas.boundingBox();
  if (!box) throw new Error("the canvas has no box");
  await page.mouse.move(box.x + 20, box.y + 20);
  await page.mouse.click(box.x + 20, box.y + 20);
  await page.waitForTimeout(1500);
  expect(await logLines(page).count()).toBe(logged);

  // The reload: no "leave this page?" — there is nothing more to lose.
  await dialog.getByRole("button", { name: "Reload" }).click();
  await openApp(page);
  const offer = page.getByTestId("recovery");
  await expect(offer).toContainText("An unnamed drawing, kept at");
  await offer.getByRole("button", { name: "Recover" }).click();
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(lines(file.text), "the drawing as kept: the line, not the circle after it").toEqual([
    [0, 0, 100, 50],
  ]);
  expect(file.text).not.toContain("CIRCLE");
});
