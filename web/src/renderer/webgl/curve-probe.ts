// The curve-rendering experiment (DESIGN §12.5, §23.3; ADR-0009).
//
// Draws the same generated circles and arcs two ways on an off-screen canvas
// and reports cost and visual agreement:
//
//   A. zoom-adaptive tessellation: every arc becomes straight segments within
//      a chord-error tolerance at the current zoom, drawn by the stroke shader;
//   B. analytic arcs: one instance per arc, evaluated exactly in the shader.
//
// Loaded on demand under `?debug`; not part of the application's render path.
import { FLOATS_PER_ARC, FLOATS_PER_SEGMENT } from "../../cad/engine";
import { ARC_FRAGMENT_SHADER, ARC_STRIP_VERTICES, ARC_VERTEX_SHADER } from "./arc-shaders";
import { linkProgram } from "./program";
import { STROKE_FRAGMENT_SHADER, STROKE_VERTEX_SHADER } from "./shaders";

const WIDTH = 1280;
const HEIGHT = 800;
const HALF_WIDTH_PX = 0.75;
const TAU = Math.PI * 2;
/** Chord error a zoom-adaptive tessellator would aim for, in pixels. */
const WORKING_TOLERANCE_PX = 0.25;
/** Chord error of the reference image, far below what the eye can see. */
const REFERENCE_TOLERANCE_PX = 0.01;
/** More segments than this is not uploaded; the count is reported instead. */
const MAX_SEGMENTS = 600_000;

interface Arc {
  cx: number;
  cy: number;
  radius: number;
  start: number;
  sweep: number;
}

interface ViewSpec {
  name: string;
  /** Pixels per world unit. */
  scale: number;
  centerX: number;
  centerY: number;
}

export interface CurveViewReport {
  view: string;
  /** Largest on-screen radius among the generated arcs, in pixels. */
  maxRadiusPx: number;
  analytic: { bytes: number; drawMs: number };
  tessellated:
    | { segments: number; bytes: number; tessellateMs: number; uploadMs: number; drawMs: number }
    | { segments: number; bytes: number; skipped: string };
  /** Analytic image against a finely tessellated reference of the same arcs. */
  agreement:
    { meanAbsoluteDifference: number; mismatchedPixels: number; litPixels: number } | undefined;
}

export interface CurveReport {
  arcs: number;
  views: CurveViewReport[];
}

/** SplitMix-style generator so every run draws the same arcs. */
function random(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x9e3779b9) >>> 0;
    let z = state;
    z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
    z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
    return ((z ^ (z >>> 16)) >>> 0) / 0x100000000;
  };
}

function generateArcs(count: number, side: number): Arc[] {
  const next = random(2026);
  return Array.from({ length: count }, () => {
    const full = next() < 0.5;
    return {
      cx: next() * side,
      cy: next() * side,
      radius: 1 + next() * 39,
      start: next() * TAU,
      sweep: full ? TAU : 0.2 + next() * (TAU - 0.4),
    };
  });
}

/** Segments needed so the chord never strays more than `tolerance` pixels from the arc. */
function segmentsFor(radiusPx: number, sweep: number, tolerance: number): number {
  if (radiusPx <= tolerance) return Math.max(3, Math.ceil(sweep / (TAU / 3)));
  const step = 2 * Math.acos(1 - tolerance / radiusPx);
  return Math.max(2, Math.ceil(sweep / step));
}

function countSegments(arcs: Arc[], scale: number, tolerance: number): number {
  return arcs.reduce(
    (total, arc) => total + segmentsFor(arc.radius * scale, arc.sweep, tolerance),
    0,
  );
}

/** Flat `x0, y0, x1, y1` world-space segments approximating every arc. */
function tessellate(arcs: Arc[], scale: number, tolerance: number): Float32Array<ArrayBuffer> {
  const out = new Float32Array(countSegments(arcs, scale, tolerance) * FLOATS_PER_SEGMENT);
  let offset = 0;
  for (const arc of arcs) {
    const count = segmentsFor(arc.radius * scale, arc.sweep, tolerance);
    let x = arc.cx + arc.radius * Math.cos(arc.start);
    let y = arc.cy + arc.radius * Math.sin(arc.start);
    for (let index = 1; index <= count; index += 1) {
      const angle = arc.start + (arc.sweep * index) / count;
      const nextX = arc.cx + arc.radius * Math.cos(angle);
      const nextY = arc.cy + arc.radius * Math.sin(angle);
      out[offset] = x;
      out[offset + 1] = y;
      out[offset + 2] = nextX;
      out[offset + 3] = nextY;
      offset += FLOATS_PER_SEGMENT;
      x = nextX;
      y = nextY;
    }
  }
  return out;
}

