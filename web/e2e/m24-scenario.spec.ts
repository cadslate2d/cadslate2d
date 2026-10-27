// The M24 exit scenario (#341): general-purpose drawings (ADR-0040). A
// house planned by command, its front façade asked for and accepted, a
// section cut through it by command; an isometric top face asked for and
// an isocircle drawn on it; then, in a new drawing, a block's views asked
// for and a title block round them by command — saved and plotted.
//
// The requests are answered by the tests' stand-in service (ai-fake.ts)
// from the L4 corpus's recordings of the real model.
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

const BRIEF = {
  terrain: { width: 15, depth: 20 },
  setbacks: { front: 3 },
  windows: 1.2,
  rooms: [
    { name: "living", area: 24, entrance: true },
    { name: "kitchen", area: 10 },
    { name: "bedroom", area: 12 },
  ],
  adjacent: [["living", "kitchen"]],
};
const FACADE =
  "Draw the front elevation of this plan with a gable roof at 30 degrees, its left end at 0,-12000";
const TOP_FACE =
  "Draw, in isometric, the outline of the top face of a box 60 long along x, 40 deep along y, at a height of 30, as one closed polyline with a corner at x 0, y 0";
const BLOCK =
  "Draw the standard views of a block 100 wide, 50 high and 40 deep with a 10 mm hole right through it from the front at x 60, z 25, in first-angle projection, its front view's lower left corner at the origin";

const panel = (page: Page) => page.getByTestId("ai-panel");

async function ask(page: Page, request: string, withSelection = false) {
  await panel(page)
    .getByLabel(/What should be drawn or changed\?|How should it change\?/)
    .fill(request);
  await panel(page).getByRole("button", { name: "Ask" }).click();
  if (withSelection) {
    // The selected objects go with it only when the user says so.
    await panel(page)
      .getByRole("region", { name: "Send drawing context?" })
      .getByRole("button", { name: "Send with these" })
      .click();
  }
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  // What the proposal made is the selection: cleared.
  await page.keyboard.press("Escape");
}

async function saved(page: Page): Promise<string> {
  return (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
}

const on = (text: string, layer: string) =>
  entities(text).filter((entity) => group(entity, 8) === layer);

test("the M24 scenario: a façade, a section, an isometric, a part's views on a sheet", async ({
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

  // 1. A house by command, in millimetres.
  await enter(page, "UNITS", "mm", "FLOORPLAN", "B", JSON.stringify(BRIEF), "", "");
  await expect(logLines(page).last()).toContainText("Plan 1 drawn");

  // 2. Its front façade by a request, the plan selected; a section by
  //    command, from the front to the back.
  await page.keyboard.press("Control+a");
  await ask(page, FACADE, true);
  await enter(page, "BSECTION", "N", "PLAN-1", "7500,0", "7500,20000", "20000,-12000");
  await expect(logLines(page).last()).toContainText("Section A-A of PLAN-1 drawn as SECT-1");
  let text = await saved(page);
  expect(on(text, "ELEV-ROOF").length).toBe(1);
  expect(on(text, "ELEV-TEXT").map((e) => group(e, 1))).toContain("FRONT ELEVATION");
  expect(on(text, "SECT-HATCH").length).toBeGreaterThanOrEqual(4);

  // 3. An isometric top face by a request, a hole in it by ELLIPSE
  //    Isocircle on the isometric snap.
  await ask(page, TOP_FACE);
  await enter(page, "SNAP", "S", "I", "10", "ELLIPSE", "I", "0,30", "8");
  text = await saved(page);
  expect(entities(text, "ELLIPSE")).toHaveLength(1);

  // 4. A new drawing: a block's views by a request, a sheet round them by
  //    command; saved and plotted.
  await enter(page, "NEW");
  await ask(page, BLOCK);
  await enter(
    page,
    "TITLEBLOCK",
    "A3",
    "Landscape",
    "1",
    "BLOCK",
    "M-001",
    "Scenario",
    "",
    "",
    "First",
    "-150,-140",
  );
  await expect(logLines(page).last()).toContainText("A3 landscape sheet drawn as TITLE-1");
  text = await saved(page);
  expect(on(text, "PART-HIDDEN").length).toBeGreaterThan(0);
  expect(on(text, "TITLE").map((e) => group(e, 1))).toContain("BLOCK");
  await enter(page, "PLOT");
  const plot = await downloadFrom(page, () => enter(page, "A3", "E", "Fit", "Y", "C"));
  expect(plot.text.startsWith("%PDF-1.4\n")).toBe(true);
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});
