import { describe, expect, test } from "vitest";
import type { FrameBuffers, StyleTables } from "../cad/engine";
import { renderFrame, type FrameSink, type FrameSource } from "./render-frame";

const emptyFrame = (): FrameBuffers => ({
  segments: new Float32Array(4),
  batches: new Uint32Array(3),
  arcs: new Float32Array(5),
  arcBatches: new Uint32Array(3),
});

class FakeSource implements FrameSource {
  sceneRevision = 1;
  styleRevision = 1;
  calls = 0;
  sceneReads = 0;
  styleReads = 0;

  readStyles<T>(consume: (tables: StyleTables) => T): T {
    this.calls += 1;
    this.styleReads += 1;
    return consume({ styles: new Uint8Array(8), patterns: new Float32Array(10) });
  }

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
  styleRevision: number | undefined;
  styleUploads = 0;
  sceneUploads = 0;

  updateStyles(revision: number): void {
    this.styleRevision = revision;
    this.styleUploads += 1;
  }

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

  // ADR-0014: the style table has a lifetime of its own.
  test("uploads the style table once, and again only when its revision changes", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    for (let frame = 0; frame < 10; frame += 1) renderFrame(source, sink);
    expect(sink.styleUploads).toBe(1);
    expect(source.styleReads).toBe(1);

    // A layer was recoloured: the table changed, the geometry did not.
    source.styleRevision = 2;
    renderFrame(source, sink);
    renderFrame(source, sink);
    expect(sink.styleUploads).toBe(2);
    expect(sink.styleRevision).toBe(2);
    expect(sink.sceneUploads).toBe(1);
    expect(source.sceneReads).toBe(1);
  });

  test("an edit uploads both the geometry and the table", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    source.sceneRevision = 2;
    source.styleRevision = 2;
    renderFrame(source, sink);
    expect([sink.sceneUploads, sink.styleUploads]).toEqual([2, 2]);
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
