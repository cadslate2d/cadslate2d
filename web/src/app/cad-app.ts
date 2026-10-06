// The application object: one engine session, one UI store, one frame loop.
// React components and the canvas binding talk to the engine only through it.
import { Dirty, HostRequest, type CadEngine, type KeyAction } from "../cad/engine";
import { FrameLoop, type ScheduleFrame } from "./frame-loop";
import { browserHost, type AutosaveRecord, type HostServices } from "./host-services";
import { UiStore } from "./ui-store";

const RENDER_FLAGS = Dirty.Scene | Dirty.Overlay | Dirty.View;
const DXF_ACCEPT = ".dxf";
const DXF_MIME_TYPE = "application/dxf";
const DEFAULT_FILE_NAME = "drawing.dxf";
const RECOVERED_FILE_NAME = "recovered.dxf";
/** How long after the last change the drawing is kept (ADR-0019). */
export const AUTOSAVE_DELAY_MS = 1000;

/** Runs `callback` after `ms`; returns what cancels it. `setTimeout`, injectable for tests. */
export type Timer = (callback: () => void, ms: number) => () => void;
const browserTimer: Timer = (callback, ms) => {
  const id = setTimeout(callback, ms);
  return () => clearTimeout(id);
};

export const DISCARD_CHANGES_MESSAGE =
  "The current drawing has changes that have not been exported. Discard them?";

export interface CadAppOptions {
  readonly schedule?: ScheduleFrame;
  readonly host?: HostServices;
  readonly timer?: Timer;
  readonly autosaveDelay?: number;
}

export class CadApp {
  readonly engine: CadEngine;
  readonly ui = new UiStore();
  readonly loop: FrameLoop;
  readonly #host: HostServices;
  readonly #timer: Timer;
  readonly #autosaveDelay: number;
  /** Draws a frame. Set while a canvas is attached. */
  #render: (() => void) | undefined;
  /** The revision last kept, so an unchanged drawing is not written again. */
  #keptRevision: number | undefined;
  /** Cancels the pending autosave, while one is pending. */
  #cancelAutosave: (() => void) | undefined;
  /** The autosave found on start, while the user has not decided about it. */
  #recoverable: AutosaveRecord | undefined;

  constructor(engine: CadEngine, options: CadAppOptions = {}) {
    this.engine = engine;
    this.#host = options.host ?? browserHost;
    this.#timer = options.timer ?? browserTimer;
    this.#autosaveDelay = options.autosaveDelay ?? AUTOSAVE_DELAY_MS;
    this.loop = new FrameLoop(
      (sample) => engine.pointerMove(sample.x, sample.y, sample.modifiers),
      (dirty) => this.#onFrame(dirty),
      options.schedule ?? ((callback) => requestAnimationFrame(callback)),
    );
    this.ui.refresh(engine);
  }

  /** Registers the function that draws a frame; returns a function that removes it. */
  setRenderer(render: () => void): () => void {
    this.#render = render;
    this.loop.invalidate(RENDER_FLAGS);
    return () => {
      if (this.#render === render) this.#render = undefined;
    };
  }

  /** One completed command-line entry. Rust interprets it. */
  submitText(text: string): void {
    this.loop.flushPointerMove();
    this.#afterInput(this.engine.submitText(text));
  }

  /** Starts a command from the UI. It is not a shortcut around the command line: Rust echoes and runs it. */
  invoke(name: string): void {
    this.loop.flushPointerMove();
    this.#afterInput(this.engine.invoke(name));
  }

  /**
   * Runs a command with its answers, exactly as if each had been typed: a
   * panel's button is a macro over the command line, never a second path
   * into the drawing (DESIGN §5, §15). The log shows every step.
   */
  run(name: string, ...answers: readonly string[]): void {
    this.invoke(name);
    for (const answer of answers) this.submitText(answer);
  }

  key(action: KeyAction): void {
    this.loop.flushPointerMove();
    this.#afterInput(this.engine.key(action));
  }

  historyPrevious(): string | undefined {
    return this.engine.historyPrevious();
  }

  historyNext(): string | undefined {
    return this.engine.historyNext();
  }

  dispose(): void {
    this.#cancelAutosave?.();
    this.#render = undefined;
    this.engine.dispose();
  }

  // ── autosave and recovery (DESIGN §25; ADR-0019) ──

  /**
   * Looks for a drawing kept in the browser that no file holds — the work
   * of a tab that closed, a browser that crashed — and offers it.
   */
  async start(): Promise<void> {
    const record = await this.#host.autosave.load().catch(() => undefined);
    if (!record || record.exported) return;
    this.#recoverable = record;
    this.ui.setRecovery({ name: record.name, savedAt: record.savedAt });
  }

  /** Recovering is a normal open: the kept drawing replaces the current one. */
  recover(): void {
    const record = this.#recoverable;
    this.#recoverable = undefined;
    this.ui.setRecovery(undefined);
    if (!record) return;
    const dirty = this.engine.loadDxf(
      new TextEncoder().encode(record.text),
      record.name ?? RECOVERED_FILE_NAME,
    );
    this.loop.invalidate(dirty);
    // What was recovered is still unexported: it stays recoverable.
    this.#keptRevision = this.engine.uiState().revision;
  }

  /** The kept drawing is not wanted: it goes, and the slot is free. */
  discardRecovery(): void {
    this.#recoverable = undefined;
    this.ui.setRecovery(undefined);
    void this.#host.autosave.clear().catch(() => undefined);
  }

  #onFrame(dirty: number): void {
    // The UI state first: drawing reads it — whether lineweights are shown —
    // and must see what this frame's input changed, not the frame before's.
    if (dirty & Dirty.Ui) this.ui.refresh(this.engine);
    if (dirty & RENDER_FLAGS) this.#render?.();
    if (dirty & Dirty.Scene) this.#scheduleAutosave();
  }

