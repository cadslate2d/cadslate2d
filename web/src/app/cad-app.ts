// The application object: one engine session, one UI store, one frame loop.
// React components and the canvas binding talk to the engine only through it.
import { Dirty, HostRequest, SnapshotStatus, type CadEngine, type KeyAction } from "../cad/engine";
import { FrameLoop, type ScheduleFrame } from "./frame-loop";
import {
  browserHost,
  type AutosaveRecord,
  type FileTarget,
  type HostServices,
  type Settings,
  type Theme,
} from "./host-services";
import { UiStore } from "./ui-store";

const RENDER_FLAGS = Dirty.Scene | Dirty.Overlay | Dirty.View;
const DXF_ACCEPT = ".dxf";
const DXF_MIME_TYPE = "application/dxf";
const PDF_MIME_TYPE = "application/pdf";
const DEFAULT_FILE_NAME = "drawing.dxf";
const RECOVERED_FILE_NAME = "recovered.dxf";
/** How long after the last change the drawing is kept (ADR-0019). */
export const AUTOSAVE_DELAY_MS = 1000;

/**
 * How long the pointer must stay still to rest on a point (#220): object
 * snap tracking acquires the snap point it rests on. The engine keeps no
 * clock; the app times the rest and tells it.
 */
export const POINTER_REST_MS = 400;
/**
 * Records the snapshot writes per slice (ADR-0024): about ten milliseconds
 * of a large drawing, so the frames between slices are kept.
 */
export const SNAPSHOT_SLICE = 2000;

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

/** What the settings panel shows and changes; the engine's part of the settings is in the UI snapshot. */
export interface SettingsSnapshot {
  /** Whether the panel is open: SETTINGS opens it. */
  readonly open: boolean;
  readonly theme: Theme;
  readonly autosaveDelayMs: number;
}

/** The settings as the panel shows them (#186): an external store React subscribes to, like the UI state. */
export class SettingsStoreView {
  #snapshot: SettingsSnapshot;
  readonly #listeners = new Set<() => void>();

  constructor(snapshot: SettingsSnapshot) {
    this.#snapshot = snapshot;
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  readonly getSnapshot = (): SettingsSnapshot => this.#snapshot;

  set(changes: Partial<SettingsSnapshot>): void {
    this.#snapshot = { ...this.#snapshot, ...changes };
    this.#listeners.forEach((listener) => listener());
  }
}

export class CadApp {
  readonly engine: CadEngine;
  readonly ui = new UiStore();
  readonly loop: FrameLoop;
  readonly #host: HostServices;
  readonly #timer: Timer;
  #autosaveDelay: number;
  /** The settings panel's state: the theme and the delay are the host's, kept here (#186). */
  readonly settings: SettingsStoreView;
  /** The settings as last kept, so they are written only when one changes. */
  #keptSettings: Settings | undefined;
  /** Draws a frame. Set while a canvas is attached. */
  #render: (() => void) | undefined;
  /** The revision last kept, so an unchanged drawing is not written again. */
  #keptRevision: number | undefined;
  /** Cancels the pending autosave, while one is pending. */
  #cancelAutosave: (() => void) | undefined;
  /** The autosave found on start, while the user has not decided about it. */
  #recoverable: AutosaveRecord | undefined;
  /** The file the drawing came from or last went to, where the browser can write back. */
  #target: FileTarget | undefined;
  /** Whether the save the export warning asked about must ask where. */
  #pendingSaveAsks = false;
  /** When the drawing was last kept, for the failure dialog. */
  #keptAt: number | undefined;
  /** The engine has panicked (#176): nothing reaches it any more. */
  #failed = false;
  /** Cancels the pending pointer rest, while the pointer is moving. */
  #cancelRest: (() => void) | undefined;

  constructor(engine: CadEngine, options: CadAppOptions = {}) {
    this.engine = engine;
    this.#host = options.host ?? browserHost;
    this.#timer = options.timer ?? browserTimer;
    this.#autosaveDelay = options.autosaveDelay ?? AUTOSAVE_DELAY_MS;
    this.settings = new SettingsStoreView({
      open: false,
      theme: "dark",
      autosaveDelayMs: this.#autosaveDelay,
    });
    this.loop = new FrameLoop(
      (sample) => {
        this.#awaitRest();
        return engine.pointerMove(sample.x, sample.y, sample.modifiers);
      },
      (dirty) => this.#onFrame(dirty),
      options.schedule ?? ((callback) => requestAnimationFrame(callback)),
    );
    engine.onFailure = (message) => this.#fail(message);
    this.ui.refresh(engine);
  }

