// Frame measurements on a drawing (DESIGN §42, #171): the costs a person
// feels, through the application's own engine and renderer. Loaded on
// demand, only when the page is opened with `?debug`.
//
// Each figure is the time of one frame — the engine call, the frame's
// reads and uploads, the draw calls, and waiting for the GPU to finish
// them — in milliseconds, over a run of frames of one kind.
import {
  FLOATS_PER_ARC,
  FLOATS_PER_SEGMENT,
  FLOATS_PER_TRIANGLE,
  type CadEngine,
} from "../../cad/engine";
import { renderFrame } from "../render-frame";
import type { WebGlRenderer } from "./webgl-renderer";

/** Milliseconds over a run of frames. */
export interface FrameTimes {
  frames: number;
  mean: number;
  max: number;
}

export interface FramesReport {
  /** The import report's first line: what was opened. */
  imported: string;
  /** Committed instances of each kind: what every frame draws. */
  instances: { segments: number; arcs: number; triangles: number };
  /** Reading the DXF into the engine: parse, build the drawing and its index. */
  openMs: number;
  /** The first frame after opening: the whole scene packed, uploaded, and drawn. */
  firstFrameMs: number;
  /** Frames while the middle button drags the view. */
  pan: FrameTimes;
  /** Frames while the wheel zooms in and out about the pointer. */
  zoom: FrameTimes;
  /** Frames while the pointer moves with nothing pressed: hover and snapping. */
  pointer: FrameTimes;
  /** The frame after one committed edit — a line drawn — which rebuilds the scene. */
  edit: FrameTimes;
}

const MIDDLE_BUTTON = 1;

function timed(
  renderer: WebGlRenderer,
  engine: CadEngine,
  frames: number,
  step: (i: number) => void,
): FrameTimes {
  let total = 0;
  let max = 0;
  for (let i = 0; i < frames; i += 1) {
    step(i);
    const start = performance.now();
    renderFrame(engine, renderer);
    renderer.waitForGpu();
    const elapsed = performance.now() - start;
    total += elapsed;
    max = Math.max(max, elapsed);
  }
  return { frames, mean: total / frames, max };
}

/**
 * Opens `text` in the application's engine and measures its frames with
 * the application's renderer. The drawing on screen is replaced.
 */
export function probeFrames(
  engine: CadEngine,
  renderer: WebGlRenderer,
  text: string,
  frames = 60,
): FramesReport {
  const bytes = new TextEncoder().encode(text);
  const openStart = performance.now();
  engine.loadDxf(bytes, "mixed.dxf");
  const openMs = performance.now() - openStart;
  const imported = engine.uiState().importReport?.lines[0] ?? "";
  const instances = engine.readScene((frame) => ({
    segments: frame.segments.length / FLOATS_PER_SEGMENT,
    arcs: frame.arcs.length / FLOATS_PER_ARC,
    triangles: frame.triangles.length / FLOATS_PER_TRIANGLE,
  }));

  const firstStart = performance.now();
  renderFrame(engine, renderer);
  renderer.waitForGpu();
  const firstFrameMs = performance.now() - firstStart;

  const FRAMES = frames;
  engine.pointerDown(400, 300, MIDDLE_BUTTON, 0);
  const pan = timed(renderer, engine, FRAMES, (i) => {
    engine.pointerMove(400 + i * 3, 300 + (i % 7), 0);
  });
  engine.pointerUp(400 + FRAMES * 3, 300, MIDDLE_BUTTON, 0);
  const zoom = timed(renderer, engine, FRAMES, (i) => {
    engine.wheel(400, 300, i < FRAMES / 2 ? -100 : 100, 0);
  });
  const pointer = timed(renderer, engine, FRAMES, (i) => {
    engine.pointerMove(100 + ((i * 13) % 600), 100 + ((i * 7) % 400), 0);
  });
  const edit = timed(renderer, engine, Math.max(1, Math.round(frames / 6)), (i) => {
    engine.submitText("L");
    engine.submitText(`${i},0`);
    engine.submitText(`${i},1`);
    engine.submitText("");
  });
  return { imported, instances, openMs, firstFrameMs, pan, zoom, pointer, edit };
}
