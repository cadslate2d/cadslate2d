// FLOORPLAN (#281, ADR-0039): a brief typed as JSON, or read from a spec
// file, planned, previewed, and drawn as ordinary walls, doors, and labels
// on their layers; the plans cycled; one undo; and Draw › Floor Plan....
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  chooseMenu,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
} from "./support";

const BRIEF = {
  terrain: { width: 15, depth: 20 },
  setbacks: { front: 3 },
  rooms: [
    { name: "living room", area: 24, entrance: true },
    { name: "kitchen", area: 10 },
    { name: "bedroom", area: 12 },
    { name: "bath", area: 4.5 },
  ],
  adjacent: [["living room", "kitchen"]],
};

/** The drawing's entities on `layer`, as DXFOUT writes them. */
async function onLayer(page: Parameters<typeof enter>[0], layer: string) {
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  return entities(file.text).filter((entity) => group(entity, 8) === layer);
}

test("a brief as JSON is planned, the plans cycled, and one drawn as one undo", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "FLOORPLAN", "B", JSON.stringify(BRIEF), "");
  await expect(prompt(page)).toHaveText("Plan 1 of 3 or [Next/Previous/Accept] <Accept>:");
  await enter(page, "N");
  await expect(prompt(page)).toHaveText("Plan 2 of 3 or [Next/Previous/Accept] <Accept>:");
  await enter(page, "");
  await expect(logLines(page).last()).toContainText("Plan 2 drawn: 4 rooms");

  const walls = await onLayer(page, "WALL-EXT");
  expect(walls.filter((e) => e.type === "LWPOLYLINE")).toHaveLength(1);
  expect((await onLayer(page, "WALL-INT")).length).toBeGreaterThan(4);
  expect(
    (await onLayer(page, "DOOR")).filter((e) => e.type === "ARC").length,
  ).toBeGreaterThanOrEqual(4);
  const labels = (await onLayer(page, "ROOM-TEXT")).map((e) => group(e, 1));
  for (const name of ["living room", "kitchen", "bedroom", "bath"]) expect(labels).toContain(name);

  await enter(page, "U");
  expect(await onLayer(page, "WALL-EXT")).toHaveLength(0);
});

test("a spec file is read as the brief, and the menu starts the command", async ({ page }) => {
  await openApp(page);
  await chooseMenu(page, "Draw", "Floor Plan...");
  await expect(prompt(page)).toHaveText("Specify terrain width or [Brief/File] <15>:");
  await chooseFile(page, () => enter(page, "F"), {
    name: "house.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(BRIEF, null, 2)),
  });
  await expect(prompt(page)).toHaveText("Specify the terrain's lower left corner <0,0>:");
  await enter(page, "", "");
  await expect(logLines(page).last()).toContainText("Plan 1 drawn: 4 rooms");
  expect((await onLayer(page, "ROOM-TEXT")).length).toBe(8);
});

test("a brief that cannot be built is refused with the reason", async ({ page }) => {
  await openApp(page);
  await enter(page, "FLOORPLAN", "8", "8", "living", "40", "bedroom", "30", "", "", "", "");
  await expect(logLines(page).last()).toContainText(
    "This cannot be built: The rooms need at least 66.5 m²",
  );
  await expect(prompt(page)).toHaveText("Command:");
});

// #324: windows at the width asked, in every room's outside walls.
test("a plan with windows: the width typed at its prompt", async ({ page }) => {
  await openApp(page);
  await enter(
    page,
    "FLOORPLAN",
    "12",
    "20",
    "living",
    "25",
    "kitchen",
    "10",
    "bedroom",
    "12",
    "",
    "",
  );
  await expect(prompt(page)).toHaveText(
    "Specify the windows' width, or press Enter for no windows:",
  );
  await enter(page, "1.5", "", "");
  await expect(logLines(page).last()).toContainText("Plan 1 drawn: 3 rooms");
  const glass = await onLayer(page, "WINDOW");
  expect(glass.length).toBeGreaterThanOrEqual(6);
  expect(glass.length % 2).toBe(0);
  await enter(page, "U");
  expect(await onLayer(page, "WINDOW")).toHaveLength(0);
});

// #325: a window cut into a drawn wall afterwards — here the plan's — by
// the WINDOW command, one undo.
test("WINDOW opens a wall of a drawn plan", async ({ page }) => {
  await openApp(page);
  await enter(page, "FLOORPLAN", "B", JSON.stringify(BRIEF), "", "");
  await expect(logLines(page).last()).toContainText("Plan 1 drawn");
  const before = (await onLayer(page, "WALL-EXT")).length;
  // The plan's front wall is along y = 3 (the front setback), x from 0 to
  // 15: its outer face is the footprint's bottom edge, in view as drawn.
  await enter(page, "WINDOW");
  await expect(prompt(page)).toHaveText("Select a face of the wall:");
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  await clickWorld(page, canvas, 7.5, BRIEF.setbacks.front);
  await expect(prompt(page)).toHaveText("Specify the window's centre on the wall:");
  await enter(page, `7.5,${BRIEF.setbacks.front + 0.125}`, "1.5");
  await expect(prompt(page)).toHaveText("Command:");
  expect((await onLayer(page, "WINDOW")).length).toBe(2);
  expect((await onLayer(page, "WALL-EXT")).length).toBe(before + 4);
  await enter(page, "U");
  expect((await onLayer(page, "WINDOW")).length).toBe(0);
  expect((await onLayer(page, "WALL-EXT")).length).toBe(before);
});
