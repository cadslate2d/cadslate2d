// The M11 exit scenario (#209): a file whose layout plots at one to two,
// turned a quarter turn; a wide polyline edited with PEDIT, a leader to a
// note, construction lines trimmed into rays, a quick select on two
// properties, the layout plotted at the file's settings, and the file
// saved and reopened with it all.
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

const tab = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Layouts" }).getByRole("button", { name, exact: true });

/**
 * The layouts fixture with Layout1's plot settings as another program
 * would leave them: one paper unit to two drawing units (142 over 143),
 * turned a quarter turn (73).
 */
function sheetAtHalf(): string {
  const base = readFileSync(fixturePath("valid/layouts.dxf"), "utf8");
  const name = base.indexOf("\n  1\nLayout1\n");
  const start = base.lastIndexOf("AcDbPlotSettings", name);
  const settings = base
    .slice(start, name)
    .replace("\n142\n1.0\n143\n1.0\n", "\n142\n1.0\n143\n2.0\n")
    .replace("\n 73\n0\n", "\n 73\n1\n");
  expect(settings).toContain("\n143\n2.0\n");
  expect(settings).toContain("\n 73\n1\n");
  return base.slice(0, start) + settings + base.slice(name);
}

/** Every coordinate a PDF's path operators name, in points. */
function pathCoordinates(pdf: string): number[] {
  return [...pdf.matchAll(/^(-?[\d.]+) (-?[\d.]+) [ml]$/gm)].flatMap((match) => [
    Number(match[1]),
    Number(match[2]),
  ]);
}

test("the M11 scenario: a wide polyline, a leader, construction lines trimmed, a quick select on two properties, the layout plotted at its file's settings", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // 1. A file whose Layout1 plots at 1:2, turned 90°.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "site.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(sheetAtHalf()),
  });
  await expect(logLines(page).filter({ hasText: "Imported 3 lines, 1 viewport." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "Z", "W", "-20,-40", "120,70");
  const screen = await mapping(page, canvas);
  const pick = async (x: number, y: number) => {
    const at = screen(x, y);
    await page.mouse.click(at.x, at.y);
  };

  // 2. A polyline with an arc, given a width and closed by PEDIT.
  await enter(page, "PL", "10,10", "60,10", "A", "60,40", "L", "10,40", "");
  await enter(page, "PEDIT");
  await pick(35, 10);
  await expect(prompt(page)).toHaveText(/^Enter an option/);
  await enter(page, "W", "3", "C", "");
  await expect(prompt(page)).toHaveText("Command:");

  // 3. A leader to a note, red, on a layer of its own.
  await enter(page, "LAYER", "N", "NOTES", "S", "NOTES", "", "COLOR", "red");
  await enter(page, "LEADER", "35,40", "20,55", "");
  await expect(prompt(page)).toHaveText("Enter leader text <none>:");
  await enter(page, "Wall to be kept");
  await expect(prompt(page)).toHaveText("Command:");

  // 4. Two red construction lines on layer 0, trimmed at the site line
  // into rays running down from it.
  await enter(page, "LAYER", "S", "0", "");
  await enter(page, "XLINE", "V", "20,0", "80,0", "");
  await enter(page, "COLOR", "bylayer");
  await enter(page, "TRIM");
  await pick(50, 0);
  await enter(page, "");
  await pick(20, 5);
  await pick(80, 5);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Command:");

  // 5. A quick select on two properties: red, and on layer 0 — the two
  // rays, not the leader.
  await enter(page, "QS", "", "C", "= red");
  await expect(logLines(page).last()).toHaveText("3 match so far.");
  await enter(page, "LA", "= 0");
  await expect(logLines(page).last()).toHaveText("2 match so far.");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("2 found.");
  await expect(page.getByTestId("selection-headline")).toHaveText("2 rays");
  await page.keyboard.press("Escape");

  // 6. Layout1 plotted at the file's settings: every default its own.
  await tab(page, "Layout1").click();
  await enter(page, "PLOT", "", "");
  await expect(prompt(page)).toHaveText(/<1:2>:$/);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Plot rotation in degrees <90>:");
  const plot = await downloadFrom(page, () => enter(page, "", "", ""));
  expect(plot.name).toBe("site-Layout1.pdf");
  expect(plot.text).toContain("/MediaBox [0 0 841.89 1190.551]");
  await expect(logLines(page).last()).toHaveText(
    'Plotting the sheet of layout "Layout1" on A3, at 1:2, turned 90°.',
  );
  // The band of the wide polyline is filled; the rays, a million units
  // long in the drawing, are clipped to what the page shows of them.
  expect(plot.text).toMatch(/ l h f\n/);
  const coordinates = pathCoordinates(plot.text);
  expect(coordinates.length).toBeGreaterThan(0);
  expect(Math.min(...coordinates)).toBeGreaterThanOrEqual(-1);
  expect(Math.max(...coordinates)).toBeLessThanOrEqual(1191);

  // 7. Saved with it all.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const polyline = entities(file.text, "LWPOLYLINE")[0];
  expect([group(polyline, 43), group(polyline, 70)]).toEqual(["3.0", "1"]);
  const leaders = entities(file.text, "LEADER");
  expect(leaders).toHaveLength(1);
  expect(group(leaders[0], 8)).toBe("NOTES");
  expect(entities(file.text, "MTEXT").map((note) => group(note, 1))).toEqual(["Wall to be kept"]);
  expect(entities(file.text, "XLINE")).toHaveLength(0);
  const rays = entities(file.text, "RAY");
  expect(rays.map((ray) => [group(ray, 10), group(ray, 20), group(ray, 62)])).toEqual([
    ["20.0", "0.0", "1"],
    ["80.0", "0.0", "1"],
  ]);
  const layout = file.text.slice(
    file.text.lastIndexOf("AcDbPlotSettings", file.text.indexOf("\n  1\nLayout1\n")),
  );
  expect(layout).toContain("\n143\n2.0\n");
  expect(layout).toContain("\n 73\n1\n");

  // 8. Reopened: everything there, and the quick select finds the same.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "site.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "Imported 3 lines, 1 polyline" });
  await expect(imported).toHaveCount(1);
  for (const part of ["1 viewport", "1 leader", "2 construction lines"]) {
    await expect(imported).toContainText(part);
  }
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "QS", "", "C", "= red", "LA", "= 0", "");
  await expect(logLines(page).last()).toHaveText("2 found.");
  await page.keyboard.press("Escape");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});
