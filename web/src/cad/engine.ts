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
  /** A command needs the host to act; call `takeHostRequest()`. */
  Host: 1 << 4,
} as const;

/** Keyboard modifier bits passed with pointer and key events. */
export const Modifier = { Shift: 1, Ctrl: 1 << 1, Alt: 1 << 2 } as const;

/** Non-text keys. Text, including Enter and Space-as-Enter, goes through `submitText`. */
export const KeyAction = {
  Escape: 0,
  Undo: 1,
  Redo: 2,
  Open: 3,
  Save: 4,
  Delete: 5,
  ToggleOsnap: 6,
  ToggleGrid: 7,
  ToggleOrtho: 8,
  TogglePolar: 9,
  ToggleLineweight: 10,
} as const;
export type KeyAction = (typeof KeyAction)[keyof typeof KeyAction];

/** Bits of `UiState.toggles`: the drafting aids that are switched on. */
export const ToggleBit = {
  Osnap: 1,
  Grid: 1 << 1,
  Ortho: 1 << 2,
  Polar: 1 << 3,
  Lineweight: 1 << 4,
} as const;

/** Something only the host can do, requested by a command. */
export const HostRequest = { None: 0, OpenFile: 1, ExportFile: 2 } as const;
export type HostRequest = (typeof HostRequest)[keyof typeof HostRequest];

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
  /** The world point a click would give, while the pointer is over the canvas. */
  readonly cursor?: readonly [number, number];
  /** Name of the object snap the cursor is on; `cursor` is then the exact snap point. */
  readonly snap?: string;
  /** Readout of ORTHO or POLAR while it constrains the pointer, e.g. `Polar: 50.0000 < 45°`. */
  readonly tracking?: string;
  readonly selectionCount: number;
  /** Whether Space submits the command line like Enter. */
  readonly spaceAccepts: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  /** Drafting toggles that are on; test with `ToggleBit`. */
  readonly toggles: number;
  /** Name of the file the drawing was opened from, if any. */
  readonly documentName?: string;
  /** The drawing changed since it was opened or last exported. */
  readonly unsavedChanges: boolean;
  /** Changes on every commit, undo, and redo: when there is something new to autosave. */
  readonly revision: number;
  /**
   * What an export will not include, line by line: content the file this
   * drawing came from held and the drawing could not. Empty when an export
   * reproduces the drawing whole; otherwise warn first (DESIGN §24.2).
   */
  readonly exportLosses: readonly string[];
  /** The last import's account, shown until dismissed or the drawing is replaced (#157). */
  readonly importReport?: ImportReport;
  /** The layers as the Layers panel shows them, in table order (#140). */
  readonly layers: readonly LayerRow[];
  /** The names of the loaded linetypes, for a linetype field to offer. */
  readonly linetypes: readonly string[];
  /** The selection as the Properties panel shows it (#141). */
  readonly selection: SelectionSummary;
  /** The text to open the paragraph editor with, while a command waits for one (#144). */
  readonly textEditor?: string;
}

/** What an import did and did not bring in, as lines for the shell. */
export interface ImportReport {
  /** Counts the session's imports, so a new report is told from one already shown. */
  readonly serial: number;
  readonly lines: readonly string[];
}

/**
 * What the Properties panel shows about the selection: counts and the
 * values the objects share. Bounded whatever the selection's size — never
 * a record per object.
 */
export interface SelectionSummary {
  readonly count: number;
  /** "No selection", "1 line", "3 objects: 2 lines, 1 circle". */
  readonly headline: string;
  readonly kinds: readonly (readonly [kind: string, count: number])[];
  /** The layer every selected object is on; absent when they differ. */
  readonly layer?: string;
  readonly color?: string;
  readonly linetype?: string;
  readonly lineweight?: string;
  /** For one object: label and value of what defines it, read only. */
  readonly geometry: readonly (readonly [label: string, value: string])[];
}

/** One row of the Layers panel: names and looks only, never IDs or geometry. */
export interface LayerRow {
  readonly name: string;
  /** The colour number, 1 to 255. */
  readonly color: number;
  /** What the colour looks like, or `undefined` for the theme's foreground. */
  readonly rgb?: readonly [number, number, number];
  readonly on: boolean;
  readonly locked: boolean;
  /** New objects go on this layer. */
  readonly current: boolean;
  readonly linetype: string;
  readonly lineweight: string;
}

/** What a batch of geometry is; the renderer maps each role to a themed style. */
export const StyleRole = {
  Entity: 0,
  Selection: 1,
  Preview: 2,
  Crosshair: 3,
  SelectionWindow: 4,
  SelectionCrossing: 5,
  SnapMarker: 6,
  Tracking: 7,
  GridMinor: 8,
  GridMajor: 9,
  Grip: 10,
  GripHot: 11,
  Hover: 12,
  Removal: 13,
} as const;
export type StyleRole = (typeof StyleRole)[keyof typeof StyleRole];

