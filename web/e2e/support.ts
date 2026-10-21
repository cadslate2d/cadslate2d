import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, type Locator, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export interface RendererStats {
  frames: number;
  drawCalls: number;
  sceneUploads: number;
  scenePartialUploads: number;
  styleUploads: number;
  overlayUploads: number;
  bufferReallocations: number;
}

/**
 * How the test's browser offers files (DESIGN §25.3). Chromium has the File
 * System Access pickers, which headless tests cannot drive: by default they
 * are removed before the app starts, so every browser takes the download
 * path; `"fake"` installs pickers that keep files in `window.__files`.
 */
export type FileSystemAccess = "none" | "fake";

export interface OpenOptions {
  readonly fileSystemAccess?: FileSystemAccess;
}

/** Removes the File System Access pickers before the app starts, so saves are downloads. */
export async function withoutFilePickers(page: Page): Promise<void> {
  await page.addInitScript(installFileSystemAccess, "none");
}

/** Opens the app with debug counters enabled and waits for the first frame. */
export async function openApp(page: Page, options: OpenOptions = {}): Promise<Locator> {
  await page.addInitScript(installFileSystemAccess, options.fileSystemAccess ?? "none");
  await page.goto("./?debug");
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  return canvas;
}

/** Runs in the page before the app: removes the pickers, or fakes them. */
function installFileSystemAccess(mode: string): void {
  const scope = window as unknown as Record<string, unknown>;
  if (mode !== "fake") {
    delete scope["showOpenFilePicker"];
    delete scope["showSaveFilePicker"];
    return;
  }
  const files = new Map<string, string>();
  const pickers = { save: 0, open: 0 };
  scope["__files"] = { files, pickers };
  const handle = (name: string) => ({
    kind: "file",
    name,
    queryPermission: () => Promise.resolve("granted"),
    requestPermission: () => Promise.resolve("granted"),
    getFile: () => Promise.resolve(new File([files.get(name) ?? ""], name)),
    createWritable: () =>
      Promise.resolve({
        // Text as it is; a DWG's bytes as the text of their first six, the version id.
        write: (content: string | Uint8Array) => {
          files.set(
            name,
            typeof content === "string"
              ? content
              : new TextDecoder().decode(content.subarray(0, 6)),
          );
          return Promise.resolve();
        },
        close: () => Promise.resolve(),
      }),
  });
  scope["showSaveFilePicker"] = (options?: { suggestedName?: string }) => {
    pickers.save += 1;
    return Promise.resolve(handle(options?.suggestedName ?? "drawing.dxf"));
  };
  scope["showOpenFilePicker"] = () => {
    pickers.open += 1;
    const [name] = [...files.keys()];
    return Promise.resolve(name ? [handle(name)] : []);
  };
}

/** What the fake pickers hold: the files by name, and how often each picker was shown. */
export async function fakeFiles(
  page: Page,
): Promise<{ files: Record<string, string>; pickers: { save: number; open: number } }> {
  return page.evaluate(() => {
    const scope = window as unknown as {
      __files?: { files: Map<string, string>; pickers: { save: number; open: number } };
    };
    const store = scope.__files;
    if (!store) throw new Error("the fake file pickers are not installed");
    return { files: Object.fromEntries(store.files), pickers: { ...store.pickers } };
  });
}

export async function rendererStats(page: Page): Promise<RendererStats> {
  return page.evaluate(() => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return { ...handle.renderer };
  });
}

/** How many times the pointer has rested on the page, and where on the canvas it last did (#220). */
export async function pointerRests(
  page: Page,
): Promise<{ count: number; at: { x: number; y: number } | undefined }> {
  return page.evaluate(() => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return handle.pointerRests();
  });
}

/**
 * Chooses an item of the menu bar with the pointer, as a drafter would:
 * `chooseMenu(page, "View", "Zoom", "Extents")` opens View, hovers the Zoom
 * cascade, and clicks Extents.
 */
