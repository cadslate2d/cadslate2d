import { expect, test } from "@playwright/test";
import {
  blocks,
  chooseFile,
  downloadBytesFrom,
  downloadFrom,
  dwgFixturePath,
  enter,
  entities,
  entitiesSection,
  fixturePath,
  group,
  lines,
  litPixels,
  logLines,
  openApp,
  prompt,
  recordRequests,
  settle,
  tableRow,
} from "./support";

test.describe("export", () => {
  // DESIGN §26.3 criterion 16.
  test("DXFOUT downloads the drawing as DXF", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "100,50", "");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));

    expect(file.name).toBe("drawing.dxf");
    expect(file.text).toContain("$ACADVER\n  1\nAC1015\n");
    expect(lines(file.text)).toContainEqual([0, 0, 100, 50]);
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
    expect(first.text.match(/^LWPOLYLINE$/gm)).toHaveLength(3);
    // Two semicircles, one quarter arc, one clockwise arc.
    expect(entitiesSection(first.text).match(/^ 42$/gm)).toHaveLength(4);
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

  // #318: a DWG of each version opens like its DXF, told by its bytes.
  for (const version of ["AC1015", "AC1018", "AC1021"]) {
    test(`a ${version} DWG opens with the entities and layers of its DXF`, async ({ page }) => {
      const canvas = await openApp(page);
      await chooseFile(
        page,
        () => enter(page, "OPEN"),
        dwgFixturePath(`valid/ezdxf-r12-three-lines.${version}.dwg`),
      );
      await expect(logLines(page).filter({ hasText: "Imported 3 lines." })).toHaveCount(1);
      await expect(logLines(page).last()).toHaveText("With 1 layer.");
      await expect(page.getByTestId("document")).toContainText(
        `ezdxf-r12-three-lines.${version}.dwg`,
      );
      expect(await litPixels(canvas)).toBeGreaterThan(300);
      // What the drawing holds is what the DXF holds: the same three lines.
      const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
      expect(file.name).toBe(`ezdxf-r12-three-lines.${version}.dxf`);
      expect(file.text.match(/^LINE$/gm)).toHaveLength(3);
    });
  }

  // #319: a DWG opened, edited, and saved as each version comes back with the edit.
  test("a DWG edited and saved as each version reopens with the edit", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => enter(page, "OPEN"),
      dwgFixturePath("valid/ezdxf-r12-three-lines.AC1015.dwg"),
    );
    await expect(logLines(page).filter({ hasText: "Imported 3 lines." })).toHaveCount(1);
    await enter(page, "L", "0,0", "40,40", "");
    let round = 0;
    for (const [keyword, signature] of [
      ["DWG2000", "AC1015"],
      ["DWG2004", "AC1018"],
      ["DWG2007", "AC1021"],
    ] as const) {
      round += 1;
      const file = await downloadBytesFrom(page, () => enter(page, "SAVEAS", keyword));
      expect(file.name).toBe(round === 1 ? "ezdxf-r12-three-lines.AC1015.dwg" : "again.dwg");
      expect(file.bytes.subarray(0, 6).toString()).toBe(signature);
      await expect(logLines(page).last()).toHaveText(
        `Drawing exported as DWG ${keyword.slice(3)}.`,
      );
      // Reopened: the three lines and the edit, whichever version carried them.
      await chooseFile(page, () => enter(page, "OPEN"), {
        name: "again.dwg",
        mimeType: "application/acad",
        buffer: file.bytes,
      });
      await expect(logLines(page).filter({ hasText: "Imported 4 lines." })).toHaveCount(round);
      await expect(page.getByTestId("document")).toContainText("again.dwg");
      // The next SAVEAS starts from this file's version.
      await enter(page, "SAVEAS");
      await expect(prompt(page)).toHaveText(
        `Save as or [DXF/DWG2000/DWG2004/DWG2007] <${keyword}>:`,
      );
      await page.keyboard.press("Escape");
      await settle(page);
    }
  });

  test("a DWG with content the drawing cannot hold is LOSSY, like a DXF", async ({ page }) => {
    await openApp(page);
    await chooseFile(
      page,
      () => enter(page, "OPEN"),
      dwgFixturePath("valid/crafted-mline-entity.AC1015.dwg"),
    );
    const report = page.getByTestId("import-report");
    await expect(report).toContainText("MLINE");
    await expect(page.getByTestId("lossy")).toHaveText("LOSSY");
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
    // Nested blocks placed every way: an array, a non-uniform scale, and an
    // INSERT of a block the file never defines are reported.
    await chooseFile(page, () => enter(page, "OPEN"), fixturePath("valid/ezdxf-r12-blocks.dxf"));

    await expect(
      logLines(page).filter({ hasText: "Imported 1 line, 6 block references." }),
    ).toHaveCount(1);
    await expect(
      logLines(page).filter({ hasText: "Not imported (unsupported): 1 (INSERT array)" }),
    ).toHaveCount(1);

    // The report is on screen, naming the losses, and the status bar says so.
    const report = page.getByTestId("import-report");
    await expect(report).toContainText("Imported 1 line, 6 block references.");
    await expect(report).toContainText("Not imported (unsupported): 1 (INSERT array)");
    await expect(page.getByTestId("lossy")).toHaveText("LOSSY");

    // Declining the warning exports nothing; it named what would be lost.
    let downloads = 0;
    page.on("download", () => (downloads += 1));
    await enter(page, "DXFOUT");
    const warning = page.getByTestId("export-warning");
    await expect(warning).toContainText("(INSERT of an undefined block)");
    await warning.getByRole("button", { name: "Cancel" }).click();
    await expect(warning).toHaveCount(0);
    await settle(page);
    expect(downloads).toBe(0);

    // Dismissed, the report goes; the losses stay.
    await report.getByRole("button", { name: "Dismiss the import report" }).click();
    await expect(report).toHaveCount(0);
    await expect(page.getByTestId("lossy")).toHaveText("LOSSY");

    // Accepting it exports what the drawing holds: the definitions and the
    // six references, the ghost among them no more.
    const file = await downloadFrom(page, async () => {
      await enter(page, "DXFOUT");
      await page
        .getByTestId("export-warning")
        .getByRole("button", { name: "Export anyway" })
        .click();
    });
    expect(file.name).toBe("ezdxf-r12-blocks.dxf");
    expect(blocks(file.text)).toEqual(["CHAIR", "PAIR"]);
    expect(file.text.match(/^INSERT$/gm)).toHaveLength(8);
    expect(file.text).not.toContain("GHOST");
    // The two chairs of the pair are in its definition; the model holds one line.
    expect(file.text.match(/^LINE$/gm)).toHaveLength(2);
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
    expect(tableRow(file.text, "LTYPE", "Dashed")).toBeDefined();
    const walls = tableRow(file.text, "LAYER", "walls");
    expect([group(walls, 70), group(walls, 62), group(walls, 6)]).toEqual(["0", "1", "Dashed"]);
    expect(file.text).toContain("  9\n$CLAYER\n  8\nwalls\n");
    expect(file.text).toContain("  9\n$LTSCALE\n 40\n5.0\n");
    const line = entities(file.text, "LINE")[0];
    expect([group(line, 8), group(line, 62), group(line, 370)]).toEqual(["walls", "5", undefined]);
    // The circle was drawn after LWEIGHT 0.5: its lineweight is written (ADR-0018).
    expect(group(entities(file.text, "CIRCLE")[0], 370)).toBe("50");
    await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");

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
