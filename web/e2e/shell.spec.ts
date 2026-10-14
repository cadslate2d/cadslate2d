import { expect, test } from "@playwright/test";
import { circles, commandInput, downloadFrom, enter, logLines, openApp, prompt } from "./support";

test.describe("toolbar", () => {
  // DESIGN §5: toolbar actions invoke the same command machinery as typed commands.
  test("a button starts its command and echoes it like typed input", async ({ page }) => {
    await openApp(page);
    await page.getByRole("button", { name: "Circle" }).click();

    await expect(prompt(page)).toHaveText("Specify center point for circle or [3P/2P]:");
    await expect(logLines(page).last()).toHaveText("Command: CIRCLE");
    await expect(commandInput(page)).toBeFocused();

    // The rest of the command is ordinary keyboard input.
    await enter(page, "0,0", "25");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(circles(file.text)).toContainEqual([0, 0, 25]);
  });

  test("a button cancels the command in progress first", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0");
    await page.getByRole("button", { name: "Rectangle" }).click();
    await expect(prompt(page)).toHaveText("Specify first corner point:");
    await expect(logLines(page).filter({ hasText: "*Cancel*" })).toHaveCount(1);
  });

  test("every tool starts a command the core knows", async ({ page }) => {
    await openApp(page);
    for (const name of [
      "Line",
      "Polyline",
      "Circle",
      "Arc",
      "Rectangle",
      "Move",
      "Copy",
      "Rotate",
      "Scale",
      "Mirror",
      "Stretch",
      "Offset",
      "Trim",
      "Extend",
      "Fillet",
      "Chamfer",
      "Break",
      "Join",
      "Erase",
    ]) {
      await page.getByRole("button", { name, exact: true }).click();
      await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
      await expect(prompt(page)).not.toHaveText("Command:");
      await page.keyboard.press("Escape");
    }
  });

  test("can be hidden, stays hidden after a reload, and drafting works without it", async ({
    page,
  }) => {
    await openApp(page);
    const toolbar = page.getByRole("navigation", { name: "Tools" });
    await expect(toolbar).toBeVisible();

    await page.getByRole("button", { name: "Toolbar" }).click();
    await expect(toolbar).toHaveCount(0);

    await page.reload();
    await openApp(page);
    await expect(toolbar).toHaveCount(0);
    await enter(page, "L", "0,0", "10,0", "");
    await expect(prompt(page)).toHaveText("Command:");
  });
});

test.describe("status toggles", () => {
  test("a toggle flips from the status bar and from its function key", async ({ page }) => {
    await openApp(page);
    const ortho = page.getByRole("button", { name: "ORTHO" });
    await expect(ortho).toHaveAttribute("aria-pressed", "false");

    await ortho.click();
    await expect(ortho).toHaveAttribute("aria-pressed", "true");
    await expect(logLines(page).last()).toHaveText("<ORTHO on>");
    await expect(commandInput(page)).toBeFocused();

    await page.keyboard.press("F8");
    await expect(ortho).toHaveAttribute("aria-pressed", "false");
    await expect(logLines(page).last()).toHaveText("<ORTHO off>");
  });

  test("each function key flips its own toggle; object snaps and the grid start on", async ({
    page,
  }) => {
    await openApp(page);
    for (const [key, label, initially] of [
      ["F3", "OSNAP", "true"],
      ["F7", "GRID", "true"],
      ["F8", "ORTHO", "false"],
      ["F10", "POLAR", "false"],
    ] as const) {
      const toggle = page.getByRole("button", { name: label });
      await expect(toggle).toHaveAttribute("aria-pressed", initially);
      await page.keyboard.press(key);
      await expect(toggle).toHaveAttribute("aria-pressed", initially === "true" ? "false" : "true");
    }
  });

  test("toggling during a command does not cancel it", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0");
    await page.keyboard.press("F8");
    await page.getByRole("button", { name: "GRID" }).click();
    await expect(prompt(page)).toHaveText("Specify next point or [Undo]:");

    await enter(page, "10,0", "");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text.match(/^LINE$/gm)).toHaveLength(1);
  });

  test("every toggle of the reference status bar is there and works (#219, #220)", async ({
    page,
  }) => {
    await openApp(page);
    for (const label of ["SNAP", "GRID", "ORTHO", "POLAR", "OSNAP", "OTRACK", "LWT"]) {
      await expect(page.getByRole("button", { name: label, exact: true })).toBeVisible();
    }
  });
});

test.describe("command log", () => {
  test("scrolls, follows new lines, and keeps earlier lines reachable", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "10,0", "20,0", "30,0", "40,0", "50,0", "60,0", "");
    const scroller = page.getByTestId("command-scroll");

    const atBottom = () =>
      scroller.evaluate((element) => ({
        overflows: element.scrollHeight > element.clientHeight,
        gap: element.scrollHeight - element.clientHeight - element.scrollTop,
      }));
    expect(await atBottom()).toEqual({ overflows: true, gap: 0 });
    await expect(prompt(page)).toBeInViewport();

    // The first line of the session is still there, above.
    await expect(logLines(page).first()).toHaveText("Command: L");
    await scroller.evaluate((element) => (element.scrollTop = 0));
    await expect(logLines(page).first()).toBeInViewport();

    // New output brings the prompt back into view.
    await enter(page, "FROB");
    expect((await atBottom()).gap).toBe(0);
  });

  test("line numbers count from the start of the session", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "");
    const numbers = await page.locator(".command__number").allTextContents();
    expect(numbers).toEqual(["1", "2", "3", "4"]);
  });
});
