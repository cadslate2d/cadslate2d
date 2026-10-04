import { expect, test } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  fixturePath,
  litPixels,
  logLines,
  openApp,
  recordRequests,
  settle,
} from "./support";

test.describe("export", () => {
  // DESIGN §26.3 criterion 16.
  test("DXFOUT downloads the drawing as DXF", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "100,50", "");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));

    expect(file.name).toBe("drawing.dxf");
    expect(file.text).toContain("$ACADVER\n  1\nAC1009\n");
    expect(file.text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n50.0\n");
    await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");
  });

  test("Ctrl+S exports instead of opening the browser's save dialog", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "10,0", "");
    const file = await downloadFrom(page, () => page.keyboard.press("Control+s"));
    expect(file.text).toContain("LINE");
  });
});

test.describe("open", () => {
  // DESIGN §26.3 criterion 17.
  test("OPEN restores a line from a DXF file and frames it", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);

    await chooseFile(page, () => enter(page, "OPEN"), fixturePath("valid/minimal-line.dxf"));

    await expect(logLines(page).last()).toHaveText("Imported 1 line.");
    expect(await litPixels(canvas)).toBeGreaterThan(empty + 300);
  });

  test("Ctrl+O opens the file picker", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => page.keyboard.press("Control+o"),
      fixturePath("valid/ezdxf-r12-three-lines.dxf"),
    );
    await expect(logLines(page).last()).toHaveText("Imported 3 lines.");
  });

  test("a file that is not DXF is refused with a reason", async ({ page }) => {
    await openApp(page);
    await chooseFile(page, () => enter(page, "OPEN"), fixturePath("malformed/not-dxf.dxf"));
    await expect(logLines(page).last()).toHaveText(
      "Cannot open not-dxf.dxf: line 1: expected a DXF group code.",
    );
  });

  test("opening and exporting make no network requests", async ({ page }) => {
    await openApp(page);
    const requests = recordRequests(page);

    await chooseFile(page, () => enter(page, "OPEN"), fixturePath("valid/minimal-line.dxf"));
    await downloadFrom(page, () => enter(page, "DXFOUT"));

    // The download itself is a local blob: URL, not a request to any server.
    expect(requests.filter((request) => !request.url.startsWith("blob:"))).toEqual([]);
  });
});

// DESIGN §24.2 (LOCKED): unsupported content is reported, and the user is
// warned before a potentially lossy export.
test.describe("unsupported content", () => {
  test("is reported on import and confirmed before export", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => enter(page, "OPEN"),
      fixturePath("valid/ezdxf-r2010-mixed-content.dxf"),
    );

    await expect(
      logLines(page).filter({ hasText: "Imported 3 lines, 2 circles, 1 arc." }),
    ).toHaveCount(1);
    await expect(
      logLines(page).filter({ hasText: "Not imported (unsupported): 1 BLOCK" }),
    ).toHaveCount(1);

    // Declining the warning exports nothing.
    const messages: string[] = [];
    page.once("dialog", (dialog) => {
      messages.push(dialog.message());
      void dialog.dismiss();
    });
    let downloads = 0;
    page.on("download", () => (downloads += 1));
    await enter(page, "DXFOUT");
    await settle(page);
    expect(messages[0]).toContain("could not import");
    expect(downloads).toBe(0);

    // Accepting it exports the supported geometry only.
    page.once("dialog", (dialog) => void dialog.accept());
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.name).toBe("ezdxf-r2010-mixed-content.dxf");
    expect(file.text.match(/^LINE$/gm)).toHaveLength(3);
    expect(file.text.match(/^CIRCLE$/gm)).toHaveLength(2);
    expect(file.text.match(/^ARC$/gm)).toHaveLength(1);
    expect(file.text).not.toContain("LWPOLYLINE");
    expect(file.text).not.toContain("TEXT");
  });
});

test.describe("unsaved work", () => {
  test("leaving the page with unexported changes asks first", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "10,0", "");

    const dialogs: string[] = [];
    page.on("dialog", (dialog) => {
      dialogs.push(dialog.type());
      void dialog.accept();
    });
    await page.reload();
    expect(dialogs).toEqual(["beforeunload"]);
  });

  test("leaving after exporting does not ask", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "10,0", "");
    await downloadFrom(page, () => enter(page, "DXFOUT"));

    const dialogs: string[] = [];
    page.on("dialog", (dialog) => {
      dialogs.push(dialog.type());
      void dialog.accept();
    });
    await page.reload();
    expect(dialogs).toEqual([]);
  });

  test("opening over unexported changes asks first", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "10,0", "");

    const messages: string[] = [];
    page.once("dialog", (dialog) => {
      messages.push(dialog.message());
      void dialog.dismiss();
    });
    await enter(page, "OPEN");
    await settle(page);
    expect(messages[0]).toContain("have not been exported");
  });
});