  /** After a committed change, and a pause: the drawing is kept, once. */
  #scheduleAutosave(): void {
    const state = this.engine.uiState();
    if (!state.unsavedChanges || state.revision === this.#keptRevision) return;
    this.#cancelAutosave?.();
    this.#cancelAutosave = this.#timer(() => {
      this.#cancelAutosave = undefined;
      void this.#keep(false);
    }, this.#autosaveDelay);
  }

  /** Writes the drawing to the autosave slot; `exported` says a file holds it too. */
  async #keep(exported: boolean): Promise<void> {
    const state = this.engine.uiState();
    if (!exported && !state.unsavedChanges) return;
    this.#keptRevision = state.revision;
    try {
      await this.#host.autosave.save({
        name: state.documentName,
        text: this.engine.snapshotDxf(),
        savedAt: Date.now(),
        exported,
      });
    } catch {
      // Storage refused: the drawing is still on screen; the next change tries again.
      this.#keptRevision = undefined;
    }
  }

  #afterInput(dirty: number): void {
    this.loop.invalidate(dirty);
    // Handled now rather than on the next frame: opening a file picker must
    // happen while the browser still counts the key press as a user gesture.
    if (dirty & Dirty.Host) void this.#serveHostRequest(this.engine.takeHostRequest());
  }

  async #serveHostRequest(request: HostRequest): Promise<void> {
    switch (request) {
      case HostRequest.OpenFile:
        return this.#openFile();
      case HostRequest.ExportFile:
        return this.#exportFile();
      case HostRequest.None:
        return;
    }
  }

  async #openFile(): Promise<void> {
    if (this.engine.uiState().unsavedChanges && !this.#host.confirm(DISCARD_CHANGES_MESSAGE))
      return;
    const file = await this.#host.pickFile(DXF_ACCEPT);
    if (!file) return;
    this.loop.invalidate(this.engine.loadDxf(file.bytes, file.name));
    // The opened file holds the drawing: whatever was kept before is replaced.
    this.#cancelAutosave?.();
    await this.#keep(true);
  }

  #exportFile(): void {
    const state = this.engine.uiState();
    // DESIGN §24.2: never imply a lossless round trip when content was
    // skipped. The warning names what the file will lose; the user decides.
    if (state.exportLosses.length > 0) {
      this.ui.setExportWarning(state.exportLosses);
      return;
    }
    this.#saveExport();
  }

  /** The export the warning asked about: the user chose to go ahead. */
  exportAnyway(): void {
    this.ui.setExportWarning(undefined);
    this.#saveExport();
  }

  /** The export the warning asked about: the user chose not to. */
  cancelExport(): void {
    this.ui.setExportWarning(undefined);
  }

  /** Takes the import report off the screen. */
  dismissImportReport(): void {
    this.loop.invalidate(this.engine.dismissImportReport());
  }

  #saveExport(): void {
    const state = this.engine.uiState();
    this.#host.saveFile(
      state.documentName ?? DEFAULT_FILE_NAME,
      this.engine.exportDxf(),
      DXF_MIME_TYPE,
    );
    this.loop.invalidate(Dirty.Ui);
    // The file holds the drawing now: a reload has nothing to recover.
    this.#cancelAutosave?.();
    void this.#keep(true);
  }
}
