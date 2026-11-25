// The docked panels' sizes: the column widened and narrowed by its left
// edge, its height shared by the dividers between panels — by the pointer
// and by the keyboard, kept to their limits, remembered across a reload,
// and back to the usual sizes on a double-click. The canvas follows.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { MIN_HEIGHT, MIN_WIDTH, STEP } from "../src/ui/PanelSizes";
import { chooseMenu, enter, logLines, openApp, prompt, rendererStats } from "./support";

const layers = (page: Page) => page.getByRole("complementary", { name: "Layers" });
const properties = (page: Page) => page.getByRole("complementary", { name: "Properties" });
const ai = (page: Page) => page.getByRole("complementary", { name: "AI Assistant" });
const edge = (page: Page) => page.getByRole("separator", { name: "Panel width" });
const divider = (page: Page, above: string, below: string) =>
  page.getByRole("separator", { name: `Height of ${above} and ${below}` });

async function width(locator: Locator): Promise<number> {
  const box = await locator.boundingBox();
  if (!box) throw new Error("not shown");
  return box.width;
}

async function height(locator: Locator): Promise<number> {
  const box = await locator.boundingBox();
  if (!box) throw new Error("not shown");
  return box.height;
}

/** Within a pixel and a half: flex layout rounds. */
async function near(read: () => Promise<number>, expected: number): Promise<void> {
  await expect
    .poll(async () => Math.abs((await read()) - expected), { message: `near ${expected}` })
    .toBeLessThan(1.5);
}

/**
 * Drags `handle` by `dx`, `dy` with the mouse, in steps, as a hand does —
 * stopping at the window's edge, as a hand does: a drag "past the limit"
 * goes as far as the window allows, and the handle stops at its limit.
 */
async function drag(page: Page, handle: Locator, dx: number, dy: number): Promise<void> {
  const box = await handle.boundingBox();
  const window = page.viewportSize();
  if (!box || !window) throw new Error("no handle");
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  const inside = (value: number, size: number) => Math.min(Math.max(value, 2), size - 2);
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(inside(x + dx, window.width), inside(y + dy, window.height), {
    steps: 8,
  });
  await page.mouse.up();
}

test.describe("panel sizes", () => {
  // Room for each panel to give up 60 px and keep its least height.
  test.use({ viewport: { width: 1600, height: 1000 } });

  test("the column is widened and narrowed by its edge, kept, and the canvas follows", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    const panelWidth = () => width(layers(page));
    const canvasWidth = () => width(canvas);
    const [panel0, canvas0] = [await panelWidth(), await canvasWidth()];

    // Dragged left 120 px: the panels gain what the canvas gives up.
    await drag(page, edge(page), -120, 0);
    await near(panelWidth, panel0 + 120);
    await near(canvasWidth, canvas0 - 120);
    // The canvas draws at its new size: its backing store follows its box.
    await expect
      .poll(() => canvas.evaluate((c: HTMLCanvasElement) => c.width / devicePixelRatio))
      .toBeCloseTo(canvas0 - 120, -1);
    await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);

    // Remembered across a reload.
    await page.reload();
    await openApp(page);
    await near(panelWidth, panel0 + 120);

    // From the keyboard: an arrow moves the edge a step, and the keys it
    // takes do not reach the command line.
    await edge(page).focus();
    await page.keyboard.press("ArrowRight");
    await near(panelWidth, panel0 + 120 - STEP);
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await near(panelWidth, panel0 + 120 + STEP);
    await expect(prompt(page)).toHaveText("Command:");

    // Never narrower than its least, nor wider than most of the window.
    await page.keyboard.press("End");
    await expect(edge(page)).toHaveAttribute("aria-valuenow", String(MIN_WIDTH));
    await drag(page, edge(page), 2000, 0);
    await expect(edge(page)).toHaveAttribute("aria-valuenow", String(MIN_WIDTH));
    await drag(page, edge(page), -5000, 0);
    const widest = Number(await edge(page).getAttribute("aria-valuemax"));
    await expect(edge(page)).toHaveAttribute("aria-valuenow", String(widest));
    expect(await canvasWidth()).toBeGreaterThan(0);

    // A double-click puts the usual width back.
    await edge(page).dblclick();
    await near(panelWidth, panel0);
    await near(canvasWidth, canvas0);

    // Typing still goes to the command line.
    await enter(page, "L", "0,0", "10,0", "");
    await expect(logLines(page).filter({ hasText: "Command: L" })).toHaveCount(1);
  });

  test("the panels share the height by the dividers between them, and keep it", async ({
    page,
  }) => {
    await openApp(page);
    const [layers0, properties0, ai0] = [
      await height(layers(page)),
      await height(properties(page)),
      await height(ai(page)),
    ];
    const between = divider(page, "Layers", "Properties");

    // Down 60 px: Layers gains what Properties gives up; the AI panel keeps its own.
    await drag(page, between, 0, 60);
    await near(() => height(layers(page)), layers0 + 60);
    await near(() => height(properties(page)), properties0 - 60);
    await near(() => height(ai(page)), ai0);

    // Never under the least height.
    await drag(page, between, 0, 2000);
    await near(() => height(properties(page)), MIN_HEIGHT);
    await near(() => height(layers(page)), layers0 + properties0 - MIN_HEIGHT);
    await near(() => height(ai(page)), ai0);

    // Remembered across a reload.
    await page.reload();
    await openApp(page);
    await near(() => height(properties(page)), MIN_HEIGHT);

    // From the keyboard, between Properties and the AI panel.
    const lower = divider(page, "Properties", "AI Assistant");
    await lower.focus();
    await page.keyboard.press("ArrowDown");
    await near(() => height(properties(page)), MIN_HEIGHT + STEP);
    await near(() => height(ai(page)), ai0 - STEP);
    await page.keyboard.press("ArrowUp");
    await near(() => height(properties(page)), MIN_HEIGHT);
    await expect(prompt(page)).toHaveText("Command:");

    // A double-click puts the usual shares back.
    await lower.dblclick();
    await near(() => height(layers(page)), layers0);
    await near(() => height(properties(page)), properties0);
    await near(() => height(ai(page)), ai0);

    // A panel hidden takes its dividers with it; the others still divide,
    // and the hidden one comes back the height it had.
    await drag(page, between, 0, 30);
    await near(() => height(properties(page)), properties0 - 30);
    await chooseMenu(page, "View", "Properties Panel");
    await expect(properties(page)).toHaveCount(0);
    await expect(page.getByRole("separator", { name: /^Height of/ })).toHaveCount(1);
    await drag(page, divider(page, "Layers", "AI Assistant"), 0, 20);
    await chooseMenu(page, "View", "Properties Panel");
    await near(() => height(properties(page)), properties0 - 30);
  });
});
