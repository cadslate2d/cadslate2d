// Binds one canvas to the app: DOM events go in as semantic engine calls and
// frames are drawn on demand. This module interprets nothing — every decision
// is made in Rust.
import { CadEngine, Dirty, Modifier, ToggleBit } from "../cad/engine";
import { documentPalette } from "../renderer/palette";
import { renderFrame } from "../renderer/render-frame";
import { WebGlRenderer } from "../renderer/webgl/webgl-renderer";
import type { CadApp } from "./cad-app";
import { publishDebugHandle } from "./debug";

/** Approximate pixel height of one wheel "line" (DOM_DELTA_LINE). */
const WHEEL_LINE_PIXELS = 16;

interface ModifierKeys {
  readonly shiftKey: boolean;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly altKey: boolean;
}

export function modifierBits(event: ModifierKeys): number {
  return (
    (event.shiftKey ? Modifier.Shift : 0) |
    (event.ctrlKey || event.metaKey ? Modifier.Ctrl : 0) |
    (event.altKey ? Modifier.Alt : 0)
  );
}

/** Wheel delta in CSS pixels, whatever unit the device reports. */
export function wheelPixels(
  event: Pick<WheelEvent, "deltaY" | "deltaMode">,
  pageHeight: number,
): number {
  switch (event.deltaMode) {
    case 1: // DOM_DELTA_LINE
      return event.deltaY * WHEEL_LINE_PIXELS;
    case 2: // DOM_DELTA_PAGE
      return event.deltaY * pageHeight;
    default:
      return event.deltaY;
  }
}

export class CanvasController {
  readonly #canvas: HTMLCanvasElement;
  readonly #app: CadApp;
  readonly #renderer: WebGlRenderer;
  readonly #resizeObserver: ResizeObserver;
  readonly #abort = new AbortController();
  readonly #detachRenderer: () => void;

  /** Requires WebGL2; check `webGl2Available()` before constructing. */
  constructor(canvas: HTMLCanvasElement, app: CadApp) {
    this.#canvas = canvas;
    this.#app = app;
    this.#renderer = new WebGlRenderer(canvas, documentPalette());
    this.#renderer.onContextRestored = () => app.loop.invalidate(Dirty.Scene);
    publishDebugHandle({
      renderer: this.#renderer.stats,
      // Loaded on demand so the probe is not part of the normal startup path.
      probeTransport: async () => {
        const { probeTransport } = await import("../renderer/webgl/transport-probe");
        return probeTransport(app.engine);
      },
      probeOpen: async (text) => {
        const { probeOpen } = await import("../renderer/webgl/transport-probe");
        return probeOpen(() => new CadEngine(), text);
      },
      probeCurves: async (arcCount) => {
        const { probeCurves } = await import("../renderer/webgl/curve-probe");
        return probeCurves(arcCount);
      },
      probeLinetypes: async (segments) => {
        const { probeLinetypes } = await import("../renderer/webgl/linetype-probe");
        return probeLinetypes(segments);
      },
      linetypePictures: async () => {
        const { linetypePictures } = await import("../renderer/webgl/linetype-probe");
        return linetypePictures();
      },
      probeStyles: async (segments, styles, order) => {
        const { probeStyles } = await import("../renderer/webgl/style-probe");
        return probeStyles(segments, styles, order);
      },
    });

    const options = { signal: this.#abort.signal };
    canvas.addEventListener("pointermove", this.#onPointerMove, options);
    canvas.addEventListener("pointerdown", this.#onPointerDown, options);
    canvas.addEventListener("pointerup", this.#onPointerUp, options);
    canvas.addEventListener("pointerleave", this.#onPointerLeave, options);
    canvas.addEventListener("wheel", this.#onWheel, { ...options, passive: false });
    canvas.addEventListener("contextmenu", (event) => event.preventDefault(), options);
    // Device pixel ratio changes (browser zoom, moving between monitors) arrive as window resizes.
    window.addEventListener("resize", this.#syncSize, options);

    this.#resizeObserver = new ResizeObserver(this.#syncSize);
    this.#resizeObserver.observe(canvas);
    this.#syncSize();
    this.#detachRenderer = app.setRenderer(() => {
      // LWT is session state in Rust; how it looks is the renderer's business.
      this.#renderer.showLineweights = (app.ui.getSnapshot().toggles & ToggleBit.Lineweight) !== 0;
      renderFrame(app.engine, this.#renderer);
    });
  }

  dispose(): void {
    this.#detachRenderer();
    this.#abort.abort();
    this.#resizeObserver.disconnect();
    this.#renderer.dispose();
    publishDebugHandle(undefined);
  }

  readonly #syncSize = (): void => {
    const { width, height } = this.#canvas.getBoundingClientRect();
    this.#renderer.resize(width, height, window.devicePixelRatio);
    this.#app.loop.invalidate(this.#app.engine.resize(width, height));
  };

  #position(event: MouseEvent): { x: number; y: number } {
    const bounds = this.#canvas.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  }

  readonly #onPointerMove = (event: PointerEvent): void => {
    this.#app.loop.queuePointerMove({ ...this.#position(event), modifiers: modifierBits(event) });
  };

  readonly #onPointerLeave = (): void => {
    this.#app.loop.flushPointerMove();
    this.#app.loop.invalidate(this.#app.engine.pointerLeave());
  };

  readonly #onPointerDown = (event: PointerEvent): void => {
    // Keep receiving moves when a drag leaves the canvas; stop middle-click autoscroll
    // and keep keyboard focus on the command line.
    this.#canvas.setPointerCapture(event.pointerId);
    event.preventDefault();
    this.#app.loop.flushPointerMove();
    const { x, y } = this.#position(event);
    this.#app.loop.invalidate(
      this.#app.engine.pointerDown(x, y, event.button, modifierBits(event)),
    );
  };

  readonly #onPointerUp = (event: PointerEvent): void => {
    this.#app.loop.flushPointerMove();
    const { x, y } = this.#position(event);
    this.#app.loop.invalidate(this.#app.engine.pointerUp(x, y, event.button, modifierBits(event)));
  };

  readonly #onWheel = (event: WheelEvent): void => {
    event.preventDefault(); // the canvas zooms; the page must not scroll
    this.#app.loop.flushPointerMove();
    const { x, y } = this.#position(event);
    const delta = wheelPixels(event, this.#canvas.clientHeight);
    this.#app.loop.invalidate(this.#app.engine.wheel(x, y, delta, modifierBits(event)));
  };
}
