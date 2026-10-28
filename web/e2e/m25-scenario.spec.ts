// The M25 exit scenario (#359): composed sheets (ADR-0041). A bracket's
// sheet asked for and accepted — its views placed by the engine, true size
// and dimensioned, a bill of materials and a balloon — then edited by
// command with notes of its own and the edit undone and redone, and
// exported as DXF and plotted.
//
// The request is answered by the tests' stand-in service (ai-fake.ts) from
// the L5 corpus's recording of the real model.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService } from "./ai-fake";
import {
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
  rendererStats,
} from "./support";

const SHEET =
  "Draw a drawing sheet for an L-bracket on an A4 sheet at 1:1 in third-angle projection, with front, left side and top views. The bracket is 80 mm wide, 60 mm high and 40 mm deep, of 5 mm plate. Dimension the width, height and depth. Add a bill of materials — part 1, Bracket, 1 off, steel S235 — and balloon it.";

const panel = (page: Page) => page.getByTestId("ai-panel");

async function saved(page: Page): Promise<string> {
  return (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
}

const on = (text: string, layer: string) =>
  entities(text).filter((entity) => group(entity, 8) === layer);

test("the M25 scenario: a composed sheet asked for, accepted, edited, exported", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  service.plan = "pro";
  const canvas = await openApp(page);
  await panel(page).getByLabel("Email").fill("scenario@example.com");
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor("scenario@example.com")}`);
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  await enter(page, "UNITS", "mm");

  // 1. The sheet asked for, and accepted as one proposal.
  await panel(page).getByLabel("What should be drawn or changed?").fill(SHEET);
  await panel(page).getByRole("button", { name: "Ask" }).click();
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  await page.keyboard.press("Escape");

  // Its views placed and titled by the engine, true size and dimensioned;
  // the table and the balloon on their layers.
  let text = await saved(page);
  const titles = on(text, "VIEW-TITLES").map((e) => group(e, 1));
  // Each view and the table titled as the model words them: "Front",
  // "Front View", … — the recording is re-made as instructions change.
  for (const word of [/front/i, /left/i, /top/i, /bill of materials/i]) {
    expect(titles.some((title) => word.test(title ?? ""))).toBe(true);
  }
  expect(entities(text, "DIMENSION").length).toBeGreaterThanOrEqual(3);
  expect(on(text, "BOM").length).toBeGreaterThan(0);
  expect(on(text, "BALLOONS").length).toBeGreaterThan(0);
  const notesBefore = on(text, "NOTES").length;

  // 2. Edited by command: notes of its own placed beside the views, one
  //    undo; undone and redone.
  await enter(page, "NOTES", "Break all sharp edges.", "Finish: zinc plated.", "", "70", "");
  await expect(logLines(page).last()).toContainText("Notes placed on TITLE-1 as notes-2");
  text = await saved(page);
  expect(on(text, "NOTES").length).toBeGreaterThan(notesBefore);
  await enter(page, "U");
  text = await saved(page);
  expect(on(text, "NOTES")).toHaveLength(notesBefore);
  await enter(page, "REDO");
  text = await saved(page);
  expect(on(text, "NOTES").length).toBeGreaterThan(notesBefore);

  // 3. Exported: the DXF above, and plotted to PDF.
  await enter(page, "PLOT");
  const plot = await downloadFrom(page, () => enter(page, "A3", "E", "Fit", "Y", "C"));
  expect(plot.text.startsWith("%PDF-1.4\n")).toBe(true);
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});