export async function chooseMenu(page: Page, title: string, ...path: string[]): Promise<void> {
  await page
    .getByRole("menubar", { name: "Menus" })
    .getByRole("menuitem", { name: title, exact: true })
    .click();
  for (const [level, label] of path.entries()) {
    const entry = menuEntry(page, level, label);
    if (level < path.length - 1) await entry.hover();
    else await entry.click();
  }
}

/** The entry called `label` of the open list at `level`: 0 the menu, 1 a cascade beside it, … */
export function menuEntry(page: Page, level: number, label: string): Locator {
  const quoted = JSON.stringify(label);
  return page
    .getByRole("menu")
    .nth(level)
    .locator(
      `[role="menuitem"][aria-label=${quoted}], [role="menuitemcheckbox"][aria-label=${quoted}]`,
    )
    .first();
}

/** Lets queued animation frames run. */
export async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
}

/** Page coordinates of the centre of `canvas`. */
export async function centerOf(canvas: Locator): Promise<{ x: number; y: number }> {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas is not laid out");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** Types each line and presses Enter after it. */
export async function enter(page: Page, ...lines: string[]): Promise<void> {
  for (const line of lines) {
    if (line) await page.keyboard.type(line);
    await page.keyboard.press("Enter");
  }
  await settle(page);
}

export const prompt = (page: Page): Locator => page.getByTestId("prompt");
export const logLines = (page: Page): Locator => page.getByTestId("log-line");
export const commandInput = (page: Page): Locator => page.locator("#command-input");

type Rgb = readonly [number, number, number];

/** Counts canvas pixels matching `predicate`, from a real screenshot — exactly what the user sees. */
async function countPixels(canvas: Locator, predicate: (rgb: Rgb) => boolean): Promise<number> {
  const image = PNG.sync.read(await canvas.screenshot());
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const rgb = [
      image.data[offset] ?? 0,
      image.data[offset + 1] ?? 0,
      image.data[offset + 2] ?? 0,
    ] as const;
    if (predicate(rgb)) count += 1;
  }
  return count;
}

/** Pixels clearly brighter than the background, i.e. covered by drawn geometry. */
export function litPixels(canvas: Locator): Promise<number> {
  return countPixels(canvas, ([red, green, blue]) => red + green + blue > 3 * 96);
}

/**
 * Pixels that `matches`, in a small rectangle of world space, `half.width` ×
 * `half.height` around a world point, in a view that has not been panned or
 * zoomed.
 */
export async function pixelsAround(
  page: Page,
  canvas: Locator,
  at: { x: number; y: number },
  half: { width: number; height: number },
  matches: (pixel: [red: number, green: number, blue: number]) => boolean,
): Promise<number> {
  const center = await centerOf(canvas);
  const image = PNG.sync.read(
    await page.screenshot({
      clip: {
        x: Math.round(center.x + at.x) - half.width,
        y: Math.round(center.y - at.y) - half.height,
        width: 2 * half.width,
        height: 2 * half.height,
      },
    }),
  );
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const pixel: [number, number, number] = [
      image.data[offset] ?? 0,
      image.data[offset + 1] ?? 0,
      image.data[offset + 2] ?? 0,
    ];
    if (matches(pixel)) count += 1;
  }
  return count;
}

/** Lit pixels — clearly brighter than the background — around a world point. */
export function litPixelsAround(
  page: Page,
  canvas: Locator,
  at: { x: number; y: number },
  half: { width: number; height: number },
): Promise<number> {
  return pixelsAround(page, canvas, at, half, ([red, green, blue]) => red + green + blue > 3 * 96);
}

/** Pixels in the crossing-selection colour (green), including antialiased ones. */
export function crossingPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => green > 80 && green > red * 1.8 && green > blue * 1.5,
  );
}

/** Pixels in the selection accent colour (blue, not white). */
export function highlightPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => blue > 180 && red < 140 && blue - red > 80 && green < 200,
  );
}

