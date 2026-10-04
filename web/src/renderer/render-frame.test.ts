import { describe, expect, test } from "vitest";
import type { FrameBuffers } from "../cad/engine";
import { renderFrame, type FrameSink, type FrameSource } from "./render-frame";

const emptyFrame = (): FrameBuffers => ({
  segments: new Float32Array(4),
  batches: new Uint32Array(3),
});

class FakeSource implements FrameSource {
  sceneRevision = 1;
  calls = 0;
  sceneReads = 0;

  readScene<T>(consume: (frame: FrameBuffers) => T): T {
    this.calls += 1;
    this.sceneReads += 1;
    return consume(emptyFrame());
  }

  readOverlay<T>(consume: (frame: FrameBuffers) => T): T {
    this.calls += 1;
    return consume(emptyFrame());
  }

  sceneTransform(): Float32Array {
    this.calls += 1;
    return new Float32Array([1, 1, 0, 0]);
  }

  overlayTransform(): Float32Array {
    this.calls += 1;
    return new Float32Array([1, 1, 0, 0]);
  }
}

class FakeSink implements FrameSink {
  sceneRevision: number | undefined;
  sceneUploads = 0;
  overlayUploads = 0;
  draws = 0;

  updateScene(revision: number): void {
    this.sceneRevision = revision;
    this.sceneUploads += 1;
  }

  updateOverlay(): void {
    this.overlayUploads += 1;
  }

  draw(): void {
    this.draws += 1;
  }
}

describe("renderFrame", () => {
  test("uploads the committed scene on the first frame", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    expect(sink.sceneUploads).toBe(1);
    expect(sink.sceneRevision).toBe(1);
    expect(sink.draws).toBe(1);
  });

  test("does not re-upload the scene while its revision is unchanged", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    for (let frame = 0; frame < 100; frame += 1) renderFrame(source, sink);
    expect(sink.sceneUploads).toBe(1);
    expect(source.sceneReads).toBe(1);
    expect(sink.draws).toBe(100);
  });

  test("re-uploads the scene exactly once when the revision changes", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    source.sceneRevision = 2;
    renderFrame(source, sink);
    renderFrame(source, sink);
    expect(sink.sceneUploads).toBe(2);
    expect(sink.sceneRevision).toBe(2);
  });

  test("refreshes the overlay every frame, independently of the scene", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    for (let frame = 0; frame < 10; frame += 1) renderFrame(source, sink);
    expect(sink.overlayUploads).toBe(10);
    expect(sink.sceneUploads).toBe(1);
  });

  test("a steady-state frame makes a constant number of engine calls", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    const afterFirst = source.calls;
    renderFrame(source, sink);
    // Overlay read, scene transform, overlay transform — regardless of drawing size.
    expect(source.calls - afterFirst).toBe(3);
  });
});