export const FLOATS_PER_SEGMENT = 4;
export const FLOATS_PER_ARC = 5;
export const WORDS_PER_BATCH = 3;
/** Bytes per row of the style table: red, green, blue, kind, weight, pattern, and two spare (ADR-0014). */
export const BYTES_PER_STYLE = 8;
/** The kind of a style row whose colour is the theme's foreground. */
export const STYLE_FOREGROUND = 0;
/** The kind of a style row whose first three bytes are its colour. */
export const STYLE_RGB = 255;
/** The weight of a style row drawn at the theme's ordinary width; else hundredths of a millimetre. */
export const STYLE_WEIGHT_DEFAULT = 255;
/** The pattern of a style row drawn continuous: row 0 of the pattern table (ADR-0015). */
export const PATTERN_CONTINUOUS = 0;
/** Floats per row of the pattern table: period, count, then up to eight dash and gap lengths. */
export const FLOATS_PER_PATTERN = 10;
/** Floats per arc instance in the arc distance buffer: the path distance and the direction. */
export const FLOATS_PER_ARC_DISTANCE = 2;

/** The style table and the linetype patterns it names, which change together (ADR-0014, ADR-0015). */
export interface StyleTables {
  /** `BYTES_PER_STYLE` bytes per row: `red, green, blue, kind, weight, pattern`. */
  readonly styles: Uint8Array;
  /** `FLOATS_PER_PATTERN` floats per row, in drawing units with LTSCALE applied. */
  readonly patterns: Float32Array;
}

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
  /** `style, first, count` per segment batch. */
  readonly batches: Uint32Array;
  /** `cx, cy, radius, start, sweep` per arc instance (ADR-0009); angles in radians. */
  readonly arcs: Float32Array;
  /** `style, first, count` per arc batch. */
  readonly arcBatches: Uint32Array;
  /**
   * One style index per segment instance: a row of the style table. Only the
   * committed scene has them; overlay geometry is styled by its batches.
   */
  readonly segmentStyles?: Uint16Array;
  /** One style index per arc instance; see `segmentStyles`. */
  readonly arcStyles?: Uint16Array;
  /**
   * Per segment instance, the distance along its entity's path at its
   * start, in drawing units: where its linetype pattern is (ADR-0015).
   * Only the committed scene has them; the overlay is drawn continuous.
   */
  readonly segmentDistances?: Float32Array;
  /** Per arc instance, that distance at its counter-clockwise start and `±1` for the path's direction. */
  readonly arcDistances?: Float32Array;
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

  /**
   * Starts a command from a toolbar button or menu item. Same entry point as
   * a typed command, echoed in the command line the same way.
   */
  invoke(name: string): number {
    return this.#session.invoke(name);
  }

  /** Recalls an older submitted line, if there is one. */
  historyPrevious(): string | undefined {
    return this.#session.history_previous();
  }

  /** Recalls a newer submitted line; an empty string means "a new line". */
  historyNext(): string | undefined {
    return this.#session.history_next();
  }

  // ── files: the host does the I/O, Rust does the format ──

  /** The request behind a `Dirty.Host` flag. Taking it clears it. */
  takeHostRequest(): HostRequest {
    return this.#session.take_host_request() as HostRequest;
  }

  /**
   * Replaces the drawing with the contents of a DXF file. A file that cannot
   * be read leaves the drawing untouched and explains why in the command log.
   */
  loadDxf(bytes: Uint8Array, name: string): number {
    return this.#session.load_dxf(bytes, name);
  }

  /** The drawing as ASCII DXF. Marks the drawing as saved. */
  exportDxf(): string {
    return this.#session.export_dxf();
  }

  /** The drawing as ASCII DXF for an autosave: marks nothing. */
  snapshotDxf(): string {
    return this.#session.snapshot_dxf();
  }

  /** Takes the import report off the screen; the log keeps its lines. */
  dismissImportReport(): number {
    return this.#session.dismiss_import_report();
  }

  // ── output: pulled in bulk ──

  /**
   * Identifies the committed geometry; re-upload committed buffers only when
   * it changes. Recolouring a layer does not change it (ADR-0014).
   */
  get sceneRevision(): number {
    return this.#session.scene_revision();
  }

  /** Identifies the style table; re-upload it only when this changes. */
  get styleRevision(): number {
    return this.#session.scene_style_revision();
  }

  /** Lends the style table and the pattern table to `consume` for the duration of the call. */
  readStyles<T>(consume: (tables: StyleTables) => T): T {
    return consume({
      styles: this.#session.scene_styles(),
      patterns: this.#session.scene_patterns(),
    });
  }

  /** Lends the committed scene's buffers to `consume` for the duration of the call. */
  readScene<T>(consume: (frame: FrameBuffers) => T): T {
    // Views are created inside this call and never escape it.
    return consume({
      segments: this.#session.scene_segments(),
      batches: this.#session.scene_batches(),
      arcs: this.#session.scene_arcs(),
      arcBatches: this.#session.scene_arc_batches(),
      segmentStyles: this.#session.scene_segment_styles(),
      arcStyles: this.#session.scene_arc_styles(),
      segmentDistances: this.#session.scene_segment_distances(),
      arcDistances: this.#session.scene_arc_distances(),
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
      arcs: this.#session.overlay_arcs(),
      arcBatches: this.#session.overlay_arc_batches(),
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