/** Pixels in the snap-marker colour (amber), including antialiased ones. */
export function markerPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) =>
      red > 150 && green > red * 0.5 && green < red * 0.85 && blue < red * 0.5,
  );
}

/**
 * Pixels in the warning red shared by the hot grip and by geometry an edit
 * would remove, including antialiased ones.
 */
export function hotGripPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => red > 150 && green < red * 0.6 && blue < red * 0.6,
  );
}

/** Pixels in the faint colours of the grid: darker than any geometry, lighter than the background. */
export function gridPixels(canvas: Locator): Promise<number> {
  return countPixels(canvas, ([red, , blue]) => blue >= 28 && blue <= 70 && red <= 40);
}

/** Pixels in the polar-tracking colour (teal), including antialiased ones. */
export function trackingPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => green > 60 && blue > 55 && red < green * 0.6 && blue > green * 0.7,
  );
}

/** Moves the pointer to a world position, in a view that has not been panned or zoomed. */
export async function hoverWorld(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  await page.mouse.move(center.x + x, center.y - y);
  await settle(page);
}

/** Clicks a world position, in a view that has not been panned or zoomed. */
export async function clickWorld(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  await page.mouse.click(center.x + x, center.y - y);
  await settle(page);
}

/** Absolute path of a DXF fixture under `test-data/dxf/`. */
export function fixturePath(relative: string): string {
  return fileURLToPath(new URL(`../../test-data/dxf/${relative}`, import.meta.url));
}

/** Absolute path of a DWG fixture under `test-data/dwg/`. */
export function dwgFixturePath(relative: string): string {
  return fileURLToPath(new URL(`../../test-data/dwg/${relative}`, import.meta.url));
}

/** Runs `action`, which must trigger a download, and returns the downloaded bytes and file name. */
export async function downloadBytesFrom(
  page: Page,
  action: () => Promise<void>,
): Promise<{ name: string; bytes: Buffer }> {
  const pending = page.waitForEvent("download");
  await action();
  const download = await pending;
  return { name: download.suggestedFilename(), bytes: await readFile(await download.path()) };
}

/** Runs `action`, which must trigger a download, and returns the downloaded text and file name. */
export async function downloadFrom(
  page: Page,
  action: () => Promise<void>,
): Promise<{ name: string; text: string }> {
  const pending = page.waitForEvent("download");
  await action();
  const download = await pending;
  return {
    name: download.suggestedFilename(),
    text: await readFile(await download.path(), "utf8"),
  };
}

/** Runs `action`, which must open a file picker, and chooses `file` in it. */
export async function chooseFile(
  page: Page,
  action: () => Promise<void>,
  file: string | { name: string; mimeType: string; buffer: Buffer },
): Promise<void> {
  const pending = page.waitForEvent("filechooser");
  await action();
  await (await pending).setFiles(file);
  await settle(page);
}

/** Records every request the page makes from now on. */
export function recordRequests(page: Page): { url: string; method: string }[] {
  const requests: { url: string; method: string }[] = [];
  page.on("request", (request) => requests.push({ url: request.url(), method: request.method() }));
  return requests;
}

// ── reading a DXF export back ──

/** One DXF record: its type (the value of its group 0) and the groups that follow. */
export interface DxfRecord {
  type: string;
  groups: [code: number, value: string][];
}

/** Every record of a file, in order; SECTION, TABLE, and their ends included. */
export function dxfRecords(text: string): DxfRecord[] {
  const lines = text.split("\n");
  const records: DxfRecord[] = [];
  for (let at = 0; at + 1 < lines.length; at += 2) {
    const code = Number(lines[at]);
    const value = lines[at + 1] ?? "";
    if (code === 0) records.push({ type: value, groups: [] });
    else records.at(-1)?.groups.push([code, value]);
  }
  return records;
}

