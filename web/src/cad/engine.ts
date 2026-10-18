// The typed wrapper around the WASM boundary, and the only module that imports
// the generated package (ADR-0002, ADR-0004).
//
// The boundary is coarse on purpose: one call per browser event, each
// returning dirty flags; frame data is then pulled in bulk. Nothing here
// exposes entities, and nothing here interprets CAD state.
import init, {
  CadSession,
  core_version,
  panic_for_test,
  panic_message,
  type InitInput,
} from "./pkg/cad_wasm";

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
  GroupSelection: 11,
  ToggleSnap: 12,
  ToggleOtrack: 13,
  Copy: 14,
  Cut: 15,
  Paste: 16,
} as const;
export type KeyAction = (typeof KeyAction)[keyof typeof KeyAction];

/** Bits of `UiState.toggles`: the drafting aids that are switched on. */
export const ToggleBit = {
  Osnap: 1,
  Grid: 1 << 1,
  Ortho: 1 << 2,
  Polar: 1 << 3,
  Lineweight: 1 << 4,
  Snap: 1 << 5,
  Otrack: 1 << 6,
} as const;

/** Something only the host can do, requested by a command. */
export const HostRequest = {
  None: 0,
  OpenFile: 1,
  ExportFile: 2,
  SaveFile: 3,
  Settings: 4,
  Plot: 5,
  InsertFile: 6,
  WriteBlock: 7,
  NewDrawing: 8,
} as const;
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
  /** Whether the clipboard holds something to paste (#247). */
  readonly canPaste: boolean;
  /** Drafting toggles that are on; test with `ToggleBit`. */
  readonly toggles: number;
  /** The drawing's unit as its suffix, or empty for unitless (#186). */
  readonly unit: string;
  /** The user's command aliases, one per line (#186). */
  readonly userAliases: string;
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
  /** The layouts, by name, in tab order (#193). */
  readonly layouts: readonly string[];
  /** The space the session is in, by name: `Model`, or a layout's. */
  readonly space: string;
  /** Whether the session is on a layout's paper. */
  readonly paper: boolean;
  /** Whether the model is worked through a viewport (MSPACE). */
  readonly through: boolean;
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
  /** How the objects were selected — "window", "fence", "quick select" — or empty. */
  readonly method: string;
  /** The groups the selected objects are in, by name (#214). */
  readonly groups: readonly string[];
}

/** One row of the Layers panel: names and looks only, never IDs or geometry. */
export interface LayerRow {
  readonly name: string;
  /** The colour number, 1 to 255. */
  readonly color: number;
  /** What the colour looks like, or `undefined` for the theme's foreground. */
  readonly rgb?: readonly [number, number, number];
  readonly on: boolean;
  /** Frozen (#213): hidden like a layer that is off, switched apart. */
  readonly frozen: boolean;
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
/** Floats per filled triangle instance: its three corners (ADR-0021). */
export const FLOATS_PER_TRIANGLE = 6;
export const WORDS_PER_BATCH = 3;
/** Bytes per row of the style table: red, green, blue, kind, weight, pattern, and two spare (ADR-0014). */
export const BYTES_PER_STYLE = 8;
/** The kind of a style row whose colour is the theme's foreground. */
export const STYLE_FOREGROUND = 0;
/** The kind of a style row whose first three bytes are its colour. */
export const STYLE_RGB = 255;
/** The kind of a style row drawn in the canvas's own colour: a wipeout's mask (#226). */
export const STYLE_BACKGROUND = 1;
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
  /** `x0, y0, x1, y1, x2, y2` per filled triangle instance (ADR-0021). */
  readonly triangles: Float32Array;
  /** `style, first, count` per triangle batch. */
  readonly triangleBatches: Uint32Array;
  /**
   * One style word per segment instance: in the low 16 bits a row of the
   * style table, in the high 16 the tier of its entity in the drawing order,
   * a signed number, which the renderer draws as depth (ADR-0032). Only the
   * committed scene has them; overlay geometry is styled by its batches.
   */
  readonly segmentStyles?: Uint32Array;
  /** One style word per arc instance; see `segmentStyles`. */
  readonly arcStyles?: Uint32Array;
  /** One style word per triangle instance; a fill takes the row's colour. */
  readonly triangleStyles?: Uint32Array;
  /**
   * Per segment instance, the distance along its entity's path at its
   * start, in drawing units: where its linetype pattern is (ADR-0015).
   * Only the committed scene has them; the overlay is drawn continuous.
   */
  readonly segmentDistances?: Float32Array;
  /** Per arc instance, that distance at its counter-clockwise start and `±1` for the path's direction. */
  readonly arcDistances?: Float32Array;
  /**
   * Which instances changed since the last frame (ADR-0022): `kind, first,
   * count` per range — `DirtyKind` — in instances of that kind. Only the
   * committed scene has them; absent, everything is new. An owned copy.
   */
  readonly dirty?: Uint32Array;
  /**
   * The runs of instances drawn under each transform of `sceneTransform`
   * (#188): `segmentFirst, segmentCount, arcFirst, arcCount, triangleFirst,
   * triangleCount, paper` per region — `WORDS_PER_REGION`. `paper` is 1 for
   * a run of a layout's own entities, drawn under the paper's transform,
   * and 0 for the model's, drawn through each viewport (#193). Only the
   * committed scene has them; the overlay is one run under one transform.
   * An owned copy.
   */
  readonly regions?: Uint32Array;
}

