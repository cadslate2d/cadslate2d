// The single external store React subscribes to (Observer pattern).
// It mirrors presentation state pulled from the engine: prompt, log,
// cursor readout, counters. It never holds drawing geometry.
import type { LayerRow, SelectionSummary, UiState } from "../cad/engine";

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
  /** Drafting toggles that are on; test with `ToggleBit`. */
  readonly toggles: number;
  readonly documentName: string | undefined;
  readonly unsavedChanges: boolean;
  readonly exportIsLossy: boolean;
  /** The layers as the Layers panel shows them: presentation data, not the table. */
  readonly layers: readonly LayerRow[];
  readonly linetypes: readonly string[];
  /** The selection as the Properties panel shows it: a summary, never the objects. */
  readonly selection: SelectionSummary;
  /** The text the paragraph editor opens with, while a command waits for a paragraph. */
  readonly textEditor: string | undefined;
}

const NO_SELECTION: SelectionSummary = {
  count: 0,
  headline: "No selection",
  kinds: [],
  geometry: [],
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
  toggles: 0,
  documentName: undefined,
  unsavedChanges: false,
  exportIsLossy: false,
  layers: [],
  linetypes: [],
  selection: NO_SELECTION,
  textEditor: undefined,
};

export class UiStore {
  #snapshot: UiSnapshot = INITIAL;
  #logSeen = 0;
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
      toggles: state.toggles,
      documentName: state.documentName,
      unsavedChanges: state.unsavedChanges,
      exportIsLossy: state.exportIsLossy,
      layers: state.layers,
      linetypes: state.linetypes,
      selection: state.selection,
      textEditor: state.textEditor,
    };
    this.#listeners.forEach((listener) => listener());
  }
}