/** The records inside one section, its SECTION and ENDSEC left out. */
export function dxfSection(text: string, name: string): DxfRecord[] {
  const records = dxfRecords(text);
  const start = records.findIndex(
    (record) => record.type === "SECTION" && group(record, 2) === name,
  );
  if (start < 0) return [];
  const end = records.findIndex((record, index) => index > start && record.type === "ENDSEC");
  return records.slice(start + 1, end < 0 ? undefined : end);
}

/** The first value of a group in a record. */
export function group(record: DxfRecord | undefined, code: number): string | undefined {
  return record?.groups.find(([c]) => c === code)?.[1];
}

/** Every value of a group in a record, in order. */
export function groupsOf(record: DxfRecord | undefined, code: number): string[] {
  return (record?.groups ?? []).filter(([c]) => c === code).map(([, value]) => value);
}

/** A group's value as a number. */
export function num(record: DxfRecord | undefined, code: number): number {
  return Number(group(record, code));
}

/** The entities of the model (or of another section), of one type when given. */
export function entities(text: string, type?: string, section = "ENTITIES"): DxfRecord[] {
  return dxfSection(text, section).filter((record) => !type || record.type === type);
}

/** Every LINE as [x0, y0, x1, y1], in file order. */
export function lines(text: string, section = "ENTITIES"): number[][] {
  return entities(text, "LINE", section).map((line) => [
    num(line, 10),
    num(line, 20),
    num(line, 11),
    num(line, 21),
  ]);
}

/** Every CIRCLE as [centre x, centre y, radius]. */
export function circles(text: string): number[][] {
  return entities(text, "CIRCLE").map((circle) => [
    num(circle, 10),
    num(circle, 20),
    num(circle, 40),
  ]);
}

/** Every ARC as [centre x, centre y, radius, start°, end°]. */
export function arcs(text: string): number[][] {
  return entities(text, "ARC").map((arc) => [
    num(arc, 10),
    num(arc, 20),
    num(arc, 40),
    num(arc, 50),
    num(arc, 51),
  ]);
}

export interface Polyline {
  closed: boolean;
  /** x, y, and the bulge of the segment that starts there (0 when straight). */
  vertices: [number, number, number][];
}

/** Every LWPOLYLINE: a bulge belongs to the vertex before it. */
export function polylines(text: string): Polyline[] {
  return entities(text, "LWPOLYLINE").map((polyline) => {
    const vertices: [number, number, number][] = [];
    for (const [code, value] of polyline.groups) {
      const last = vertices[vertices.length - 1];
      if (code === 10) vertices.push([Number(value), NaN, 0]);
      else if (code === 20 && last) last[1] = Number(value);
      else if (code === 42 && last) last[2] = Number(value);
    }
    return { closed: (num(polyline, 70) & 1) === 1, vertices };
  });
}

/** The row of a table (LAYER, LTYPE, STYLE, …) with that name. */
export function tableRow(text: string, table: string, name: string): DxfRecord | undefined {
  return dxfSection(text, "TABLES").find(
    (record) => record.type === table && group(record, 2) === name,
  );
}

/** The names of the drawing's block definitions, model and paper space left out. */
export function blocks(text: string): string[] {
  return dxfSection(text, "BLOCKS")
    .filter((record) => record.type === "BLOCK")
    .map((record) => group(record, 2) ?? "")
    .filter((name) => !name.startsWith("*"));
}

/** The ENTITIES section of a DXF export as text: where group 42 is a bulge and nothing else. */
export function entitiesSection(text: string): string {
  const start = text.indexOf("  2\nENTITIES\n");
  const end = text.indexOf("  0\nENDSEC\n", start);
  return text.slice(start, end);
}

/**
 * The view's mapping, read off the status bar: the world coordinates under
 * two screen points give the scale and where the centre is.
 */
