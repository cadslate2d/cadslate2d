// The application object: one engine session, one UI store, one frame loop.
// React components and the canvas binding talk to the engine only through it.
import { Dirty, type CadEngine, type KeyAction } from "../cad/engine";
import { FrameLoop, type ScheduleFrame } from "./frame-loop";
import { UiStore } from "./ui-store";

const RENDER_FLAGS = Dirty.Scene | Dirty.Overlay | Dirty.View;

export class CadApp {
  readonly engine: CadEngine;
  readonly ui = new UiStore();
  readonly loop: FrameLoop;
  /** Draws a frame. Set while a canvas is attached. */
  #render: (() => void) | undefined;

  constructor(
    engine: CadEngine,
    schedule: ScheduleFrame = (callback) => requestAnimationFrame(callback),
  ) {
    this.engine = engine;
    this.loop = new FrameLoop(
      (sample) => engine.pointerMove(sample.x, sample.y, sample.modifiers),
      (dirty) => this.#onFrame(dirty),
      schedule,
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
    this.loop.invalidate(this.engine.submitText(text));
  }

  key(action: KeyAction): void {
    this.loop.flushPointerMove();
    this.loop.invalidate(this.engine.key(action));
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
    if (dirty & RENDER_FLAGS) this.#render?.();
    if (dirty & Dirty.Ui) this.ui.refresh(this.engine);
  }
}
