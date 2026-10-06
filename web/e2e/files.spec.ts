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

  test("polylines with arc segments open, draw their arcs, and export them again", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await chooseFile(
      page,
      () => enter(page, "OPEN"),
      fixturePath("valid/ezdxf-r2010-bulged-lwpolylines.dxf"),
    );
    await expect(logLines(page).filter({ hasText: "Imported 3 polylines." })).toHaveCount(1);
    // The file's header says the drawing is in metres.
    await expect(logLines(page).last()).toHaveText("Drawing unit: metres.");

    const first = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(first.text.match(/^POLYLINE$/gm)).toHaveLength(3);
    // Two semicircles, one quarter arc, one clockwise arc.
    expect(first.text.match(/^ 42$/gm)).toHaveLength(4);
    expect(first.text).toContain(" 42\n-0.5\n");

    // Reopening our own export gives the same file again.
    await chooseFile(page, () => enter(page, "OPEN"), {
      name: "again.dxf",
      mimeType: "application/dxf",
      buffer: Buffer.from(first.text),
    });
    // The unit came through our own export and back.
    expect(first.text).toContain("$INSUNITS\n 70\n6\n");
    await expect(logLines(page).last()).toHaveText("Drawing unit: metres.");
    const second = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(second.text).toBe(first.text);
    expect(await litPixels(canvas)).toBeGreaterThan(300);
  });

  test("Ctrl+O opens the file picker", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => page.keyboard.press("Control+o"),
      fixturePath("valid/ezdxf-r12-three-lines.dxf"),
    );
    await expect(logLines(page).filter({ hasText: "Imported 3 lines." })).toHaveCount(1);
    // ezdxf's Defpoints layer came with the file.
    await expect(logLines(page).last()).toHaveText("With 1 layer.");
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
      logLines(page).filter({ hasText: "Imported 3 lines, 2 circles, 1 arc, 1 polyline." }),
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
    // R12 has no LWPOLYLINE: the imported polyline is written as POLYLINE/VERTEX.
    expect(file.text.match(/^POLYLINE$/gm)).toHaveLength(1);
    expect(file.text).not.toContain("LWPOLYLINE");
    expect(file.text).not.toContain("TEXT");
  });
});

// #139: layers, colours, and linetypes travel through the file; lineweights
// cannot in R12, and the export says so.
test.describe("properties in DXF", () => {
  test("layers, colours, and linetypes survive export and open", async ({ page }) => {
    await openApp(page);
    await enter(page, "LINETYPE", "L", "Dashed", "", "LTSCALE", "5");
    await enter(
      page,
      "LAYER",
      "N",
      "walls",
      "C",
      "1",
      "walls",
      "LT",
      "Dashed",
      "walls",
      "S",
      "walls",
      "",
    );
    await enter(page, "COLOR", "5", "L", "0,0", "100,0", "");
    await enter(page, "LWEIGHT", "0.5");
    await expect(logLines(page).last()).toHaveText(
      "Lineweights are not being shown: switch LWT on to see them.",
    );
    await enter(page, "C", "50,50", "10");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain("  2\nLTYPE\n 70\n2\n");
    expect(file.text).toContain("  0\nLTYPE\n  2\nDashed\n");
    expect(file.text).toContain("  0\nLAYER\n  2\nwalls\n 70\n0\n 62\n1\n  6\nDashed\n");
    expect(file.text).toContain("  9\n$CLAYER\n  8\nwalls\n");
    expect(file.text).toContain("  9\n$LTSCALE\n 40\n5.0\n");
    expect(file.text).toContain("LINE\n  8\nwalls\n 62\n5\n");
    expect(file.text).not.toContain("\n370\n");
    await expect(logLines(page).last()).toHaveText(
      "Lineweights are not written in DXF R12: dropped from 1 object.",
    );

    await page.reload();
    await openApp(page);
    await chooseFile(page, () => enter(page, "OPEN"), {
      name: file.name,
      mimeType: "application/dxf",
      buffer: Buffer.from(file.text),
    });
    await expect(logLines(page).filter({ hasText: "Imported 1 line, 1 circle." })).toHaveCount(1);
    await expect(logLines(page).last()).toHaveText("With 1 layer and 1 linetype.");
    await enter(page, "LAYER", "?");
    await expect(logLines(page).last()).toHaveText('"walls": colour 1, on, Dashed, current');
    await enter(page, "", "LINETYPE", "?");
    await expect(logLines(page).last()).toHaveText('"Dashed": __ __ __ __ __ __ __');
    await enter(page, "");
    const again = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(again.text).toBe(file.text);
    await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");
  });

  test("a file from another writer brings its layers and linetypes", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => enter(page, "OPEN"),
      fixturePath("valid/ezdxf-r12-layers-linetypes.dxf"),
    );
    await expect(logLines(page).last()).toHaveText("With 5 layers and 3 linetypes.");
    await enter(page, "LAYER", "?");
    await expect(logLines(page).filter({ hasText: '"HIDDEN": colour 3, off' })).toHaveCount(1);
    await expect(logLines(page).filter({ hasText: '"FIXED": colour 2, on, locked' })).toHaveCount(
      1,
    );
    await expect(
      logLines(page).filter({ hasText: '"WALLS": colour 1, on, DASHED, current' }),
    ).toHaveCount(1);
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
