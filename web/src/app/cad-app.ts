// The application object: one engine session, one UI store, one frame loop.
// React components and the canvas binding talk to the engine only through it.
import { Dirty, HostRequest, type CadEngine, type KeyAction } from "../cad/engine";
import { FrameLoop, type ScheduleFrame } from "./frame-loop";
import { browserHost, type HostServices } from "./host-services";
import { UiStore } from "./ui-store";

const RENDER_FLAGS = Dirty.Scene | Dirty.Overlay | Dirty.View;
const DXF_ACCEPT = ".dxf";
const DXF_MIME_TYPE = "application/dxf";
const DEFAULT_FILE_NAME = "drawing.dxf";

export const DISCARD_CHANGES_MESSAGE =
  "The current drawing has changes that have not been exported. Discard them?";
export const LOSSY_EXPORT_MESSAGE =
  "This drawing was opened from a file with content CadSlate2D could not import. " +
  "The exported file will not include that content. Export anyway?";

export interface CadAppOptions {
  readonly schedule?: ScheduleFrame;
  readonly host?: HostServices;
}

export class CadApp {
  readonly engine: CadEngine;
  readonly ui = new UiStore();
  readonly loop: FrameLoop;
  readonly #host: HostServices;
  /** Draws a frame. Set while a canvas is attached. */
  #render: (() => void) | undefined;

  constructor(engine: CadEngine, options: CadAppOptions = {}) {
    this.engine = engine;
    this.#host = options.host ?? browserHost;
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
    this.#render = undefined;
    this.engine.dispose();
  }

  #onFrame(dirty: number): void {
    // The UI state first: drawing reads it — whether lineweights are shown —
    // and must see what this frame's input changed, not the frame before's.
    if (dirty & Dirty.Ui) this.ui.refresh(this.engine);
    if (dirty & RENDER_FLAGS) this.#render?.();
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
    if (file) this.loop.invalidate(this.engine.loadDxf(file.bytes, file.name));
  }

  #exportFile(): void {
    const state = this.engine.uiState();
    // DESIGN §24.2: never imply a lossless round trip when content was skipped.
    if (state.exportIsLossy && !this.#host.confirm(LOSSY_EXPORT_MESSAGE)) return;
    this.#host.saveFile(
      state.documentName ?? DEFAULT_FILE_NAME,
      this.engine.exportDxf(),
      DXF_MIME_TYPE,
    );
    this.loop.invalidate(Dirty.Ui);
  }
}
