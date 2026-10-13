// The M5 exit scenario (DESIGN §33): a drawing saved whole and kept safe.
//
// A drawing with every M4 table and entity is saved as R2000 and reopened
// byte for byte; a hostile file is refused with the reason; a file with
// unsupported content is reported on screen and its export warned about;
// the autosave brings the drawing back after a reload; and, where the
// browser allows, a save is written back to its file.
import { expect, test } from "@playwright/test";
import {
  blocks,
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  fakeFiles,
  fixturePath,
  group,
  lines,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("the M5 file scenario", async ({ page }) => {
  test.slow();
  const canvas = await openApp(page);

  // ── every M4 table and entity ──
  await enter(page, "LINETYPE", "L", "Dashed", "");
  await enter(
    page,
    "LAYER",
    "N",
    "walls",
    "C",
    "4",
    "walls",
    "LT",
    "Dashed",
    "walls",
    "LW",
    "0.5",
    "walls",
    "S",
    "walls",
    "",
  );
  await enter(page, "L", "0,0", "100,0", "", "C", "50,30", "10", "A", "0,50", "20,70", "40,50");
  await enter(page, "PL", "0,-20", "40,-20", "A", "40,0", "L", "0,0", "");
  await enter(page, "LAYER", "S", "0", "");
  await enter(page, "TEXT", "0,-50", "8", "0", "Plan");
  await enter(page, "MTEXT", "H", "6", "60,-40", "140,-80");
  const field = page.getByLabel("Paragraph");
  await field.pressSequentially("Two chairs");
  await field.press("Enter");
  await field.pressSequentially("by the window");
  await field.press("Control+Enter");
  await expect(page.getByRole("dialog", { name: "Text editor" })).toHaveCount(0);
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "L", "200,0", "240,0", "", "L", "200,0", "200,40", "");
  await enter(page, "B", "Chair", "200,0");
  await expect(prompt(page)).toHaveText(
    "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:",
  );
  // A window round both lines, from empty space to empty space.
  await clickWorld(page, canvas, 190, -10);
  await clickWorld(page, canvas, 250, 50);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText('Block "Chair" defined from 2 objects.');
  await enter(page, "I", "Chair", "300,0", "2", "90");
  await expect(prompt(page)).toHaveText("Command:");

  // ── saved as R2000, whole ──
  const first = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(first.text).toContain("$ACADVER\n  1\nAC1015\n");
  const walls = tableRow(first.text, "LAYER", "walls");
  expect([group(walls, 62), group(walls, 6), group(walls, 370)]).toEqual(["4", "Dashed", "50"]);
  expect(entities(first.text, "MTEXT")).toHaveLength(1);
  expect(group(entities(first.text, "MTEXT")[0], 1)).toBe("Two chairs\\Pby the window");
  expect(entities(first.text, "LWPOLYLINE")).toHaveLength(1);
  expect(blocks(first.text)).toEqual(["Chair"]);
  expect(entities(first.text, "INSERT")).toHaveLength(2);
  expect(lines(first.text)).toEqual([[0, 0, 100, 0]]);

  // ── reopened, and saved again: the same bytes ──
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: first.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(first.text),
  });
  await expect(
    logLines(page).filter({
      hasText: "Imported 1 line, 1 circle, 1 arc, 1 polyline, 2 texts, 2 block references.",
    }),
  ).toHaveCount(1);
  await expect(logLines(page).last()).toHaveText("With 1 layer, 1 linetype, and 1 block.");
  await expect(page.getByTestId("lossy")).toHaveCount(0);
  const again = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(again.text).toBe(first.text);

  // ── a hostile file is refused with the reason, and the drawing stays ──
  await chooseFile(page, () => enter(page, "OPEN"), fixturePath("malformed/long-value.dxf"));
  await expect(logLines(page).last()).toContainText(
    "Cannot open long-value.dxf: line 8: a value is longer than 65536 bytes.",
  );
  await chooseFile(page, () => enter(page, "OPEN"), fixturePath("malformed/cyclic-blocks.dxf"));
  await expect(logLines(page).last()).toContainText("would contain itself");
  expect((await downloadFrom(page, () => enter(page, "DXFOUT"))).text).toBe(first.text);

  // ── unsupported content: reported on screen, and the export asks first ──
  await chooseFile(page, () => enter(page, "OPEN"), fixturePath("valid/ezdxf-r12-blocks.dxf"));
  const report = page.getByTestId("import-report");
  await expect(report).toContainText("Not imported (unsupported): 1 (INSERT array)");
  await expect(page.getByTestId("lossy")).toHaveText("LOSSY");
  await enter(page, "DXFOUT");
  const warning = page.getByTestId("export-warning");
  await expect(warning).toContainText("(INSERT with a non-uniform scale)");
  await warning.getByRole("button", { name: "Cancel" }).click();
  await expect(warning).toHaveCount(0);

  // ── the autosave brings an unexported drawing back after a reload ──
  await enter(page, "C", "500,500", "25");
  await page.waitForTimeout(1500);
  page.once("dialog", (dialog) => void dialog.accept());
  await page.reload();
  await openApp(page);
  const offer = page.getByTestId("recovery");
  await expect(offer).toContainText("ezdxf-r12-blocks.dxf, kept at");
  await offer.getByRole("button", { name: "Recover" }).click();
  // The recovered drawing is our own file, whole: nothing left to warn about.
  await expect(page.getByTestId("lossy")).toHaveCount(0);
  const recovered = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(recovered.text, "CIRCLE")).toHaveLength(1);
  expect(entities(recovered.text, "INSERT")).toHaveLength(6);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

test("where the browser allows, SAVE writes back to the file it saved to", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "the File System Access API is Chromium's");
  await openApp(page, { fileSystemAccess: "fake" });
  await enter(page, "L", "0,0", "100,50", "", "SAVE");
  await expect(page.getByTestId("document")).toHaveText("drawing.dxf");
  await enter(page, "C", "50,50", "10", "SAVE");
  await expect(page.getByTestId("document")).toHaveText("drawing.dxf");
  const store = await fakeFiles(page);
  expect(store.pickers.save).toBe(1);
  expect(entities(store.files["drawing.dxf"] ?? "", "CIRCLE")).toHaveLength(1);
});