/** Words per scene region: first and count for segments, arcs, and triangles, then the paper flag. */
export const WORDS_PER_REGION = 7;

/**
 * The viewports of the layout shown (#193): per viewport, its frame in clip
 * space — `x0, y0, x1, y1` — then one clip transform per scene region for
 * the model seen through it. Empty on the model.
 */
export type SceneViewports = Float32Array;
/** Floats per viewport before its transforms: the frame. */
export const FLOATS_PER_VIEWPORT_FRAME = 4;

/** What `snapshotStep` answers. */
export const SnapshotStatus = {
  /** More to write: step again. */
  Running: 0,
  /** The file is complete: `snapshotBytes` has it. */
  Done: 1,
  /** The drawing changed since the snapshot began, or none was begun: begin again after the next pause. */
  Changed: 2,
} as const;
export type SnapshotStatus = (typeof SnapshotStatus)[keyof typeof SnapshotStatus];

/** The instance kind a dirty range names. */
export const DirtyKind = {
  Segments: 0,
  Arcs: 1,
  Triangles: 2,
} as const;
export type DirtyKind = (typeof DirtyKind)[keyof typeof DirtyKind];

/** Words per dirty range: `kind, first, count`. */
export const WORDS_PER_DIRTY_RANGE = 3;

/** A run of instances to upload: `[first, count]`. */
export type InstanceRange = readonly [first: number, count: number];

/** The ranges of one kind in a scene's dirty list, as `[first, count]` pairs. */
export function dirtyRanges(dirty: Uint32Array, kind: DirtyKind): InstanceRange[] {
  const ranges: InstanceRange[] = [];
  for (let offset = 0; offset < dirty.length; offset += WORDS_PER_DIRTY_RANGE) {
    if (dirty[offset] === kind) ranges.push([dirty[offset + 1] ?? 0, dirty[offset + 2] ?? 0]);
  }
  return ranges;
}

/**
 * `[scaleX, scaleY, translateX, translateY]`: `clip = position * scale + translate`.
 * The committed scene's holds four per region (#188), in the order of the
 * frame's `regions`; the overlay's is one.
 */
export type ClipTransform = Float32Array;
/** Floats per clip transform. */
export const FLOATS_PER_TRANSFORM = 4;

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
/**
 * The engine has panicked (#176): the call that raised this is the one it
 * happened in, or any call after. The message is the panic's.
 */
export class EngineFailure extends Error {
  constructor(message: string) {
    super(message);
    this.name = "EngineFailure";
  }
}

export class CadEngine {
  readonly #session: CadSession;
  /** The panic that stopped the engine, once one has. */
  #failure: string | undefined;
  /** Told once, when the engine first fails. */
  onFailure: ((message: string) => void) | undefined;

