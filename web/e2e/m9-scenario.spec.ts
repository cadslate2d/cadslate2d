// The M9 exit scenario (#195): the M8 scenario file opened with its
// layouts shown, a viewport added and scaled, a title block drawn on
// paper, the layout plotted, and the file saved and reopened with it all.
import { expect, test, type Page } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  mapping,
  openApp,
  prompt,
  scenarioDxf,
} from "./support";

const tab = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Layouts" }).getByRole("button", { name, exact: true });
const space = (page: Page) => page.getByTestId("space");

test("the M9 scenario: a file's layouts shown, a viewport added and scaled, a title block drawn, the layout plotted, saved, and reopened", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // 1. Opened with its layouts as layouts: three tabs, the file's
  // viewport an entity of Layout1, nothing kept opaque.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "scenario.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(scenarioDxf()),
  });
  await expect(logLines(page).filter({ hasText: "1 viewport." })).toHaveCount(1);
  await expect(logLines(page).filter({ hasText: "2 layouts: Layout1, Layout2." })).toHaveCount(1);
  await expect(page.getByTestId("lossy")).toHaveCount(0);
  await expect(page.getByTestId("layout-tab")).toHaveCount(3);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();

  // 2. On Layout1: the paper, its view the sheet.
  await tab(page, "Layout1").click();
  await expect(space(page)).toHaveText("PAPER");
  await expect(tab(page, "Layout1")).toHaveAttribute("aria-pressed", "true");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("2 selected.");
  await expect(page.getByTestId("selection-headline")).toHaveText("2 objects: 1 line, 1 viewport");
  await page.keyboard.press("Escape");

  // 3. A second viewport, then scaled to half by its frame.
  await enter(page, "MVIEW", "300,220", "400,280");
  await expect(logLines(page).last()).toContainText("Viewport at 1:");
  const screen = await mapping(page, canvas);
  const edge = screen(350, 280);
  await page.mouse.click(edge.x, edge.y);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 viewport");
  await enter(page, "SC", "300,220", "0.5");
  await expect(prompt(page)).toHaveText("Command:");

  // 4. A title block on the paper: a rectangle and a line of text.
  await enter(page, "REC", "300,10", "410,60");
  await enter(page, "DT", "310,30", "8", "0", "Plan");
  await expect(prompt(page)).toHaveText("Command:");

  // 5. The layout plotted: the sheet as the page, a clip per viewport.
  const plot = await downloadFrom(page, () => enter(page, "PLOT", "", "", "", "", ""));
  expect(plot.name).toBe("scenario-Layout1.pdf");
  expect(plot.text).toContain("/MediaBox [0 0 1190.551 841.89]");
  expect(plot.text.match(/ re W n\n/g)).toHaveLength(3);
  await expect(logLines(page).last()).toHaveText(
    'Plotting the sheet of layout "Layout1" on A3, to fit.',
  );

  // 6. Saved with it all: the two viewports — the new one at half its
  // size — the title block on the paper, and the model as it was.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const viewports = entities(file.text, "VIEWPORT", "BLOCKS");
  expect(viewports).toHaveLength(2);
  expect(viewports.map((viewport) => group(viewport, 40))).toEqual(["277.0", "50.0"]);
  const text = entities(file.text, "TEXT", "BLOCKS")[0];
  expect([group(text, 1), group(text, 67)]).toEqual(["Plan", "1"]);
  expect(entities(file.text, "LWPOLYLINE", "BLOCKS")).toHaveLength(2);
  expect(entities(file.text, "ELLIPSE")).toHaveLength(1);
  expect(entities(file.text, "SPLINE")).toHaveLength(1);
  await expect(page.getByTestId("lossy")).toHaveCount(0);

  // 7. Reopened: the layouts and everything on them.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "scenario.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  await expect(logLines(page).filter({ hasText: "2 viewports." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await expect(page.getByTestId("layout-tab")).toHaveCount(3);
  await tab(page, "Layout1").click();
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("5 selected.");
  await page.keyboard.press("Escape");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});
