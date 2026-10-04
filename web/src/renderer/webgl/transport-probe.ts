// Measures the render transport (DESIGN §12.6). Loaded on demand, only when
// the page is opened with `?debug`; it is not part of the normal bundle path.
//
// It uses its own off-screen WebGL2 context, so measuring never disturbs the
// application's renderer, and it reads only bulk frame buffers — the same
// data the renderer gets.
import { FLOATS_PER_SEGMENT, type CadEngine } from "../../cad/engine";

export interface TransportReport {
  segments: number;
  /** Bytes of committed instance data (the M0 layout). */
  instancedBytes: number;
  /** Bytes the same scene would need as four expanded vertices per segment. */
  expandedBytes: number;
  /** Median milliseconds. */
  ms: {
    /** Create typed-array views over WASM memory; touch nothing. */
    pullViews: number;
    /** Create views and copy the segment data out of WASM memory. */
    pullAndCopy: number;
    /** Upload straight from the views into an existing GPU buffer. */
    uploadFromViews: number;
    /** Copy first, then upload the copy. */
    uploadFromCopy: number;
    /** Upload a buffer the size of the expanded-vertex layout. */
    uploadExpanded: number;
    /** Reallocate the GPU buffer on every upload instead of reusing it. */
    uploadWithRealloc: number;
    /** One pointer-move frame: engine call, overlay pull, overlay upload. */
    pointerFrame: number;
    /** Commit one entity, then rebuild and re-upload the committed scene. */
    editAndReupload: number;
  };
  /** JavaScript heap growth per pointer frame, in bytes, when the browser reports it. */
  heapBytesPerPointerFrame: number | undefined;
}

/** Floats per vertex if strokes were expanded on the CPU: both endpoints plus a corner id. */
const EXPANDED_FLOATS_PER_VERTEX = 5;
const EXPANDED_VERTICES_PER_SEGMENT = 4;

function median(samples: number[]): number {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

function time(repeats: number, run: () => void): number {
  const samples: number[] = [];
  for (let index = 0; index < repeats; index += 1) {
    const start = performance.now();
    run();
    samples.push(performance.now() - start);
  }
  return median(samples);
}

interface ChromiumMemory {
  readonly usedJSHeapSize: number;
}

function heapSize(): number | undefined {
  return (performance as Performance & { memory?: ChromiumMemory }).memory?.usedJSHeapSize;
}

export function probeTransport(engine: CadEngine, repeats = 15): TransportReport {
  const gl = document.createElement("canvas").getContext("webgl2");
  if (!gl) throw new Error("WebGL2 is not available");

  const segments = engine.readScene((frame) => frame.segments.length / FLOATS_PER_SEGMENT);
  const instancedBytes = segments * FLOATS_PER_SEGMENT * Float32Array.BYTES_PER_ELEMENT;
  const expanded = new Float32Array(
    segments * EXPANDED_VERTICES_PER_SEGMENT * EXPANDED_FLOATS_PER_VERTEX,
  );

  const scene = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, scene);
  gl.bufferData(gl.ARRAY_BUFFER, Math.max(instancedBytes, 16), gl.STATIC_DRAW);
  const wide = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, wide);
  gl.bufferData(gl.ARRAY_BUFFER, Math.max(expanded.byteLength, 16), gl.STATIC_DRAW);
  const overlay = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, overlay);
  gl.bufferData(gl.ARRAY_BUFFER, 4096, gl.DYNAMIC_DRAW);

  // gl.finish() makes the driver complete the upload inside the timed region.
  const upload = (buffer: WebGLBuffer, data: Float32Array) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, data);
    gl.finish();
  };

  const ms = {
    pullViews: time(repeats, () =>
      engine.readScene((frame) => frame.segments.length + frame.batches.length),
    ),
    pullAndCopy: time(repeats, () => engine.readScene((frame) => frame.segments.slice().length)),
    uploadFromViews: time(repeats, () =>
      engine.readScene((frame) => upload(scene, frame.segments)),
    ),
    uploadFromCopy: time(repeats, () =>
      engine.readScene((frame) => upload(scene, frame.segments.slice())),
    ),
    uploadExpanded: time(repeats, () => upload(wide, expanded)),
    uploadWithRealloc: time(repeats, () =>
      engine.readScene((frame) => {
        gl.bindBuffer(gl.ARRAY_BUFFER, scene);
        gl.bufferData(gl.ARRAY_BUFFER, frame.segments, gl.STATIC_DRAW);
        gl.finish();
      }),
    ),
    pointerFrame: 0,
    editAndReupload: 0,
  };

  // Pointer frames: the committed scene is not touched at all.
  const POINTER_FRAMES = 500;
  let step = 0;
  const pointerFrame = () => {
    step += 1;
    engine.pointerMove(100 + (step % 400), 100 + (step % 300), 0);
    engine.readOverlay((frame) => upload(overlay, frame.segments));
    engine.overlayTransform();
    engine.sceneTransform();
  };
  for (let warmUp = 0; warmUp < 50; warmUp += 1) pointerFrame();
  const heapBefore = heapSize();
  ms.pointerFrame = time(POINTER_FRAMES, pointerFrame);
  const heapAfter = heapSize();

  // An edit: one new line, then the whole committed scene is rebuilt and re-uploaded.
  let editStep = 0;
  ms.editAndReupload = time(Math.min(repeats, 7), () => {
    editStep += 1;
    engine.submitText("L");
    engine.submitText(`${editStep},0`);
    engine.submitText(`${editStep},1`);
    engine.submitText("");
    engine.readScene((frame) => upload(scene, frame.segments));
  });

  gl.getExtension("WEBGL_lose_context")?.loseContext();

  return {
    segments,
    instancedBytes,
    expandedBytes: expanded.byteLength,
    ms,
    heapBytesPerPointerFrame:
      heapBefore === undefined || heapAfter === undefined
        ? undefined
        : (heapAfter - heapBefore) / POINTER_FRAMES,
  };
}