  constructor() {
    const session = new CadSession();
    // Every call into the session passes through the guard: after a panic
    // none reaches it.
    this.#session = new Proxy(session, {
      get: (target, property) => {
        const value = Reflect.get(target, property, target) as unknown;
        if (typeof value !== "function") return value;
        return (...args: unknown[]) =>
          this.#call(() => (value as (...a: unknown[]) => unknown).apply(target, args));
      },
    });
  }

  /** What stopped the engine, once something has. */
  get failure(): string | undefined {
    return this.#failure;
  }

  /** Makes the engine panic, for tests of what happens then. */
  panicForTest(): void {
    this.#call(() => panic_for_test());
  }

  #call<T>(invoke: () => T): T {
    if (this.#failure !== undefined) throw new EngineFailure(this.#failure);
    try {
      return invoke();
    } catch (cause) {
      // A trap is a panic (the engine aborts on one); anything else is an
      // ordinary error the call reports.
      if (!(cause instanceof WebAssembly.RuntimeError)) throw cause;
      let message: string | undefined;
      try {
        message = panic_message();
      } catch {
        // The message is not to be had: the trap's own text will do.
      }
      this.#failure = message ?? String(cause);
      this.onFailure?.(this.#failure);
      throw new EngineFailure(this.#failure);
    }
  }

  /** Releases the WASM-side session. The engine must not be used afterwards. */
  dispose(): void {
    if (this.#failure === undefined) this.#session.free();
  }

  // ── input: each call returns a Dirty bitmask ──

  /** Canvas size in CSS pixels. */
  resize(width: number, height: number): number {
    return this.#session.resize(width, height);
  }

  /** The pointer has rested a moment (#220): object snap tracking acquires the point under it. */
  pointerRest(): number {
    return this.#session.pointer_rest();
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

  /** File › New (#245): an empty, untitled drawing in place of this one. */
  newDrawing(): number {
    return this.#session.new_drawing();
  }

  /** The file INSERT asked for (#222), read and handed to the waiting command. */
  insertDxf(bytes: Uint8Array, name: string): number {
    return this.#session.insert_dxf(bytes, name);
  }

  /** The file WBLOCK made (#222), once: its name and its text. */
  takeBlockFile(): { name: string; text: string } | undefined {
    const [name, text] = this.#session.take_block_file();
    return name === undefined || text === undefined ? undefined : { name, text };
  }

  /**
   * What the browser kept between sessions, given back once on start
   * (#186): the toggles' bits, the unit's suffix (empty for none), and the
   * user's aliases as text. One coarse call (ADR-0002).
   */
  restoreSettings(toggles: number, unit: string, aliases: string): number {
    return this.#session.restore_settings(toggles, unit, aliases);
  }

  /** The drawing as ASCII DXF, for a save or an autosave: marks nothing. */
  snapshotDxf(): string {
    return this.#session.snapshot_dxf();
  }

  /** The space shown as a one-page PDF with the session's plot settings (#187, #194): ASCII text. */
  plotPdf(): string {
    return this.#session.plot_pdf();
  }

  /** Begins the same snapshot in slices (ADR-0024): `snapshotStep` writes it a few records at a time. */
  snapshotBegin(): void {
    this.#session.snapshot_begin();
  }

  /** Writes up to `budget` more records of the snapshot under way; see `SnapshotStatus`. */
  snapshotStep(budget: number): SnapshotStatus {
    return this.#session.snapshot_step(budget) as SnapshotStatus;
  }

  /** The completed snapshot as UTF-8 bytes, owned. Empty when none is complete. */
  snapshotBytes(): Uint8Array {
    return this.#session.snapshot_bytes();
  }

  /** Records that the drawing was saved — to `name`, when it has a file now. */
  markExported(name?: string): number {
    return this.#session.mark_exported(name);
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
    // The ranges first: taking them builds the scene the views are of.
    const dirty = this.#session.scene_dirty_ranges();
    // Views are created inside this call and never escape it.
    return consume({
      dirty,
      regions: this.#session.scene_regions(),
      segments: this.#session.scene_segments(),
      batches: this.#session.scene_batches(),
      arcs: this.#session.scene_arcs(),
      arcBatches: this.#session.scene_arc_batches(),
      triangles: this.#session.scene_triangles(),
      triangleBatches: this.#session.scene_triangle_batches(),
      segmentStyles: this.#session.scene_segment_styles(),
      arcStyles: this.#session.scene_arc_styles(),
      triangleStyles: this.#session.scene_triangle_styles(),
      segmentDistances: this.#session.scene_segment_distances(),
      arcDistances: this.#session.scene_arc_distances(),
    });
  }

  /** Clip transforms for the committed scene, one per region. An owned copy, safe to keep. */
  sceneTransform(): ClipTransform {
    return this.#session.scene_transform();
  }

  /** The viewports of the layout shown, with the model's transforms through each (#193). An owned copy. */
  sceneViewports(): SceneViewports {
    return this.#session.scene_viewports();
  }

  /** Lends the transient overlay's buffers to `consume` for the duration of the call. */
  readOverlay<T>(consume: (frame: FrameBuffers) => T): T {
    return consume({
      segments: this.#session.overlay_segments(),
      batches: this.#session.overlay_batches(),
      arcs: this.#session.overlay_arcs(),
      arcBatches: this.#session.overlay_arc_batches(),
      triangles: this.#session.overlay_triangles(),
      triangleBatches: this.#session.overlay_triangle_batches(),
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
