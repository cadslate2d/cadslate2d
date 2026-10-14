// The layer tools (#213): a layer isolated by picking what is on it and
// the others brought back; a layer frozen from the Layers panel, written
// frozen, not off.
import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

const panel = (page: Page) => page.getByRole("complementary", { name: "Layers" });

test("LAYISO isolates a layer by picking, LAYUNISO brings the others back, and a frozen layer stays frozen", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "LA", "N", "walls,doors", "");
  await enter(page, "L", "-100,0", "100,0", "");
  await enter(page, "LA", "S", "walls", "", "L", "-100,40", "100,40", "");
  await enter(page, "LA", "S", "doors", "", "L", "-100,80", "100,80", "");
  await enter(page, "LA", "S", "0", "");

  // Isolated by picking the wall: the others off, walls current.
  await enter(page, "LAYISO");
  await clickWorld(page, canvas, 0, 40);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText('Layer "walls" is isolated.');
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("1 selected.");
  await page.keyboard.press("Escape");
  await expect(panel(page).getByRole("button", { name: "Switch on doors" })).toBeVisible();

  await enter(page, "LAYUNISO");
  await expect(logLines(page).last()).toHaveText("2 layers restored.");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("3 selected.");
  await page.keyboard.press("Escape");

  // Frozen from the panel: not shown, and written frozen.
  await panel(page).getByRole("button", { name: "Freeze doors" }).click();
  await expect(logLines(page).filter({ hasText: "1 layer frozen." })).toHaveCount(1);
  await expect(panel(page).getByRole("button", { name: "Thaw doors" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("2 selected.");
  await page.keyboard.press("Escape");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const doors = tableRow(file.text, "LAYER", "doors");
  expect([group(doors, 70), group(doors, 62)]).toEqual(["1", "7"]);
});
