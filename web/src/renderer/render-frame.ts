// The frame shape from DESIGN §12.4: committed geometry is uploaded only when
// its revision changed, and the style table only when its own did (ADR-0014);
// the small transient overlay is refreshed separately; everything else is a
// transform update and a redraw.
import type { ClipTransform, FrameBuffers, StyleTables } from "../cad/engine";

/** What a frame needs from the CAD engine. */
export interface FrameSource {
  readonly sceneRevision: number;
  readScene<T>(consume: (frame: FrameBuffers) => T): T;
  readonly styleRevision: number;
  readStyles<T>(consume: (tables: StyleTables) => T): T;
  sceneTransform(): ClipTransform;
  readOverlay<T>(consume: (frame: FrameBuffers) => T): T;
  overlayTransform(): ClipTransform;
}

/** What a frame needs from a GPU adapter. */
export interface FrameSink {
  /** Revision of the committed scene currently on the GPU, if any. */
  readonly sceneRevision: number | undefined;
  updateScene(revision: number, frame: FrameBuffers): void;
  /** Revision of the style table currently on the GPU, if any. */
  readonly styleRevision: number | undefined;
  updateStyles(revision: number, tables: StyleTables): void;
  updateOverlay(frame: FrameBuffers): void;
  draw(sceneTransform: ClipTransform, overlayTransform: ClipTransform): void;
}

/**
 * Draws one frame. The number of calls into the engine is constant: it does
 * not depend on how many entities the drawing holds.
 */
export function renderFrame(source: FrameSource, sink: FrameSink): void {
  const revision = source.sceneRevision;
  if (revision !== sink.sceneRevision) {
    source.readScene((frame) => sink.updateScene(revision, frame));
  }
  const styles = source.styleRevision;
  if (styles !== sink.styleRevision) {
    source.readStyles((tables) => sink.updateStyles(styles, tables));
  }
  source.readOverlay((frame) => sink.updateOverlay(frame));
  sink.draw(source.sceneTransform(), source.overlayTransform());
}
