// The single external store React subscribes to (Observer pattern).
// It mirrors presentation state pulled from the engine: prompt, log,
// cursor readout, counters. It never holds drawing geometry.
import type {
  DocumentFormat,
  ImportReport,
  LayerRow,
  SelectionSummary,
  UiState,
} from "../cad/engine";

/** What the recovery dialog says about the kept drawing: its name and when it was kept. */
export interface Recovery {
  readonly name: string | undefined;
  readonly savedAt: number;
}

/** What the failure dialog says (#176): the panic, and when the drawing was last kept. */
export interface Failure {
  readonly message: string;
  readonly keptAt: number | undefined;
}

/** Log lines kept for display. Older lines scroll out. */
export const LOG_LINES_KEPT = 200;

export interface UiSnapshot {
  readonly prompt: string;
  readonly command: string | undefined;
  readonly log: readonly string[];
  readonly cursor: readonly [number, number] | undefined;
  /** Name of the object snap under the cursor, if any. */
  readonly snap: string | undefined;
  /** Readout of ORTHO or POLAR while it constrains the pointer. */
  readonly tracking: string | undefined;
  readonly selectionCount: number;
  readonly spaceAccepts: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly canPaste: boolean;
  /** Whether constraint bars, and constraint dimensions, are shown (#269). */
  readonly constraintBars: boolean;
  readonly constraintDimensions: boolean;
  /** Drafting toggles that are on; test with `ToggleBit`. */
  readonly toggles: number;
  /** The drawing's unit as its suffix, or empty for unitless. */
  readonly unit: string;
  /** The current isometric plane, or empty with the standard snap style (#336). */
  readonly isoplane: string;
  /** The user's command aliases, one per line, as the engine holds them. */
  readonly userAliases: string;
  readonly documentName: string | undefined;
  /** The format the file came in (#318). */
  readonly documentFormat: DocumentFormat;
  readonly unsavedChanges: boolean;
  /** What an export will not include; empty when nothing (DESIGN §24.2). */
  readonly exportLosses: readonly string[];
  /** The layouts, by name, in tab order (#193). */
  readonly layouts: readonly string[];
  /** The space the session is in, by name: `Model`, or a layout's. */
  readonly space: string;
  /** Whether the session is on a layout's paper; whether the model is worked through a viewport. */
  readonly paper: boolean;
  readonly through: boolean;
  /** The last import's account, until dismissed or the drawing is replaced. */
  readonly importReport: ImportReport | undefined;
  /** The losses an export the user asked for would have; the warning dialog shows them. */
  readonly exportWarning: readonly string[] | undefined;
  /** A drawing kept in the browser that no file holds, offered on start. */
  readonly recovery: Recovery | undefined;
  /** The engine has stopped; the dialog says so and offers a reload. */
  readonly failure: Failure | undefined;
  /** The layers as the Layers panel shows them: presentation data, not the table. */
  readonly layers: readonly LayerRow[];
  readonly linetypes: readonly string[];
  /** The selection as the Properties panel shows it: a summary, never the objects. */
  readonly selection: SelectionSummary;
  /** The text the paragraph editor opens with, while a command waits for a paragraph. */
  readonly textEditor: string | undefined;
  /** Whether the editor takes one line: a text's words, Enter finishing them. */
  readonly textEditorLine: boolean;
}

const NO_SELECTION: SelectionSummary = {
  count: 0,
  headline: "No selection",
  kinds: [],
  geometry: [],
  method: "",
  groups: [],
};

/** What the store reads from the engine. */
export interface UiSource {
  uiState(): UiState;
  logSince(since: number): string[];
}

const INITIAL: UiSnapshot = {
  prompt: "Command:",
  command: undefined,
  log: [],
  cursor: undefined,
  snap: undefined,
  tracking: undefined,
  selectionCount: 0,
  spaceAccepts: true,
  canUndo: false,
  canRedo: false,
  canPaste: false,
  constraintBars: true,
  constraintDimensions: true,
  toggles: 0,
  unit: "",
  isoplane: "",
  userAliases: "",
  documentName: undefined,
  documentFormat: "dxf",
  unsavedChanges: false,
  exportLosses: [],
  layouts: [],
  space: "Model",
  paper: false,
  through: false,
  importReport: undefined,
  exportWarning: undefined,
  recovery: undefined,
  failure: undefined,
  layers: [],
  linetypes: [],
  selection: NO_SELECTION,
  textEditor: undefined,
  textEditorLine: false,
};

export class UiStore {
  #snapshot: UiSnapshot = INITIAL;
  #logSeen = 0;
  #exportWarning: readonly string[] | undefined;
  #recovery: Recovery | undefined;
  #failure: Failure | undefined;
  readonly #listeners = new Set<() => void>();

  // Arrow properties: React calls these detached from the instance.
  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  readonly getSnapshot = (): UiSnapshot => this.#snapshot;

  /** Log lines written since the session began, including any no longer kept. */
  get logTotal(): number {
    return this.#logSeen;
  }

  /** Shows the export warning with what the file will lose, or hides it. */
  setExportWarning(losses: readonly string[] | undefined): void {
    this.#exportWarning = losses;
    this.#snapshot = { ...this.#snapshot, exportWarning: losses };
    this.#listeners.forEach((listener) => listener());
  }

  /** The engine has stopped: the dialog says so, from now on. */
  setFailure(failure: Failure): void {
    this.#failure = failure;
    this.#snapshot = { ...this.#snapshot, failure };
    this.#listeners.forEach((listener) => listener());
  }

  /** Offers the kept drawing, or withdraws the offer. */
  setRecovery(recovery: Recovery | undefined): void {
    this.#recovery = recovery;
    this.#snapshot = { ...this.#snapshot, recovery };
    this.#listeners.forEach((listener) => listener());
  }

  /** Pulls the current state and any new log lines, then notifies subscribers. */
  refresh(source: UiSource): void {
    const state = source.uiState();
    let log = this.#snapshot.log;
    if (state.logTotal > this.#logSeen) {
      log = [...log, ...source.logSince(this.#logSeen)].slice(-LOG_LINES_KEPT);
      this.#logSeen = state.logTotal;
    }
    this.#snapshot = {
      prompt: state.prompt,
      command: state.command,
      log,
      cursor: state.cursor,
      snap: state.snap,
      tracking: state.tracking,
      selectionCount: state.selectionCount,
      spaceAccepts: state.spaceAccepts,
      canUndo: state.canUndo,
      canRedo: state.canRedo,
      canPaste: state.canPaste,
      constraintBars: state.constraintBars,
      constraintDimensions: state.constraintDimensions,
      toggles: state.toggles,
      unit: state.unit,
      isoplane: state.isoplane,
      userAliases: state.userAliases,
      documentName: state.documentName,
      documentFormat: state.documentFormat,
      unsavedChanges: state.unsavedChanges,
      exportLosses: state.exportLosses,
      layouts: state.layouts,
      space: state.space,
      paper: state.paper,
      through: state.through,
      importReport: state.importReport,
      exportWarning: this.#exportWarning,
      recovery: this.#recovery,
      failure: this.#failure,
      layers: state.layers,
      linetypes: state.linetypes,
      selection: state.selection,
      textEditor: state.textEditor,
      textEditorLine: state.textEditorLine ?? false,
    };
    this.#listeners.forEach((listener) => listener());
  }
}
