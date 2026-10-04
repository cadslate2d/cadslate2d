// Wires one canvas to the engine and the renderer: DOM events go in as
// semantic engine calls, dirty flags come back, and frames are drawn on
// demand. This module interprets nothing — every decision is made in Rust.
import { Dirty, Modifier, type CadEngine } from "../cad/engine";
import { documentPalette } from "../renderer/palette";
import { renderFrame } from "../renderer/render-frame";
import { WebGlRenderer } from "../renderer/webgl/webgl-renderer";
import { publishDebugHandle } from "./debug";
import { FrameLoop } from "./frame-loop";

const RENDER_FLAGS = Dirty.Scene | Dirty.Overlay | Dirty.View;
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
  readonly #engine: CadEngine;
  readonly #renderer: WebGlRenderer;
  readonly #loop: FrameLoop;
  readonly #resizeObserver: ResizeObserver;
  readonly #abort = new AbortController();

  /** Requires WebGL2; check `webGl2Available()` before constructing. */
  constructor(canvas: HTMLCanvasElement, engine: CadEngine) {
    this.#canvas = canvas;
    this.#engine = engine;
    this.#renderer = new WebGlRenderer(canvas, documentPalette());
    this.#loop = new FrameLoop(
      (sample) => engine.pointerMove(sample.x, sample.y, sample.modifiers),
      (dirty) => this.#onFrame(dirty),
      (callback) => requestAnimationFrame(callback),
    );
    this.#renderer.onContextRestored = () => this.#loop.invalidate(Dirty.Scene);
    publishDebugHandle({ renderer: this.#renderer.stats });

    const options = { signal: this.#abort.signal };
    canvas.addEventListener("pointermove", this.#onPointerMove, options);
    canvas.addEventListener("pointerdown", this.#onPointerDown, options);
    canvas.addEventListener("pointerup", this.#onPointerUp, options);
    canvas.addEventListener("wheel", this.#onWheel, { ...options, passive: false });
    canvas.addEventListener("contextmenu", (event) => event.preventDefault(), options);
    // Device pixel ratio changes (browser zoom, moving between monitors) arrive as window resizes.
    window.addEventListener("resize", this.#syncSize, options);

    this.#resizeObserver = new ResizeObserver(this.#syncSize);
    this.#resizeObserver.observe(canvas);
    this.#syncSize();
  }

  dispose(): void {
    this.#abort.abort();
    this.#resizeObserver.disconnect();
    this.#renderer.dispose();
    publishDebugHandle(undefined);
  }

  #onFrame(dirty: number): void {
    if (dirty & RENDER_FLAGS) renderFrame(this.#engine, this.#renderer);
  }

  readonly #syncSize = (): void => {
    const { width, height } = this.#canvas.getBoundingClientRect();
    this.#renderer.resize(width, height, window.devicePixelRatio);
    this.#loop.invalidate(this.#engine.resize(width, height));
  };

  #position(event: MouseEvent): { x: number; y: number } {
    const bounds = this.#canvas.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  }

  readonly #onPointerMove = (event: PointerEvent): void => {
    this.#loop.queuePointerMove({ ...this.#position(event), modifiers: modifierBits(event) });
  };

  readonly #onPointerDown = (event: PointerEvent): void => {
    // Keep receiving moves when a drag leaves the canvas; stop middle-click autoscroll.
    this.#canvas.setPointerCapture(event.pointerId);
    if (event.button === 1) event.preventDefault();
    this.#loop.flushPointerMove();
    const { x, y } = this.#position(event);
    this.#loop.invalidate(this.#engine.pointerDown(x, y, event.button, modifierBits(event)));
  };

  readonly #onPointerUp = (event: PointerEvent): void => {
    this.#loop.flushPointerMove();
    const { x, y } = this.#position(event);
    this.#loop.invalidate(this.#engine.pointerUp(x, y, event.button, modifierBits(event)));
  };

  readonly #onWheel = (event: WheelEvent): void => {
    event.preventDefault(); // the canvas zooms; the page must not scroll
    this.#loop.flushPointerMove();
    const { x, y } = this.#position(event);
    const delta = wheelPixels(event, this.#canvas.clientHeight);
    this.#loop.invalidate(this.#engine.wheel(x, y, delta, modifierBits(event)));
  };
}
