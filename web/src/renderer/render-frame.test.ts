import { describe, expect, test } from "vitest";
import type { FrameBuffers } from "../cad/engine";
import { renderFrame, type FrameSink, type FrameSource } from "./render-frame";

class FakeSource implements FrameSource {
  sceneRevision = 1;
  calls = { readScene: 0, sceneTransform: 0 };

  readScene<T>(consume: (frame: FrameBuffers) => T): T {
    this.calls.readScene += 1;
    return consume({ segments: new Float32Array(4), batches: new Uint32Array(3) });
  }

  sceneTransform(): Float32Array {
    this.calls.sceneTransform += 1;
    return new Float32Array([1, 1, 0, 0]);
  }
}

class FakeSink implements FrameSink {
  sceneRevision: number | undefined;
  uploads = 0;
  draws = 0;

  updateScene(revision: number): void {
    this.sceneRevision = revision;
    this.uploads += 1;
  }

  draw(): void {
    this.draws += 1;
  }
}

describe("renderFrame", () => {
  test("uploads the committed scene on the first frame", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    expect(sink.uploads).toBe(1);
    expect(sink.sceneRevision).toBe(1);
    expect(sink.draws).toBe(1);
  });

  test("does not re-upload while the revision is unchanged", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    for (let frame = 0; frame < 100; frame += 1) renderFrame(source, sink);
    expect(sink.uploads).toBe(1);
    expect(source.calls.readScene).toBe(1);
    expect(sink.draws).toBe(100);
  });

  test("re-uploads exactly once when the revision changes", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    renderFrame(source, sink);
    source.sceneRevision = 2;
    renderFrame(source, sink);
    renderFrame(source, sink);
    expect(sink.uploads).toBe(2);
    expect(sink.sceneRevision).toBe(2);
  });

  test("pulls the transform once per frame", () => {
    const [source, sink] = [new FakeSource(), new FakeSink()];
    for (let frame = 0; frame < 10; frame += 1) renderFrame(source, sink);
    expect(source.calls.sceneTransform).toBe(10);
  });
});
