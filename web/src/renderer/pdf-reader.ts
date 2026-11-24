// PDF pages as pictures (ADR-0042 §3, #528): pdf.js renders the page the
// user picks, over white, its long side the pixels the host asks for. pdf.js
// and its worker are a chunk of their own, fetched the first time a PDF is
// chosen; a page is a picture here — a vector PDF's lines are not read as
// geometry. Here because pdf.js draws on a canvas, a graphics context, which
// only the renderer makes (DESIGN §12.1).

import type { PdfDocument, PdfReader } from "../app/pictures";
import { pictureOf } from "./picture-decoder";

type PdfJs = typeof import("pdfjs-dist");

let loading: Promise<PdfJs> | undefined;

/**
 * pdf.js and its worker, fetched once. The worker is bundled by Vite as a
 * plain `.js` file, so any static host serves it as a script, and handed
 * to pdf.js as a port.
 */
function pdfjs(): Promise<PdfJs> {
  loading ??= Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?worker"),
  ]).then(([pdf, worker]) => {
    pdf.GlobalWorkerOptions.workerPort = new worker.default({ name: "pdf.js" });
    return pdf;
  });
  return loading;
}

/**
 * Where pdf.js finds its decoders of scanned images — JBIG2 and JPEG 2000 —
 * and of colour profiles: `pdfjs/` beside the page, where the build copies
 * them (vite.config.ts); the package itself in development. Absolute, as the
 * worker that fetches them is elsewhere.
 */
function decodersUrl(): string {
  const where = import.meta.env.DEV ? "/node_modules/pdfjs-dist/wasm/" : "pdfjs/";
  return new URL(where, document.baseURI).href;
}

/** pdf.js in the browser: a document opened from its bytes, a page drawn at a size. */
export const browserPdfReader: PdfReader = {
  async open(bytes) {
    const pdf = await pdfjs();
    let file;
    try {
      file = await pdf.getDocument({
        data: new Uint8Array(bytes),
        // A page is a picture: no forms drawn as HTML.
        enableXfa: false,
        wasmUrl: decodersUrl(),
      }).promise;
    } catch (error) {
      throw new Error(
        error instanceof Error && error.name === "PasswordException"
          ? "the PDF is protected by a password"
          : "the browser cannot read it as a PDF",
        { cause: error },
      );
    }
    const opened = file;
    const document: PdfDocument = {
      pages: opened.numPages,
      async page(number, side) {
        const page = await opened.getPage(number);
        const natural = page.getViewport({ scale: 1 });
        const viewport = page.getViewport({
          scale: side / Math.max(natural.width, natural.height),
        });
        const canvas = window.document.createElement("canvas");
        canvas.width = Math.max(1, Math.round(viewport.width));
        canvas.height = Math.max(1, Math.round(viewport.height));
        await page.render({ canvas, viewport, background: "#ffffff" }).promise;
        page.cleanup();
        return pictureOf(canvas, canvas.width, canvas.height, () => {
          canvas.width = 0;
          canvas.height = 0;
        });
      },
      close() {
        void opened.loadingTask.destroy();
      },
    };
    return document;
  },
};
