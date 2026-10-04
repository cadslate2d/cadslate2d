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
  /** Mean milliseconds per operation. */
  ms: {
    /** Waiting for the GPU process with nothing queued: the floor under every figure that uploads. */
    waitForGpu: number;
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

/** Browsers clamp `performance.now()` to about 0.1 ms, so single operations cannot be timed. */
const MIN_TIMED_MS = 40;

/** Mean milliseconds per run, timing ever larger batches until the clock can resolve them. */
function time(run: () => void): number {
  run(); // warm up
  for (let batch = 1; ; batch *= 2) {
    const start = performance.now();
    for (let index = 0; index < batch; index += 1) run();
    const elapsed = performance.now() - start;
    if (elapsed >= MIN_TIMED_MS || batch >= 1 << 20) return elapsed / batch;
  }
}

interface ChromiumMemory {
  readonly usedJSHeapSize: number;
}

function heapSize(): number | undefined {
  return (performance as Performance & { memory?: ChromiumMemory }).memory?.usedJSHeapSize;
}

export function probeTransport(engine: CadEngine): TransportReport {
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

  // Chromium queues GL commands for another process; `gl.finish()` returns
  // before they run. Reading a pixel back is what actually waits for them, so
  // the timed region includes the real upload and not just queueing it.
  const pixel = new Uint8Array(4);
  const sync = () => gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
  const upload = (buffer: WebGLBuffer, data: Float32Array) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, data);
    sync();
  };

  const ms = {
    waitForGpu: time(sync),
    pullViews: time(() =>
      engine.readScene((frame) => frame.segments.length + frame.batches.length),
    ),
    pullAndCopy: time(() => engine.readScene((frame) => frame.segments.slice().length)),
    uploadFromViews: time(() => engine.readScene((frame) => upload(scene, frame.segments))),
    uploadFromCopy: time(() => engine.readScene((frame) => upload(scene, frame.segments.slice()))),
    uploadExpanded: time(() => upload(wide, expanded)),
    uploadWithRealloc: time(() =>
      engine.readScene((frame) => {
        gl.bindBuffer(gl.ARRAY_BUFFER, scene);
        gl.bufferData(gl.ARRAY_BUFFER, frame.segments, gl.STATIC_DRAW);
        sync();
      }),
    ),
    pointerFrame: 0,
    editAndReupload: 0,
  };

  // Pointer frames: the committed scene is not touched at all.
  let step = 0;
  const pointerFrame = () => {
    step += 1;
    engine.pointerMove(100 + (step % 400), 100 + (step % 300), 0);
    engine.readOverlay((frame) => upload(overlay, frame.segments));
    engine.overlayTransform();
    engine.sceneTransform();
  };
  ms.pointerFrame = time(pointerFrame);

  // Heap growth over many frames. Needs --enable-precise-memory-info to be meaningful.
  const HEAP_FRAMES = 20_000;
  const heapBefore = heapSize();
  for (let frame = 0; frame < HEAP_FRAMES; frame += 1) pointerFrame();
  const heapAfter = heapSize();

  // An edit: one new line, then the whole committed scene is rebuilt and re-uploaded.
  let editStep = 0;
  ms.editAndReupload = time(() => {
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
        : (heapAfter - heapBefore) / HEAP_FRAMES,
  };
}

/** Milliseconds to import a DXF file into a fresh session: parse, build the drawing and its index. */
export function probeOpen(createEngine: () => CadEngine, text: string): number {
  const bytes = new TextEncoder().encode(text);
  const engine = createEngine();
  engine.resize(800, 600);
  const start = performance.now();
  engine.loadDxf(bytes, "probe.dxf");
  const elapsed = performance.now() - start;
  engine.dispose();
  return elapsed;
}
