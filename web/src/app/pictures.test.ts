// The host's pictures (ADR-0042 §3, #525): the size a picture is made at,
// which pictures stay lossless, and the PNG the host writes — read back by
// the browser's own inflate, its filters undone, the pixels as given.
import { describe, expect, test } from "vitest";
import { encodePng, isLossless, madeSize } from "./pictures";

/** A PNG's chunks, by type: the data of each, IDATs joined. */
function chunks(png: Uint8Array): Map<string, Uint8Array> {
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  const found = new Map<string, Uint8Array>();
  let at = 8;
  while (at < png.length) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(...png.subarray(at + 4, at + 8));
    const data = png.subarray(at + 8, at + 8 + length);
    const before = found.get(type);
    found.set(type, before ? new Uint8Array([...before, ...data]) : data);
    at += 12 + length;
  }
  return found;
}

async function inflate(bytes: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([new Uint8Array(bytes)])
    .stream()
    .pipeThrough(new DecompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** The samples of filtered rows, each filter undone as a PNG reader does. */
function unfiltered(data: Uint8Array, rowLength: number, channels: number): Uint8Array {
  const rows = data.length / (rowLength + 1);
  const out = new Uint8Array(rows * rowLength);
  for (let y = 0; y < rows; y += 1) {
    const filter = data[y * (rowLength + 1)] ?? 0;
    for (let x = 0; x < rowLength; x += 1) {
      const raw = data[y * (rowLength + 1) + 1 + x] ?? 0;
      const left = x >= channels ? (out[y * rowLength + x - channels] ?? 0) : 0;
      const up = y > 0 ? (out[(y - 1) * rowLength + x] ?? 0) : 0;
      const upLeft = x >= channels && y > 0 ? (out[(y - 1) * rowLength + x - channels] ?? 0) : 0;
      const p = left + up - upLeft;
      const paeth =
        Math.abs(p - left) <= Math.abs(p - up) && Math.abs(p - left) <= Math.abs(p - upLeft)
          ? left
          : Math.abs(p - up) <= Math.abs(p - upLeft)
            ? up
            : upLeft;
      const predicted = [0, left, up, (left + up) >> 1, paeth][filter] ?? 0;
      out[y * rowLength + x] = (raw + predicted) & 0xff;
    }
  }
  return out;
}

describe("pictures", () => {
  test("a picture is made at most the side on its long side, never enlarged", () => {
    expect(madeSize(4000, 3000, 2048)).toEqual([2048, 1536]);
    expect(madeSize(1000, 5000, 2048)).toEqual([410, 2048]);
    expect(madeSize(300, 200, 2048)).toEqual([300, 200]);
    expect(madeSize(10000, 1, 2048)).toEqual([2048, 1]);
  });

  test("a PNG, a GIF, or a BMP stays lossless; anything else is a photograph", () => {
    expect(isLossless(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBe(true);
    expect(isLossless(new TextEncoder().encode("GIF89a"))).toBe(true);
    expect(isLossless(new TextEncoder().encode("BM...."))).toBe(true);
    expect(isLossless(new Uint8Array([0xff, 0xd8, 0xff]))).toBe(false);
    expect(isLossless(new Uint8Array([]))).toBe(false);
  });

  test("the PNG written is opaque 8-bit colour, and reads back as the pixels given", async () => {
    const [width, height] = [37, 23];
    const pixels = new Uint8ClampedArray(width * height * 4);
    for (let i = 0; i < width * height; i += 1) {
      const x = i % width;
      const y = Math.floor(i / width);
      pixels.set([x * 6, y * 11, (x * y) % 256, i % 3 === 0 ? 0 : 255], i * 4);
    }
    const png = await encodePng(pixels, width, height);
    expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const found = chunks(png);
    const header = found.get("IHDR") ?? new Uint8Array();
    const view = new DataView(header.buffer, header.byteOffset);
    expect([view.getUint32(0), view.getUint32(4), ...header.subarray(8)]).toEqual([
      width,
      height,
      8,
      2,
      0,
      0,
      0,
    ]);
    expect(found.has("IEND")).toBe(true);
    const samples = unfiltered(await inflate(found.get("IDAT") ?? new Uint8Array()), width * 3, 3);
    // Alpha dropped: the pixels are drawn over white before they come here.
    const expected = new Uint8Array(width * height * 3);
    for (let i = 0; i < width * height; i += 1) {
      expected.set(pixels.subarray(i * 4, i * 4 + 3), i * 3);
    }
    expect(samples).toEqual(expected);
  });

  test("a picture whose every pixel is grey is written grey, one sample a pixel", async () => {
    const pixels = new Uint8ClampedArray(4 * 3 * 4);
    for (let i = 0; i < 12; i += 1) pixels.set([i * 20, i * 20, i * 20, 255], i * 4);
    const found = chunks(await encodePng(pixels, 4, 3));
    expect(found.get("IHDR")?.[9]).toBe(0);
    const samples = unfiltered(await inflate(found.get("IDAT") ?? new Uint8Array()), 4, 1);
    expect([...samples]).toEqual(Array.from({ length: 12 }, (_, i) => i * 20));
  });
});