  /** Starts timing a rest of the pointer afresh; when it rests, the engine is told. */
  #awaitRest(): void {
    this.#cancelRest?.();
    this.#cancelRest = this.#timer(() => {
      this.#cancelRest = undefined;
      if (!this.#failed) this.loop.invalidate(this.engine.pointerRest());
    }, POINTER_REST_MS);
  }

  /** Whether the engine has stopped. Everything the app is asked afterwards is declined. */
  get failed(): boolean {
    return this.#failed;
  }

  /** Loads the page afresh: a new engine, and the kept drawing offered back (ADR-0019). */
  reload(): void {
    this.#host.reload();
  }

  /**
   * The engine panicked in some call. The frame loop stops, the autosave is
   * never written again — what it holds is the drawing as last kept, and
   * the dialog says so — and every entry point declines from now on.
   */
  #fail(message: string): void {
    if (this.#failed) return;
    this.#failed = true;
    this.#cancelAutosave?.();
    this.#cancelAutosave = undefined;
    this.loop.stop();
    this.ui.setFailure({ message, keptAt: this.#keptAt });
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
    if (this.#failed) return;
    this.loop.flushPointerMove();
    this.#afterInput(this.engine.submitText(text));
  }

  /** Starts a command from the UI. It is not a shortcut around the command line: Rust echoes and runs it. */
  invoke(name: string): void {
    if (this.#failed) return;
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
    if (this.#failed) return;
    this.loop.flushPointerMove();
    this.#afterInput(this.engine.key(action));
  }

  historyPrevious(): string | undefined {
    if (this.#failed) return undefined;
    return this.engine.historyPrevious();
  }

  historyNext(): string | undefined {
    if (this.#failed) return undefined;
    return this.engine.historyNext();
  }

  dispose(): void {
    this.#cancelAutosave?.();
    this.#render = undefined;
    this.engine.dispose();
  }

  // ── autosave and recovery (DESIGN §25; ADR-0019) ──

  /**
   * Gives the engine and the shell what the browser kept (#186), then
   * looks for a drawing kept in the browser that no file holds — the work
   * of a tab that closed, a browser that crashed — and offers it.
   */
  async start(): Promise<void> {
    this.#restoreSettings();
    const record = await this.#host.autosave.load().catch(() => undefined);
    if (!record || record.exported) return;
    this.#recoverable = record;
    this.ui.setRecovery({ name: record.name, savedAt: record.savedAt });
  }

  // ── settings (#186) ──

  /** What the browser kept, given to the engine in one call and to the shell. */
  #restoreSettings(): void {
    const kept = this.#host.settings.load();
    const current = this.ui.getSnapshot();
    const theme = kept.theme ?? "dark";
    this.#autosaveDelay = kept.autosaveDelayMs ?? this.#autosaveDelay;
    this.settings.set({ theme, autosaveDelayMs: this.#autosaveDelay });
    this.#host.applyTheme(theme);
    if (!this.#failed) {
      const dirty = this.engine.restoreSettings(
        kept.toggles ?? current.toggles,
        kept.unit ?? current.unit,
        kept.aliases ?? current.userAliases,
      );
      this.loop.invalidate(dirty);
      this.ui.refresh(this.engine);
    }
    this.#keptSettings = this.#currentSettings();
  }

  #currentSettings(): Settings {
    const ui = this.ui.getSnapshot();
    const shell = this.settings.getSnapshot();
    return {
      toggles: ui.toggles,
      unit: ui.unit,
      aliases: ui.userAliases,
      autosaveDelayMs: shell.autosaveDelayMs,
      theme: shell.theme,
    };
  }

  /** Writes the settings when one has changed since they were last kept. */
  #keepSettings(): void {
    const kept = this.#keptSettings;
    if (!kept) return;
    const current = this.#currentSettings();
    const same =
      current.toggles === kept.toggles &&
      current.unit === kept.unit &&
      current.aliases === kept.aliases &&
      current.autosaveDelayMs === kept.autosaveDelayMs &&
      current.theme === kept.theme;
    if (same) return;
    this.#keptSettings = current;
    this.#host.settings.save(current);
  }

  openSettings(): void {
    this.settings.set({ open: true });
  }

  closeSettings(): void {
    this.settings.set({ open: false });
  }

  setTheme(theme: Theme): void {
    this.settings.set({ theme });
    this.#host.applyTheme(theme);
    this.#keepSettings();
  }

  /** The pause after a change before the drawing is kept, in milliseconds; never less than a tenth of a second. */
  setAutosaveDelay(ms: number): void {
    const delay = Math.max(100, Math.round(ms));
    this.#autosaveDelay = delay;
    this.settings.set({ autosaveDelayMs: delay });
    this.#keepSettings();
  }

  /** Recovering is a normal open: the kept drawing replaces the current one. */
  recover(): void {
    const record = this.#recoverable;
    this.#recoverable = undefined;
    this.ui.setRecovery(undefined);
    if (!record || this.#failed) return;
    const dirty = this.engine.loadDxf(record.bytes, record.name ?? RECOVERED_FILE_NAME);
    this.loop.invalidate(dirty);
    this.#target =
      record.handle && record.name ? { name: record.name, handle: record.handle } : undefined;
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
    if (dirty & Dirty.Ui) {
      this.ui.refresh(this.engine);
      this.#keepSettings();
    }
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
    if (this.#failed) return;
    const state = this.engine.uiState();
    if (!exported && !state.unsavedChanges) return;
    this.#keptRevision = state.revision;
    // The drawing changed while the snapshot was being written: what the
    // next pause keeps will have the change too.
    const bytes = await this.#snapshot();
    if (!bytes) {
      this.#keptRevision = undefined;
      return;
    }
    try {
      // Only a real handle survives the structured clone into storage.
      const handle = this.#target?.handle;
      const storable =
        typeof FileSystemFileHandle !== "undefined" && handle instanceof FileSystemFileHandle;
      const savedAt = Date.now();
      await this.#host.autosave.save({
        name: state.documentName,
        handle: storable ? handle : undefined,
        bytes,
        savedAt,
        exported,
      });
      this.#keptAt = savedAt;
    } catch {
      // Storage refused: the drawing is still on screen; the next change tries again.
      this.#keptRevision = undefined;
    }
  }

  /**
   * The drawing as DXF, written a slice per turn of the event loop
   * (ADR-0024), so frames are drawn between; `undefined` when the drawing
   * changed meanwhile, or the engine stopped.
   */
  #snapshot(): Promise<Uint8Array | undefined> {
    this.engine.snapshotBegin();
    return new Promise((resolve) => {
      const slice = () => {
        if (this.#failed) {
          resolve(undefined);
          return;
        }
        switch (this.engine.snapshotStep(SNAPSHOT_SLICE)) {
          case SnapshotStatus.Running:
            this.#timer(slice, 0);
            return;
          case SnapshotStatus.Done:
            resolve(this.engine.snapshotBytes());
            return;
          case SnapshotStatus.Changed:
            resolve(undefined);
        }
      };
      slice();
    });
  }

  #afterInput(dirty: number): void {
    // The next keystroke may arrive before the next frame: what it means —
    // whether Space submits — must follow this input, not the frame before.
    if (dirty & Dirty.Ui) this.ui.refresh(this.engine);
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
        return this.#save(true);
      case HostRequest.SaveFile:
        return this.#save(false);
      case HostRequest.Settings:
        this.openSettings();
        return;
      case HostRequest.Plot:
        return this.#plot();
      case HostRequest.None:
        return;
    }
  }

  async #openFile(): Promise<void> {
    if (this.engine.uiState().unsavedChanges && !this.#host.confirm(DISCARD_CHANGES_MESSAGE))
      return;
    const file = await this.#host.files.open(DXF_ACCEPT);
    if (!file) return;
    this.loop.invalidate(this.engine.loadDxf(file.bytes, file.name));
    this.#target = file.target;
    // The opened file holds the drawing: whatever was kept before is replaced.
    this.#cancelAutosave?.();
    await this.#keep(true);
  }

  /**
   * Saves the drawing: back to its file when the browser can and `ask` is
   * not set (SAVE), otherwise to a file the user chooses (SAVEAS, DXFOUT),
   * which is a download where the browser has no picker (DESIGN §25.3).
   */
  async #save(ask: boolean): Promise<void> {
    const state = this.engine.uiState();
    // DESIGN §24.2: never imply a lossless round trip when content was
    // skipped. The warning names what the file will lose; the user decides.
    if (state.exportLosses.length > 0) {
      this.#pendingSaveAsks = ask;
      this.ui.setExportWarning(state.exportLosses);
      return;
    }
    await this.#performSave(ask);
  }

  /** The save the warning asked about: the user chose to go ahead. */
  exportAnyway(): void {
    this.ui.setExportWarning(undefined);
    if (this.#failed) return;
    void this.#performSave(this.#pendingSaveAsks);
  }

  /** The export the warning asked about: the user chose not to. */
  cancelExport(): void {
    this.ui.setExportWarning(undefined);
  }

  /** Takes the import report off the screen. */
  dismissImportReport(): void {
    if (this.#failed) return;
    this.loop.invalidate(this.engine.dismissImportReport());
  }

  /**
   * PLOT: the page the engine made, offered as a PDF under the drawing's
   * name (#187) — and the layout's, when a layout is plotted (#194).
   */
  async #plot(): Promise<void> {
    const pdf = this.engine.plotPdf();
    const state = this.engine.uiState();
    const name = (state.documentName ?? DEFAULT_FILE_NAME).replace(/\.dxf$/i, "");
    const file = state.paper ? `${name}-${state.space}.pdf` : `${name}.pdf`;
    await this.#host.files.saveAs(file, pdf, PDF_MIME_TYPE);
  }

  async #performSave(ask: boolean): Promise<void> {
    const files = this.#host.files;
    const state = this.engine.uiState();
    const text = this.engine.snapshotDxf();
    const known = this.#target;
    const writtenBack =
      !ask && known !== undefined && files.writesBack && (await files.write(known, text));
    const target = writtenBack
      ? known
      : await files.saveAs(state.documentName ?? DEFAULT_FILE_NAME, text, DXF_MIME_TYPE);
    if (!target) return;
    this.#target = files.writesBack ? target : undefined;
    this.loop.invalidate(this.engine.markExported(target.name));
    // The file holds the drawing now: a reload has nothing to recover.
    this.#cancelAutosave?.();
    void this.#keep(true);
  }
}
