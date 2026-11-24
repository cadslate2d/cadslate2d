// The browser's picture decoder (ADR-0042 §3, #525): `createImageBitmap`,
// upright as the camera held it, drawn on a 2D canvas at the size the host
// asks for, over white, and read back as pixels or written as a JPEG. Here
// because a canvas is a graphics context, which only the renderer makes
// (DESIGN §12.1); the rules of what is made are the host's
// (`app/pictures.ts`).

import type { PictureDecoder } from "../app/pictures";

/** How good a JPEG the host writes, 0 to 1. */
const JPEG_QUALITY = 0.9;

/** A canvas `width` × `height` holding `source` drawn over white. */
function drawn(
  source: ImageBitmap,
  width: number,
  height: number,
): OffscreenCanvas | HTMLCanvasElement {
  const canvas =
    typeof OffscreenCanvas === "function"
      ? new OffscreenCanvas(width, height)
      : Object.assign(document.createElement("canvas"), { width, height });
  const context = canvas.getContext("2d") as
    OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D | null;
  if (!context) throw new Error("the browser would not draw the picture");
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = "high";
  context.drawImage(source, 0, 0, width, height);
  return canvas;
}

async function jpegOf(canvas: OffscreenCanvas | HTMLCanvasElement): Promise<Uint8Array> {
  const blob =
    "convertToBlob" in canvas
      ? await canvas.convertToBlob({ type: "image/jpeg", quality: JPEG_QUALITY })
      : await new Promise<Blob | null>((resolve) =>
          canvas.toBlob(resolve, "image/jpeg", JPEG_QUALITY),
        );
  if (!blob) throw new Error("the browser would not write the picture");
  return new Uint8Array(await blob.arrayBuffer());
}

/** The browser's decoder: `createImageBitmap`, upright as the camera held it, drawn on a canvas. */
export const browserPictureDecoder: PictureDecoder = {
  async decode(bytes) {
    let bitmap: ImageBitmap;
    try {
      bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)]), {
        imageOrientation: "from-image",
      });
    } catch {
      throw new Error("the browser cannot read it as a picture");
    }
    return {
      width: bitmap.width,
      height: bitmap.height,
      pixels(width, height) {
        const canvas = drawn(bitmap, width, height);
        const context = canvas.getContext("2d") as
          OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
        return context.getImageData(0, 0, width, height).data;
      },
      jpeg(width, height) {
        return jpegOf(drawn(bitmap, width, height));
      },
      close() {
        bitmap.close();
      },
    };
  },
};
