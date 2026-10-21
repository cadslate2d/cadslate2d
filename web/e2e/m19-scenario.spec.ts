// The M19 exit scenario (#320): a DWG of each version — AutoCAD 2000, 2004,
// 2007 — opened, drafted on, saved as each of the four formats, and each
// save reopened with the drafting in it. The DWG is read by its bytes, the
// format kept with the drawing, and what a file cannot hold reported.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  downloadBytesFrom,
  dwgFixturePath,
  enter,
  logLines,
  openApp,
  prompt,
  settle,
} from "./support";

/** What the mixed-content fixture holds, as the import counts it, before and after the drafting. */
const OPENED = "Imported 3 lines, 2 circles, 1 arc, 1 polyline, 1 text, 1 block reference.";
const DRAFTED = "Imported 4 lines, 3 circles, 1 arc, 1 polyline, 1 text, 1 block reference.";
const TABLES = "With 2 layers and 1 block.";

const FORMATS = [
  { keyword: "DXF", extension: "dxf", signature: "  0\nSE", label: "DXF" },
  { keyword: "DWG2000", extension: "dwg", signature: "AC1015", label: "DWG 2000" },
  { keyword: "DWG2004", extension: "dwg", signature: "AC1018", label: "DWG 2004" },
  { keyword: "DWG2007", extension: "dwg", signature: "AC1021", label: "DWG 2007" },
] as const;

test("the M19 scenario: a DWG of each version opened, drafted on, saved as each format, and reopened", async ({
  page,
}) => {
  await openApp(page);
  let opened = 0;
  let drafted = 0;
  for (const version of ["AC1015", "AC1018", "AC1021"] as const) {
    // 1. A drawing AutoCAD's contemporaries wrote: lines, circles, an arc, a
    // polyline, text, a block and its reference, on two layers.
    const name = `ezdxf-r2010-mixed-content.${version}.dwg`;
    await chooseFile(page, () => enter(page, "OPEN"), dwgFixturePath(`valid/${name}`));
    opened += 1;
    await expect(logLines(page).filter({ hasText: OPENED })).toHaveCount(opened);
    await expect(logLines(page).filter({ hasText: TABLES })).toHaveCount(opened + drafted);
    await expect(logLines(page).last()).toHaveText("Drawing unit: metres.");
    await expect(page.getByTestId("document")).toHaveText(name);
    await expect(page.getByTestId("lossy")).toHaveCount(0);

    // 2. Drafting on it: a line and a circle.
    await enter(page, "L", "200,200", "260,240", "", "C", "230,230", "10");
    await expect(page.getByTestId("document")).toHaveText(`${name} •`);

    // 3. Saved as each format — SAVEAS asks, Enter would keep the file's
    // own — and each save reopened: the drafting is there, whatever
    // carried it, and the drawing is of that format from then on.
    for (const format of FORMATS) {
      await enter(page, "SAVEAS");
      await expect(prompt(page)).toContainText("Save as or [DXF/DWG2000/DWG2004/DWG2007]");
      const file = await downloadBytesFrom(page, () => enter(page, format.keyword));
      expect(file.name.endsWith(`.${format.extension}`)).toBe(true);
      expect(file.bytes.subarray(0, 6).toString()).toBe(format.signature);
      await expect(logLines(page).last()).toHaveText(`Drawing exported as ${format.label}.`);
      await expect(page.getByTestId("document")).not.toContainText("•");

      const saved = `drafted-${version}.${format.extension}`;
      await chooseFile(page, () => enter(page, "OPEN"), {
        name: saved,
        mimeType: format.extension === "dxf" ? "application/dxf" : "application/acad",
        buffer: file.bytes,
      });
      drafted += 1;
      await expect(logLines(page).filter({ hasText: DRAFTED })).toHaveCount(drafted);
      await expect(logLines(page).filter({ hasText: TABLES })).toHaveCount(opened + drafted);
      await expect(logLines(page).last()).toHaveText("Drawing unit: metres.");
      await expect(page.getByTestId("document")).toHaveText(saved);
      await expect(page.getByTestId("lossy")).toHaveCount(0);
      // The format came with the file: the next SAVEAS offers it.
      await enter(page, "SAVEAS");
      await expect(prompt(page)).toHaveText(
        `Save as or [DXF/DWG2000/DWG2004/DWG2007] <${format.keyword}>:`,
      );
      await page.keyboard.press("Escape");
      await settle(page);
    }
  }
});
