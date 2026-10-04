// Turns dirty flags into at most one frame per animation tick, and coalesces
// pointer movement to one engine call per frame (ADR-0002 §2).

export interface PointerSample {
  readonly x: number;
  readonly y: number;
  readonly modifiers: number;
}

export type ScheduleFrame = (callback: () => void) => void;

export class FrameLoop {
  readonly #movePointer: (sample: PointerSample) => number;
  readonly #onFrame: (dirty: number) => void;
  readonly #schedule: ScheduleFrame;
  #dirty = 0;
  #pendingMove: PointerSample | undefined;
  #scheduled = false;

  /**
   * @param movePointer forwards a pointer move to the engine and returns its dirty flags
   * @param onFrame     runs once per scheduled frame with the accumulated dirty flags
   * @param schedule    `requestAnimationFrame`, or a fake in tests
   */
  constructor(
    movePointer: (sample: PointerSample) => number,
    onFrame: (dirty: number) => void,
    schedule: ScheduleFrame,
  ) {
    this.#movePointer = movePointer;
    this.#onFrame = onFrame;
    this.#schedule = schedule;
  }

  /** Records that something changed. No flags, no frame. */
  invalidate(dirty: number): void {
    if (dirty === 0) return;
    this.#dirty |= dirty;
    this.#request();
  }

  /** Remembers the latest pointer position; it reaches the engine once per frame. */
  queuePointerMove(sample: PointerSample): void {
    this.#pendingMove = sample;
    this.#request();
  }

  /**
   * Delivers a queued pointer move now. Call before forwarding a discrete
   * event (button, key, wheel) so the engine sees events in order.
   */
  flushPointerMove(): void {
    const sample = this.#pendingMove;
    if (sample === undefined) return;
    this.#pendingMove = undefined;
    this.#dirty |= this.#movePointer(sample);
  }

  #request(): void {
    if (this.#scheduled) return;
    this.#scheduled = true;
    this.#schedule(() => {
      this.#scheduled = false;
      this.flushPointerMove();
      const dirty = this.#dirty;
      this.#dirty = 0;
      if (dirty !== 0) this.#onFrame(dirty);
    });
  }
}
