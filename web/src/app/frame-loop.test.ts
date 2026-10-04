import { describe, expect, test } from "vitest";
import { FrameLoop, type PointerSample } from "./frame-loop";

function harness(moveDirty = 0b10) {
  const scheduled: (() => void)[] = [];
  const moves: PointerSample[] = [];
  const frames: number[] = [];
  const loop = new FrameLoop(
    (sample) => {
      moves.push(sample);
      return moveDirty;
    },
    (dirty) => frames.push(dirty),
    (callback) => scheduled.push(callback),
  );
  const tick = () => scheduled.splice(0).forEach((callback) => callback());
  return { loop, scheduled, moves, frames, tick };
}

const sample = (x: number): PointerSample => ({ x, y: 0, modifiers: 0 });

describe("FrameLoop", () => {
  test("schedules nothing when nothing is dirty", () => {
    const { loop, scheduled } = harness();
    loop.invalidate(0);
    expect(scheduled).toHaveLength(0);
  });

  test("merges several invalidations into one frame", () => {
    const { loop, scheduled, frames, tick } = harness();
    loop.invalidate(0b001);
    loop.invalidate(0b100);
    expect(scheduled).toHaveLength(1);
    tick();
    expect(frames).toEqual([0b101]);
  });

  test("coalesces pointer moves to the latest position, once per frame", () => {
    const { loop, moves, frames, tick } = harness();
    loop.queuePointerMove(sample(1));
    loop.queuePointerMove(sample(2));
    loop.queuePointerMove(sample(3));
    tick();
    expect(moves).toEqual([sample(3)]);
    expect(frames).toEqual([0b10]);
  });

  test("a pointer move that changes nothing produces no frame", () => {
    const { loop, moves, frames, tick } = harness(0);
    loop.queuePointerMove(sample(1));
    tick();
    expect(moves).toHaveLength(1);
    expect(frames).toHaveLength(0);
  });

  test("flushing delivers a queued move before a discrete event", () => {
    const { loop, moves, tick, frames } = harness();
    loop.queuePointerMove(sample(7));
    loop.flushPointerMove();
    expect(moves).toEqual([sample(7)]);
    loop.flushPointerMove();
    expect(moves).toHaveLength(1);
    tick();
    expect(frames).toEqual([0b10]);
  });

  test("schedules again after a frame has run", () => {
    const { loop, scheduled, frames, tick } = harness();
    loop.invalidate(1);
    tick();
    loop.invalidate(4);
    expect(scheduled).toHaveLength(1);
    tick();
    expect(frames).toEqual([1, 4]);
  });
});
