// The M17 exit scenario (#243): a plate annotated with multileaders — one
// note with two leaders, made with one and given the second by
// MLEADEREDIT, and a balloon that is a block — the style changed, saved,
// and reopened.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  dxfSection,
  enter,
  entities,
  group,
  groupsOf,
  logLines,
  openApp,
  prompt,
} from "./support";

test("the M17 scenario: a note with two leaders, a balloon, a style changed, saved and reopened", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await page.keyboard.press("F3");

  // 1. A plate with two holes.
  await enter(page, "RECTANG", "0,0", "120,60");
  await enter(page, "C", "30,30", "6", "C", "90,30", "6");

  // 2. A balloon block: a circle with its number, made from what is drawn
  // and the reference BLOCK leaves erased.
  await enter(page, "C", "250,0", "6", "TEXT", "248,-2", "4", "0", "1");
  await clickWorld(page, canvas, 240, 10);
  await clickWorld(page, canvas, 260, -10);
  await enter(page, "BLOCK", "BALLOON", "250,0");
  await expect(prompt(page)).toHaveText("Command:");
  await clickWorld(page, canvas, 256, 0);
  await page.keyboard.press("Delete");

  // 3. A note to the first hole, then a second leader to the other.
  await enter(page, "MLEADER", "34,34", "60,90", "2X DIA 6");
  await enter(page, "MLEADEREDIT");
  await clickWorld(page, canvas, 47, 62);
  await expect(prompt(page)).toHaveText("Specify leader arrowhead location or [Remove leader]:");
  await enter(page, "86,34", "");
  await expect(logLines(page).filter({ hasText: "A leader is added." })).toHaveCount(1);

  // 4. The balloon on a corner.
  await enter(page, "MLEADER", "C", "B", "BALLOON", "120,60", "150,90");
  await expect(prompt(page)).toHaveText("Command:");

  // 5. The style changed: both redrawn by one row.
  await enter(page, "MLEADERSTYLE", "A", "4", "DO", "8", "");
  await expect(logLines(page).filter({ hasText: '"Standard": dogleg length 8.' })).toHaveCount(1);

  // 6. Saved: two multileaders — the note with two leader lines, the
  // balloon by its block — drawn by a style with the new values.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const [note, balloon] = entities(file.text, "MULTILEADER");
  expect(groupsOf(note, 304)).toEqual(["2X DIA 6", "LEADER_LINE{", "LEADER_LINE{"]);
  expect(group(balloon, 172)).toBe("1");
  const style = dxfSection(file.text, "OBJECTS").find((record) => record.type === "MLEADERSTYLE");
  expect(group(style, 44)).toBe("4.0");
  expect(group(style, 43)).toBe("8.0");

  // 7. Reopened: both come back, nothing reported lost.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "plate.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "Imported " }).last();
  await expect(imported).toContainText("2 multileaders");
  await expect(logLines(page).filter({ hasText: "Not imported" })).toHaveCount(0);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});
