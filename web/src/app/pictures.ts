// Pictures for the drawing (ADR-0042 §3, #525): the host makes every image
// the engine holds. The browser decodes what the user picks — a PNG, a
// JPEG, a phone's photo, whatever it can read — upright as its camera held
// it; the host draws it at most `side` pixels on its long side over white,
// and encodes it again: a JPEG for a photograph, an opaque 8-bit PNG for a
// picture that came lossless (PNG, GIF, BMP), as line art does. So the
// engine never decodes, a plot embeds the bytes as they are (§5), and
// nothing of the file but its pixels — no location, no camera — is kept.

import type { PickedFile } from "./host-services";

/** The longest side a picture is made at, unless the settings say otherwise. */
export const PICTURE_SIDE = 2048;
/** The settings' bounds for it: the engine holds at most 8,192 pixels each way. */
export const PICTURE_SIDE_MIN = 256;
export const PICTURE_SIDE_MAX = 8192;

/** What the file picker offers: any picture the browser may read, and PDFs (#528). */
export const PICTURE_ACCEPT =
  "image/*,.png,.jpg,.jpeg,.webp,.gif,.bmp,.heic,.avif,application/pdf,.pdf";

/** A picture the browser decoded: its size, and it drawn at another. */
export interface DecodedPicture {
  readonly width: number;
  readonly height: number;
  /** Its pixels at `width` × `height`, over white: RGBA, four bytes a pixel, row by row from the top. */
  pixels(width: number, height: number): Uint8ClampedArray;
  /** It at `width` × `height`, over white, as a JPEG. */
  jpeg(width: number, height: number): Promise<Uint8Array>;
  /** Lets go of what the decoder holds. */
  close(): void;
}

/** The browser's decoder, behind an interface so the host's rules are tested without one. */
export interface PictureDecoder {
  /** Decodes `bytes`; rejects when the browser cannot read them as a picture. */
  decode(bytes: Uint8Array): Promise<DecodedPicture>;
}

/**
 * A PDF the browser opened (#528): how many pages it has, and a page drawn
 * over white with its long side the pixels asked for.
 */
export interface PdfDocument {
  readonly pages: number;
  /** Page `number`, from 1, its long side `side` pixels. */
  page(number: number, side: number): Promise<DecodedPicture>;
  /** Lets go of the document. */
  close(): void;
}

/** pdf.js, behind an interface so the host's rules are tested without it. */
export interface PdfReader {
  /** Opens the PDF `bytes` hold; rejects, saying why, when the browser cannot. */
  open(bytes: Uint8Array): Promise<PdfDocument>;
}

/** Whether `bytes` are a PDF: its header in the first kilobyte, as readers allow. */
export function isPdf(bytes: Uint8Array): boolean {
  const head = new TextDecoder("latin1").decode(bytes.subarray(0, 1024));
  return head.includes("%PDF-");
}

/**
 * The name a PDF's page is attached under: the file's, and the page's
 * number when there is more than one — `plans page 2.pdf`, which the engine
 * names `plans page 2`.
 */
export function pageFileName(fileName: string, page: number, pages: number): string {
  if (pages === 1) return fileName;
  const stem = fileName.replace(/\.pdf$/i, "");
  return `${stem} page ${page}.pdf`;
}

/**
 * Page `page` of `pdf` as a picture for the engine (ADR-0042 §3): drawn at
 * `side` pixels on its long side — a page is drawn at the resolution the
 * bound allows, enlarged or not — and written as an opaque PNG, as a page
 * is most often a drawing, whose lines a JPEG would blur.
 */
export async function makePagePicture(
  pdf: PdfDocument,
  fileName: string,
  page: number,
  side: number,
): Promise<MadePicture> {
  const decoded = await pdf.page(page, side);
  try {
    const { width, height } = decoded;
    return {
      name: pageFileName(fileName, page, pdf.pages),
      bytes: await encodePng(decoded.pixels(width, height), width, height),
    };
  } finally {
    decoded.close();
  }
}

/** A picture made for the engine: its file's name and its bytes, a JPEG or an opaque PNG. */
export interface MadePicture {
  readonly name: string;
  readonly bytes: Uint8Array;
}

/** Whether `bytes` are a lossless picture — a PNG, a GIF, a BMP — which is kept lossless. */
export function isLossless(bytes: Uint8Array): boolean {
  const starts = (...signature: number[]) => signature.every((byte, at) => bytes[at] === byte);
  return (
    starts(0x89, 0x50, 0x4e, 0x47) || // PNG
    starts(0x47, 0x49, 0x46, 0x38) || // GIF8
    starts(0x42, 0x4d) // BM
  );
}

