// The M8 exit scenario (DESIGN §36; #189): a file from another program —
// an ellipse, a spline, a block with an attribute, two layouts with a
// viewport — opened with everything it holds, edited with the selection
// and view commands of M8, plotted to a PDF, and saved with its layouts
// intact and nothing lost.
import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  entities,
  fixturePath,
  group,
  logLines,
  mapping,
  openApp,
  prompt,
} from "./support";

/** The layouts fixture with an ellipse, a spline, and a block with an attribute added. */
function scenarioDxf(): string {
  const base = readFileSync(fixturePath("valid/layouts.dxf"), "utf8");
  const record = (lines: string[]) => lines.join("\n") + "\n";
  const block = record([
    "  0",
    "BLOCK",
    "  8",
    "0",
    "  2",
    "TAG",
    " 70",
    "2",
    " 10",
    "0.0",
    " 20",
    "0.0",
    " 30",
    "0.0",
    "  3",
    "TAG",
    "  0",
    "LWPOLYLINE",
    "  8",
    "0",
    " 90",
    "4",
    " 70",
    "1",
    " 10",
    "0.0",
    " 20",
    "0.0",
    " 10",
    "30.0",
    " 20",
    "0.0",
    " 10",
    "30.0",
    " 20",
    "10.0",
    " 10",
    "0.0",
    " 20",
    "10.0",
    "  0",
    "ATTDEF",
    "  8",
    "0",
    " 10",
    "2.0",
    " 20",
    "2.0",
    " 30",
    "0.0",
    " 40",
    "5.0",
    "  1",
    "",
    "  3",
    "Room",
    "  2",
    "ROOM",
    " 70",
    "0",
    "  0",
    "ENDBLK",
  ]);
  const drawn = record([
    "  0",
    "ELLIPSE",
    "  8",
    "0",
    " 10",
    "200.0",
    " 20",
    "50.0",
    " 30",
    "0.0",
    " 11",
    "20.0",
    " 21",
    "0.0",
    " 31",
    "0.0",
    " 40",
    "0.5",
    " 41",
    "0.0",
    " 42",
    "6.283185307179586",
    "  0",
    "SPLINE",
    "  8",
    "0",
    " 70",
    "8",
    " 71",
    "3",
    " 72",
    "0",
    " 73",
    "0",
    " 74",
    "4",
    " 11",
    "0.0",
    " 21",
    "100.0",
    " 31",
    "0.0",
    " 11",
    "30.0",
    " 21",
    "160.0",
    " 31",
    "0.0",
    " 11",
    "70.0",
    " 21",
    "160.0",
    " 31",
    "0.0",
    " 11",
    "100.0",
    " 21",
    "100.0",
    " 31",
    "0.0",
    "  0",
    "INSERT",
    "  8",
    "0",
    " 66",
    "1",
    "  2",
    "TAG",
    " 10",
    "0.0",
    " 20",
    "-60.0",
    " 30",
    "0.0",
    "  0",
    "ATTRIB",
    "  8",
    "0",
    " 10",
    "2.0",
    " 20",
    "-58.0",
    " 30",
    "0.0",
    " 40",
    "5.0",
    "  1",
    "101",
    "  2",
    "ROOM",
    " 70",
    "0",
    "  0",
    "SEQEND",
  ]);
  const endsec = "  0\nENDSEC\n";
  const blocks = base.indexOf("  2\nBLOCKS\n");
  const blocksEnd = base.indexOf(endsec, blocks);
  let text = base.slice(0, blocksEnd) + block + base.slice(blocksEnd);
  const ents = text.indexOf("  2\nENTITIES\n");
  const entsEnd = text.indexOf(endsec, ents);
  text = text.slice(0, entsEnd) + drawn + text.slice(entsEnd);
  return text;
}

const editor = (page: Page) => page.getByRole("dialog", { name: "Text editor" });

test("the M8 scenario: a file from another program opened whole, edited, plotted, and saved with its layouts", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // 1. Opened with everything it holds: the curves and the block the
  // reader once reported, the layouts it once lost — and not LOSSY.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "scenario.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(scenarioDxf()),
  });
  await expect(
    logLines(page).filter({ hasText: "Imported 3 lines, 1 ellipse, 1 spline, 1 block reference." }),
  ).toHaveCount(1);
  await expect(
    logLines(page).filter({ hasText: "2 layouts, not drawn yet: Layout1, Layout2." }),
  ).toHaveCount(1);
  await expect(logLines(page).filter({ hasText: "Not imported" })).toHaveCount(0);
  await expect(page.getByTestId("lossy")).toHaveCount(0);
  await expect(page.getByTestId("layouts")).toHaveText("2 layouts");
  await page.getByRole("button", { name: "Dismiss the import report" }).click();

  // 2. The attribute, read from the file, edited in place.
  // The view was fitted to the file: clicks go through its mapping.
  const screen = await mapping(page, canvas);
  await enter(page, "ATTEDIT");
  const edge = screen(15, -60);
  await page.mouse.click(edge.x, edge.y);
  await expect(prompt(page)).toHaveText("Room <101>:");
  await enter(page, "202");
  await expect(prompt(page)).toHaveText("Command:");

  // 3. An alias of the drafter's own, then a quick select by kind, and
  // the ellipse moved by a typed displacement.
  await enter(page, "ALIASEDIT");
  await expect(editor(page)).toBeVisible();
  await page.getByRole("textbox", { name: "Paragraph" }).fill("QQ, QSELECT");
  await page.keyboard.press("Control+Enter");
  await expect(logLines(page).last()).toHaveText("1 alias defined.");
  await enter(page, "QQ", "ellipse", "");
  await expect(logLines(page).last()).toHaveText("1 found.");
  await expect(page.getByTestId("selection-headline")).toHaveText("1 ellipse");
  await expect(page.getByTestId("selection-method")).toHaveText("by quick select");
  await enter(page, "M", "0,0", "10,0");
  await expect(prompt(page)).toHaveText("Command:");

  // 4. The view by name, and a fence through the spline and the line.
  await enter(page, "ZOOM", "E");
  await enter(page, "SELECT", "F", "50,-10", "50,170", "");
  await expect(logLines(page).last()).toHaveText("2 found, 2 total");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("2 selected.");
  await page.keyboard.press("Escape");

  // 5. On paper: a PDF of model space, A3 to fit.
  const plot = await downloadFrom(page, () => enter(page, "PLOT", "A3", "E", "Fit", "Y", "C"));
  expect(plot.name).toBe("scenario.pdf");
  expect(plot.text.startsWith("%PDF-1.4\n")).toBe(true);
  expect(plot.text).toContain("/MediaBox [0 0 1190.551 841.89]");
  expect(plot.text.endsWith("%%EOF\n")).toBe(true);

  // 6. Saved with what it held: the curves, the edited attribute, and
  // the layouts with their viewport, byte for byte where they were ours
  // to copy.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const ellipse = entities(file.text, "ELLIPSE")[0];
  expect(group(ellipse, 10)).toBe("210.0");
  expect(entities(file.text, "SPLINE")).toHaveLength(1);
  const attrib = entities(file.text, "ATTRIB")[0];
  expect([group(attrib, 2), group(attrib, 1)]).toEqual(["ROOM", "202"]);
  expect(file.text).toContain("\nVIEWPORT\n");
  expect(file.text).toContain("\n  2\n*Paper_Space0\n");
  expect(file.text).toContain("\n  1\nLayout2\n");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
  await expect(page.getByTestId("lossy")).toHaveCount(0);
});
