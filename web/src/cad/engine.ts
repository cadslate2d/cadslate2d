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

/** Non-text keys. Text, including Enter and Space-as-Enter, goes through `submitText`. */
export const KeyAction = { Escape: 0, Undo: 1, Redo: 2 } as const;
export type KeyAction = (typeof KeyAction)[keyof typeof KeyAction];

/**
 * Presentation state for the shell. It holds nothing from which drawing
 * geometry could be reconstructed (ADR-0002 §5).
 */
export interface UiState {
  /** The current prompt, e.g. `Command:` or `Specify next point:`. */
  readonly prompt: string;
  /** Canonical name of the active command, if any. */
  readonly command?: string;
  /** Total log lines ever written; fetch new ones with `logSince`. */
  readonly logTotal: number;
  /** World position under the cursor while the pointer is over the canvas. */
  readonly cursor?: readonly [number, number];
  readonly selectionCount: number;
  /** Whether Space submits the command line like Enter. */
  readonly spaceAccepts: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
}

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
let memory: WebAssembly.Memory | undefined;

/**
 * Instantiates the WASM module once; later calls share the same promise.
 * `source` overrides where the module is loaded from (used by tests).
 */
export function loadEngine(source?: InitInput): Promise<void> {
  ready ??= init(source === undefined ? undefined : { module_or_path: source }).then((exports) => {
    memory = exports.memory;
  });
  return ready;
}

/** The module's linear memory. For tests that exercise view invalidation. */
export function wasmMemory(): WebAssembly.Memory {
  if (!memory) throw new Error("the engine is not loaded");
  return memory;
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

  /** The pointer left the canvas. */
  pointerLeave(): number {
    return this.#session.pointer_leave();
  }

  /** `deltaY` in CSS pixels. */
  wheel(x: number, y: number, deltaY: number, modifiers: number): number {
    return this.#session.wheel(x, y, deltaY, modifiers);
  }

  key(action: KeyAction): number {
    return this.#session.key(action);
  }

  /** One completed command-line entry. The text is interpreted in Rust, never here. */
  submitText(text: string): number {
    return this.#session.submit_text(text);
  }

  /** Recalls an older submitted line, if there is one. */
  historyPrevious(): string | undefined {
    return this.#session.history_previous();
  }

  /** Recalls a newer submitted line; an empty string means "a new line". */
  historyNext(): string | undefined {
    return this.#session.history_next();
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

  /** Lends the transient overlay's buffers to `consume` for the duration of the call. */
  readOverlay<T>(consume: (frame: FrameBuffers) => T): T {
    return consume({
      segments: this.#session.overlay_segments(),
      batches: this.#session.overlay_batches(),
    });
  }

  /** Clip transform for the overlay. An owned copy, safe to keep. */
  overlayTransform(): ClipTransform {
    return this.#session.overlay_transform();
  }

  uiState(): UiState {
    return this.#session.ui_state() as UiState;
  }

  /** Command-log lines numbered `since` and later. */
  logSince(since: number): string[] {
    return this.#session.log_since(since);
  }
}