/** The size a picture is made at: at most `side` on its long side, its proportions kept, never enlarged. */
export function madeSize(width: number, height: number, side: number): [number, number] {
  const scale = Math.min(1, side / Math.max(width, height));
  return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

/** Makes the picture `file` holds as ADR-0042 §3 says, decoded by `decoder`, at most `side` on its long side. */
export async function makePicture(
  file: PickedFile,
  decoder: PictureDecoder,
  side: number,
): Promise<MadePicture> {
  const decoded = await decoder.decode(file.bytes);
  try {
    const [width, height] = madeSize(decoded.width, decoded.height, side);
    const bytes = isLossless(file.bytes)
      ? await encodePng(decoded.pixels(width, height), width, height)
      : await decoded.jpeg(width, height);
    return { name: file.name, bytes };
  } finally {
    decoded.close();
  }
}

// ── PNG ──

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (CRC_TABLE[(crc ^ byte) & 0xff] ?? 0) ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** One chunk: its length, its type, its data, and the CRC of type and data. */
function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i += 1) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/** The Paeth predictor of PNG's filter 4. */
function paeth(left: number, up: number, upLeft: number): number {
  const p = left + up - upLeft;
  const toLeft = Math.abs(p - left);
  const toUp = Math.abs(p - up);
  const toUpLeft = Math.abs(p - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) return left;
  return toUp <= toUpLeft ? up : upLeft;
}

/**
 * The rows of samples, each led by the filter that leaves the smallest
 * differences — PNG's usual choice — as a reader undoes them.
 */
function filtered(samples: Uint8Array, rowLength: number, channels: number): Uint8Array {
  const rows = samples.length / rowLength;
  const out = new Uint8Array(rows * (rowLength + 1));
  const candidate = new Uint8Array(rowLength);
  const best = new Uint8Array(rowLength);
  for (let y = 0; y < rows; y += 1) {
    const row = samples.subarray(y * rowLength, (y + 1) * rowLength);
    const above = y > 0 ? samples.subarray((y - 1) * rowLength, y * rowLength) : undefined;
    let bestFilter = 0;
    let bestCost = Infinity;
    for (let filter = 0; filter < 5; filter += 1) {
      let cost = 0;
      for (let x = 0; x < rowLength; x += 1) {
        const left = x >= channels ? (row[x - channels] ?? 0) : 0;
        const up = above?.[x] ?? 0;
        const upLeft = x >= channels ? (above?.[x - channels] ?? 0) : 0;
        const predicted =
          filter === 0
            ? 0
            : filter === 1
              ? left
              : filter === 2
                ? up
                : filter === 3
                  ? (left + up) >> 1
                  : paeth(left, up, upLeft);
        const value = ((row[x] ?? 0) - predicted) & 0xff;
        candidate[x] = value;
        cost += value < 128 ? value : 256 - value;
      }
      if (cost < bestCost) {
        bestCost = cost;
        bestFilter = filter;
        best.set(candidate);
      }
    }
    const at = y * (rowLength + 1);
    out[at] = bestFilter;
    out.set(best, at + 1);
  }
  return out;
}

/** `bytes` compressed as a zlib stream, by the browser's own deflate. */
async function deflate(bytes: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([new Uint8Array(bytes)])
    .stream()
    .pipeThrough(new CompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/**
 * An opaque 8-bit PNG of RGBA `pixels`, `width` × `height`: grey when every
 * pixel is, colour otherwise, its alpha dropped — the pixels are drawn
 * over white already. What the engine takes as a PNG (ADR-0042 §3).
 */
export async function encodePng(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
): Promise<Uint8Array> {
  const count = width * height;
  let grey = true;
  for (let i = 0; i < count && grey; i += 1) {
    const at = i * 4;
    grey = pixels[at] === pixels[at + 1] && pixels[at] === pixels[at + 2];
  }
  const channels = grey ? 1 : 3;
  const samples = new Uint8Array(count * channels);
  for (let i = 0; i < count; i += 1) {
    for (let c = 0; c < channels; c += 1) samples[i * channels + c] = pixels[i * 4 + c] ?? 0;
  }
  const header = new Uint8Array(13);
  const view = new DataView(header.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  // 8 bits a sample; grey (0) or colour (2); deflate; adaptive filters; not interlaced.
  header.set([8, grey ? 0 : 2, 0, 0, 0], 8);
  const data = await deflate(filtered(samples, width * channels, channels));
  const parts = [
    new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", data),
    chunk("IEND", new Uint8Array(0)),
  ];
  const out = new Uint8Array(parts.reduce((length, part) => length + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}
