// The typed wrapper around the WASM boundary, and the only module that imports
// the generated package (ADR-0002, ADR-0004).
//
// The boundary is coarse on purpose: one call per browser event, each
// returning dirty flags; frame data is then pulled in bulk. Nothing here
// exposes entities, and nothing here interprets CAD state.
import init, { CadSession, core_version, type InitInput } from "./pkg/cad_wasm";

/** Bits of the mask returned by every input call. Pull only what is flagged. */
export const Dirty = {
  /** The committed scene revision changed. */
  Scene: 1,
  /** Transient/overlay geometry changed. */
  Overlay: 1 << 1,
  /** Pan, zoom, or canvas size changed. */
  View: 1 << 2,
  /** Prompt, log, status, or toggles changed. */
  Ui: 1 << 3,
} as const;

/** Keyboard modifier bits passed with pointer and key events. */
export const Modifier = { Shift: 1, Ctrl: 1 << 1, Alt: 1 << 2 } as const;

/** What a batch of geometry is; the renderer maps each role to a themed style. */
export const StyleRole = { Entity: 0, Selection: 1, Preview: 2, Crosshair: 3 } as const;
export type StyleRole = (typeof StyleRole)[keyof typeof StyleRole];

export const FLOATS_PER_SEGMENT = 4;
export const WORDS_PER_BATCH = 3;

/**
 * Packed geometry for one render lifetime.
 *
 * Both arrays are views into WASM memory. They are valid only inside the
 * callback that receives them: copy or upload the data there and never keep
 * a reference, because the next call into WASM may detach them.
 */
export interface FrameBuffers {
  /** `x0, y0, x1, y1` per segment instance, relative to the frame origin. */
  readonly segments: Float32Array;
  /** `style, first, count` per batch. */
  readonly batches: Uint32Array;
}

/** `[scaleX, scaleY, translateX, translateY]`: `clip = position * scale + translate`. */
export type ClipTransform = Float32Array;

let ready: Promise<void> | undefined;

/**
 * Instantiates the WASM module once; later calls share the same promise.
 * `source` overrides where the module is loaded from (used by tests).
 */
export function loadEngine(source?: InitInput): Promise<void> {
  ready ??= init(source === undefined ? undefined : { module_or_path: source }).then(
    () => undefined,
  );
  return ready;
}

/** Version of the Rust core. Call only after `loadEngine()` resolves. */
export function coreVersion(): string {
  return core_version();
}

/** One open drawing. Create only after `loadEngine()` resolves. */
export class CadEngine {
  readonly #session = new CadSession();

  /** Releases the WASM-side session. The engine must not be used afterwards. */
  dispose(): void {
    this.#session.free();
  }

  // ── input: each call returns a Dirty bitmask ──

  /** Canvas size in CSS pixels. */
  resize(width: number, height: number): number {
    return this.#session.resize(width, height);
  }

  pointerMove(x: number, y: number, modifiers: number): number {
    return this.#session.pointer_move(x, y, modifiers);
  }

  pointerDown(x: number, y: number, button: number, modifiers: number): number {
    return this.#session.pointer_down(x, y, button, modifiers);
  }

  pointerUp(x: number, y: number, button: number, modifiers: number): number {
    return this.#session.pointer_up(x, y, button, modifiers);
  }

  /** `deltaY` in CSS pixels. */
  wheel(x: number, y: number, deltaY: number, modifiers: number): number {
    return this.#session.wheel(x, y, deltaY, modifiers);
  }

  // ── output: pulled in bulk ──

  /** Identifies the committed scene; re-upload committed buffers only when it changes. */
  get sceneRevision(): number {
    return this.#session.scene_revision();
  }

  /** Lends the committed scene's buffers to `consume` for the duration of the call. */
  readScene<T>(consume: (frame: FrameBuffers) => T): T {
    // Views are created inside this call and never escape it.
    return consume({
      segments: this.#session.scene_segments(),
      batches: this.#session.scene_batches(),
    });
  }

  /** Clip transform for the committed scene. An owned copy, safe to keep. */
  sceneTransform(): ClipTransform {
    return this.#session.scene_transform();
  }
}
