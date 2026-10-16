// Multileaders (#240, ADR-0033): MLEADER draws one from its arrowhead to
// its landing, with a dogleg and a note laid out by the current style —
// on screen, and in the file as a MULTILEADER drawn by a MLEADERSTYLE.
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  downloadFrom,
  dxfSection,
  enter,
  entities,
  group,
  groupsOf,
  openApp,
  prompt,
} from "./support";

/** Compares a clip around the canvas centre with its baseline, in Chromium. */
async function matches(page: Page, canvas: Locator, name: string) {
  const center = await centerOf(canvas);
  await expect(page).toHaveScreenshot(name, {
    clip: { x: Math.round(center.x) - 80, y: Math.round(center.y) - 45, width: 160, height: 90 },
    maxDiffPixels: 60,
  });
}

test("a multileader is drawn from its arrowhead to its note and saved as a MULTILEADER", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "ZOOM", "S", "4x");
  await enter(page, "MLEADER", "-17,-9");
  await expect(prompt(page)).toHaveText("Specify leader landing location:");
  await enter(page, "-5,5");
  await expect(prompt(page)).toHaveText("Enter the note <none>:");
  await enter(page, "HOLE");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  if (browserName === "chromium") await matches(page, canvas, "mleader-note.png");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const [mleader] = entities(file.text, "MULTILEADER");
  expect(mleader).toBeDefined();
  expect(group(mleader, 304)).toBe("HOLE");
  const style = dxfSection(file.text, "OBJECTS").find((record) => record.type === "MLEADERSTYLE");
  expect(group(style, 44)).toBe("2.5");
  // Its style by handle, after the context data's text style.
  expect(groupsOf(mleader, 340).at(-1)).toBe(group(style, 5));
});