/** Browsers clamp `performance.now()` to about 0.1 ms; time batches. */
function time(run: () => void, minimumMs = 60): number {
  run();
  for (let batch = 1; ; batch *= 2) {
    const start = performance.now();
    for (let index = 0; index < batch; index += 1) run();
    const elapsed = performance.now() - start;
    if (elapsed >= minimumMs || batch >= 1 << 16) return elapsed / batch;
  }
}

export function probeCurves(arcCount: number): CurveReport {
  const canvas = document.createElement("canvas");
  canvas.width = WIDTH;
  canvas.height = HEIGHT;
  const context = canvas.getContext("webgl2", { alpha: false, antialias: false });
  if (!context) throw new Error("WebGL2 is not available");
  const gl = context;

  // Constant density: on average one arc per 20×20 units, radii up to 40.
  const side = 20 * Math.sqrt(arcCount);
  const arcs = generateArcs(arcCount, side);
  const target = arcs.reduce((largest, arc) => (arc.radius > largest.radius ? arc : largest));
  const onTarget = target.start + target.sweep / 2;

  const strokes = linkProgram(gl, STROKE_VERTEX_SHADER, STROKE_FRAGMENT_SHADER);
  const analytic = linkProgram(gl, ARC_VERTEX_SHADER, ARC_FRAGMENT_SHADER);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl.viewport(0, 0, WIDTH, HEIGHT);

  // ── B: one instance per arc; uploaded once, never again ──
  const arcData = new Float32Array(arcs.length * FLOATS_PER_ARC);
  arcs.forEach((arc, index) =>
    arcData.set([arc.cx, arc.cy, arc.radius, arc.start, arc.sweep], index * FLOATS_PER_ARC),
  );
  const arcVao = gl.createVertexArray();
  const arcBuffer = gl.createBuffer();
  gl.bindVertexArray(arcVao);
  gl.bindBuffer(gl.ARRAY_BUFFER, arcBuffer);
  gl.bufferData(gl.ARRAY_BUFFER, arcData, gl.STATIC_DRAW);
  const stride = FLOATS_PER_ARC * 4;
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 4, gl.FLOAT, false, stride, 0);
  gl.vertexAttribDivisor(0, 1);
  gl.enableVertexAttribArray(1);
  gl.vertexAttribPointer(1, 1, gl.FLOAT, false, stride, 16);
  gl.vertexAttribDivisor(1, 1);

  // ── A: instanced segments, as the application draws lines ──
  const segmentVao = gl.createVertexArray();
  const quad = gl.createBuffer();
  const segmentBuffer = gl.createBuffer();
  gl.bindVertexArray(segmentVao);
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, segmentBuffer);
  gl.enableVertexAttribArray(1);
  gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);
  gl.vertexAttribDivisor(1, 1);
  gl.bindVertexArray(null);

  // Chromium queues GL commands for another process; `gl.finish()` returns
  // before they run. Reading a pixel back is what actually waits for them.
  const pixel = new Uint8Array(4);
  const sync = () => gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);

  const setUniforms = (program: WebGLProgram, view: ViewSpec) => {
    gl.useProgram(program);
    const location = (name: string) => gl.getUniformLocation(program, name);
    const scaleX = (2 * view.scale) / WIDTH;
    const scaleY = (2 * view.scale) / HEIGHT;
    gl.uniform2f(location("u_scale"), scaleX, scaleY);
    gl.uniform2f(location("u_translate"), -view.centerX * scaleX, -view.centerY * scaleY);
    gl.uniform2f(location("u_viewport"), WIDTH, HEIGHT);
    gl.uniform1f(location("u_half_width"), HALF_WIDTH_PX);
    gl.uniform4f(location("u_color"), 1, 1, 1, 1);
  };
  const clear = () => {
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
  };
  const drawAnalytic = (view: ViewSpec) => {
    clear();
    setUniforms(analytic, view);
    gl.bindVertexArray(arcVao);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, ARC_STRIP_VERTICES, arcs.length);
    sync();
  };
  const uploadSegments = (segments: Float32Array) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, segmentBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, segments, gl.DYNAMIC_DRAW);
    sync();
  };
  const drawSegments = (view: ViewSpec, count: number) => {
    clear();
    setUniforms(strokes, view);
    gl.bindVertexArray(segmentVao);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);
    sync();
  };
  const readLuminance = (): Uint8Array => {
    const pixels = new Uint8Array(WIDTH * HEIGHT * 4);
    gl.readPixels(0, 0, WIDTH, HEIGHT, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    const luminance = new Uint8Array(WIDTH * HEIGHT);
    for (let index = 0; index < luminance.length; index += 1)
      luminance[index] = pixels[index * 4] ?? 0;
    return luminance;
  };

  const views: ViewSpec[] = [
    // The whole drawing: thousands of tiny circles.
    { name: "overview", scale: HEIGHT / side, centerX: side / 2, centerY: side / 2 },
    // Ordinary work: radii from a few to about 160 pixels, heavy overlap.
    { name: "working", scale: 4, centerX: side / 2, centerY: side / 2 },
    // Deep zoom on the edge of the largest circle: its radius is tens of thousands of pixels.
    {
      name: "deep",
      scale: 2000,
      centerX: target.cx + target.radius * Math.cos(onTarget),
      centerY: target.cy + target.radius * Math.sin(onTarget),
    },
  ];

  const reports = views.map((view): CurveViewReport => {
    const analyticDrawMs = time(() => drawAnalytic(view));
    drawAnalytic(view);
    const analyticImage = readLuminance();

    const segmentCount = countSegments(arcs, view.scale, WORKING_TOLERANCE_PX);
    const bytes = segmentCount * FLOATS_PER_SEGMENT * 4;
    let tessellated: CurveViewReport["tessellated"];
    if (segmentCount > MAX_SEGMENTS) {
      tessellated = {
        segments: segmentCount,
        bytes,
        skipped: "too many segments to allocate without culling",
      };
    } else {
      let segments = new Float32Array(0);
      const tessellateMs = time(() => {
        segments = tessellate(arcs, view.scale, WORKING_TOLERANCE_PX);
      });
      const uploadMs = time(() => uploadSegments(segments));
      const drawMs = time(() => drawSegments(view, segmentCount));
      tessellated = { segments: segmentCount, bytes, tessellateMs, uploadMs, drawMs };
    }

    // Agreement with a reference tessellated far finer than the eye resolves.
    let agreement: CurveViewReport["agreement"];
    const visible = arcs.filter((arc) => {
      const reach = arc.radius + 2 / view.scale;
      return (
        Math.abs(arc.cx - view.centerX) - reach < WIDTH / 2 / view.scale &&
        Math.abs(arc.cy - view.centerY) - reach < HEIGHT / 2 / view.scale
      );
    });
    const referenceCount = countSegments(visible, view.scale, REFERENCE_TOLERANCE_PX);
    if (referenceCount <= MAX_SEGMENTS) {
      // Rebase onto the view centre so f32 keeps sub-pixel precision at deep zoom.
      const rebased = visible.map((arc) => ({
        ...arc,
        cx: arc.cx - view.centerX,
        cy: arc.cy - view.centerY,
      }));
      uploadSegments(tessellate(rebased, view.scale, REFERENCE_TOLERANCE_PX));
      drawSegments({ ...view, centerX: 0, centerY: 0 }, referenceCount);
      const reference = readLuminance();
      let total = 0;
      let mismatched = 0;
      let lit = 0;
      for (let index = 0; index < reference.length; index += 1) {
        const a = analyticImage[index] ?? 0;
        const b = reference[index] ?? 0;
        total += Math.abs(a - b);
        if (b > 128) lit += 1;
        if ((a > 160 && b < 64) || (b > 160 && a < 64)) mismatched += 1;
      }
      agreement = {
        meanAbsoluteDifference: total / reference.length,
        mismatchedPixels: mismatched,
        litPixels: lit,
      };
    }

    return {
      view: view.name,
      maxRadiusPx: Math.round(target.radius * view.scale),
      analytic: { bytes: arcData.byteLength, drawMs: analyticDrawMs },
      tessellated,
      agreement,
    };
  });

  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return { arcs: arcCount, views: reports };
}