export async function mapping(
  page: Page,
  canvas: Locator,
): Promise<(x: number, y: number) => { x: number; y: number }> {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  const readout = page.getByTestId("coordinates");
  const at = async (sx: number, sy: number) => {
    const stale = await readout.textContent();
    await page.mouse.move(box.x + sx, box.y + sy);
    await settle(page);
    // The readout follows the pointer a frame later.
    await expect.poll(() => readout.textContent()).not.toBe(stale);
    const [x, y] = (await readout.textContent())?.split(",").map(Number) ?? [];
    return { x: x ?? 0, y: y ?? 0 };
  };
  const a = await at(100, 100);
  const b = await at(300, 100);
  const scale = 200 / (b.x - a.x);
  return (x, y) => ({ x: box.x + 100 + (x - a.x) * scale, y: box.y + 100 - (y - a.y) * scale });
}

/**
 * The M8 scenario file: the layouts fixture with an ellipse, a spline, and
 * a block with an attribute added, as another program would write them.
 */
export function scenarioDxf(): string {
  const base = readFileSync(fixturePath("valid/layouts.dxf"), "utf8");
  const record = (lines: string[]) => lines.join("\n") + "\n";
  const block = record([
    "  0",
    "BLOCK",
    "  8",
    "0",
    "  2",
    "TAG",
    " 70",
    "2",
    " 10",
    "0.0",
    " 20",
    "0.0",
    " 30",
    "0.0",
    "  3",
    "TAG",
    "  0",
    "LWPOLYLINE",
    "  8",
    "0",
    " 90",
    "4",
    " 70",
    "1",
    " 10",
    "0.0",
    " 20",
    "0.0",
    " 10",
    "30.0",
    " 20",
    "0.0",
    " 10",
    "30.0",
    " 20",
    "10.0",
    " 10",
    "0.0",
    " 20",
    "10.0",
    "  0",
    "ATTDEF",
    "  8",
    "0",
    " 10",
    "2.0",
    " 20",
    "2.0",
    " 30",
    "0.0",
    " 40",
    "5.0",
    "  1",
    "",
    "  3",
    "Room",
    "  2",
    "ROOM",
    " 70",
    "0",
    "  0",
    "ENDBLK",
  ]);
  const drawn = record([
    "  0",
    "ELLIPSE",
    "  8",
    "0",
    " 10",
    "200.0",
    " 20",
    "50.0",
    " 30",
    "0.0",
    " 11",
    "20.0",
    " 21",
    "0.0",
    " 31",
    "0.0",
    " 40",
    "0.5",
    " 41",
    "0.0",
    " 42",
    "6.283185307179586",
    "  0",
    "SPLINE",
    "  8",
    "0",
    " 70",
    "8",
    " 71",
    "3",
    " 72",
    "0",
    " 73",
    "0",
    " 74",
    "4",
    " 11",
    "0.0",
    " 21",
    "100.0",
    " 31",
    "0.0",
    " 11",
    "30.0",
    " 21",
    "160.0",
    " 31",
    "0.0",
    " 11",
    "70.0",
    " 21",
    "160.0",
    " 31",
    "0.0",
    " 11",
    "100.0",
    " 21",
    "100.0",
    " 31",
    "0.0",
    "  0",
    "INSERT",
    "  8",
    "0",
    " 66",
    "1",
    "  2",
    "TAG",
    " 10",
    "0.0",
    " 20",
    "-60.0",
    " 30",
    "0.0",
    "  0",
    "ATTRIB",
    "  8",
    "0",
    " 10",
    "2.0",
    " 20",
    "-58.0",
    " 30",
    "0.0",
    " 40",
    "5.0",
    "  1",
    "101",
    "  2",
    "ROOM",
    " 70",
    "0",
    "  0",
    "SEQEND",
  ]);
  const endsec = "  0\nENDSEC\n";
  const blocks = base.indexOf("  2\nBLOCKS\n");
  const blocksEnd = base.indexOf(endsec, blocks);
  let text = base.slice(0, blocksEnd) + block + base.slice(blocksEnd);
  const ents = text.indexOf("  2\nENTITIES\n");
  const entsEnd = text.indexOf(endsec, ents);
  text = text.slice(0, entsEnd) + drawn + text.slice(entsEnd);
  return text;
}
