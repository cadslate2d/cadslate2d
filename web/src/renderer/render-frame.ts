// The frame shape from DESIGN §12.4: committed geometry is uploaded only when
// its revision changed; everything else is a transform update and a redraw.
import type { ClipTransform, FrameBuffers } from "../cad/engine";

/** What a frame needs from the CAD engine. */
export interface FrameSource {
  readonly sceneRevision: number;
  readScene<T>(consume: (frame: FrameBuffers) => T): T;
  sceneTransform(): ClipTransform;
}

/** What a frame needs from a GPU adapter. */
export interface FrameSink {
  /** Revision of the committed scene currently on the GPU, if any. */
  readonly sceneRevision: number | undefined;
  updateScene(revision: number, frame: FrameBuffers): void;
  draw(sceneTransform: ClipTransform): void;
}

export function renderFrame(source: FrameSource, sink: FrameSink): void {
  const revision = source.sceneRevision;
  if (revision !== sink.sceneRevision) {
    source.readScene((frame) => sink.updateScene(revision, frame));
  }
  sink.draw(source.sceneTransform());
}
