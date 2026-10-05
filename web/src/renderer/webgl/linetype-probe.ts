// The linetype experiment (ADR-0015). Loaded on demand, only when the page is
// opened with `?debug`; it is not part of the normal bundle path.
//
// Two ways to draw a dashed line, on the same generated geometry, in the
// probe's own WebGL2 context:
//
//   shader   one instance per segment, as for a continuous line, plus the
//            distance along its path at which it starts. The fragment shader
//            works out where along the pattern each pixel is.
//   split    every segment is cut into its dashes when the scene is built,
//            and each dash is an ordinary instance.
//
// For each: how many instances and bytes, how long the scene takes to build,
// and how long a frame takes at several zooms. `linetypePictures` draws a
// small scene both ways, for the eye.

const CANVAS_SIZE = 1024;
const MIN_TIMED_MS = 60;
/** The most dash-and-gap lengths a pattern may have. */
const MAX_PATTERN = 8;

export interface LinetypeCandidateReport {
  name: "shader" | "split";
  instances: number;
  bytes: number;
  /** Mean milliseconds to turn the paths into instance data. */
  buildMs: number;
  /** Mean milliseconds per frame, one per zoom in `LinetypeReport.zooms`. */
  frameMs: number[];
}

export interface LinetypeReport {
  segments: number;
  /** Dash and gap lengths, in drawing units. */
  pattern: number[];
  /** Pixels per drawing unit at which frames were timed. */
  zooms: number[];
  candidates: LinetypeCandidateReport[];
}

function time(run: () => void): number {
  run();
  for (let batch = 1; ; batch *= 2) {
    const start = performance.now();
    for (let index = 0; index < batch; index += 1) run();
    const elapsed = performance.now() - start;
    if (elapsed >= MIN_TIMED_MS || batch >= 1 << 16) return elapsed / batch;
  }
}

// One program serves both candidates: a segment with a start distance and a
// pattern. With `u_period` zero the pattern is off and the segment is solid,
// which is what the split candidate's dashes are.
const VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec4 a_segment;  // x0, y0, x1, y1 in drawing units
layout(location = 2) in float a_start;   // distance along the path at (x0, y0)
uniform vec2 u_origin;                    // drawing units at the canvas centre
uniform float u_scale;                    // pixels per drawing unit
uniform vec2 u_viewport;                  // pixels
uniform float u_half_width;               // pixels
out vec2 v_local;                         // pixels: along, across
out float v_length;                       // pixels
flat out float v_start;                   // drawing units

void main() {
  vec2 start = (a_segment.xy - u_origin) * u_scale;
  vec2 end = (a_segment.zw - u_origin) * u_scale;
  vec2 delta = end - start;
  float len = length(delta);
  vec2 along = len > 1e-6 ? delta / len : vec2(1.0, 0.0);
  vec2 across = vec2(-along.y, along.x);
  float extent = u_half_width + 1.0;
  float end_sign = a_corner.x * 2.0 - 1.0;
  vec2 position = mix(start, end, a_corner.x)
    + along * end_sign * extent + across * a_corner.y * extent;
  v_local = vec2(a_corner.x * len + end_sign * extent, a_corner.y * extent);
  v_length = len;
  v_start = a_start;
  gl_Position = vec4(position / (u_viewport * 0.5), 0.0, 1.0);
}
`;

const FRAGMENT = `#version 300 es
precision highp float;
uniform float u_half_width;               // pixels
uniform float u_scale;                    // pixels per drawing unit
uniform float u_pattern[${MAX_PATTERN}];  // dash, gap, dash, gap … in drawing units
uniform int u_pattern_count;
uniform float u_period;                   // sum of the pattern; 0 for a solid line
uniform vec4 u_color;
in vec2 v_local;
in float v_length;
flat in float v_start;
out vec4 out_color;

// How far, in pixels along the line, this fragment is from the nearest dash.
// Zero inside a dash.
float to_dash(float along_px) {
  float at = mod(v_start + along_px / u_scale, u_period);
  float nearest = 1e9;
  float from = 0.0;
  for (int index = 0; index < u_pattern_count; index += 2) {
    float dash = u_pattern[index];
    nearest = min(nearest, max(max(from - at, at - (from + dash)), 0.0));
    from += dash + u_pattern[index + 1];
  }
  // The first dash of the next period is a neighbour too.
  nearest = min(nearest, u_period - at);
  return nearest * u_scale;
}

void main() {
  float inside = clamp(v_local.x, 0.0, v_length);
  // The segment itself: a capsule, as for a continuous line.
  float distance_px = length(vec2(v_local.x - inside, v_local.y));
  // Dashes closer together than a few pixels cannot be told apart: the line
  // is drawn solid rather than as noise.
  if (u_period > 0.0 && u_period * u_scale >= 4.0) {
    // Each dash is a capsule of its own, so dashes have round ends and a
    // dash of no length is a dot.
    distance_px = max(distance_px, length(vec2(to_dash(inside), v_local.y)));
  }
  float coverage = clamp(u_half_width + 0.5 - distance_px, 0.0, 1.0);
  out_color = vec4(u_color.rgb * u_color.a * coverage, u_color.a * coverage);
}
`;

// An arc, drawn by a quad round its circle: enough for the pictures, where
// there are a handful. `a_start` is the distance along the path at the arc's
// first end, and the pattern runs on by arc length.
const ARC_VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_corner;    // (0|1, -1|+1), as for a segment
layout(location = 1) in vec4 a_arc;       // cx, cy, radius, start angle
layout(location = 2) in vec2 a_sweep;     // sweep (signed: negative is clockwise), start distance
uniform vec2 u_origin;
uniform float u_scale;
uniform vec2 u_viewport;
uniform float u_half_width;
out vec2 v_from_center;                   // pixels
flat out float v_radius;                  // pixels
flat out float v_from;                    // start angle
flat out float v_sweep;
flat out float v_start;

void main() {
  vec2 center = (a_arc.xy - u_origin) * u_scale;
  float radius = a_arc.z * u_scale;
  float reach = radius + u_half_width + 1.0;
  vec2 corner = vec2(a_corner.x * 2.0 - 1.0, a_corner.y);
  v_from_center = corner * reach;
  v_radius = radius;
  v_from = a_arc.w;
  v_sweep = a_sweep.x;
  v_start = a_sweep.y;
  gl_Position = vec4((center + corner * reach) / (u_viewport * 0.5), 0.0, 1.0);
}
`;

const ARC_FRAGMENT = `#version 300 es
precision highp float;
uniform float u_half_width;
uniform float u_scale;
uniform float u_pattern[${MAX_PATTERN}];
uniform int u_pattern_count;
uniform float u_period;
uniform vec4 u_color;
in vec2 v_from_center;
flat in float v_radius;
flat in float v_from;
flat in float v_sweep;
flat in float v_start;
out vec4 out_color;

const float TAU = 6.283185307179586;

float to_dash(float along_px) {
  float at = mod(v_start + along_px / u_scale, u_period);
  float nearest = 1e9;
  float from = 0.0;
  for (int index = 0; index < u_pattern_count; index += 2) {
    float dash = u_pattern[index];
    nearest = min(nearest, max(max(from - at, at - (from + dash)), 0.0));
    from += dash + u_pattern[index + 1];
  }
  nearest = min(nearest, u_period - at);
  return nearest * u_scale;
}

void main() {
  float across = length(v_from_center) - v_radius;
  // The angle travelled from the arc's first end, in its own direction.
  float turn = atan(v_from_center.y, v_from_center.x) - v_from;
  float travelled = v_sweep >= 0.0 ? mod(turn, TAU) : mod(-turn, TAU);
  float sweep = abs(v_sweep);
  float distance_px = abs(across);
  float along_px = travelled * v_radius;
  if (travelled > sweep && sweep < TAU - 1e-4) {
    // Past an end: the nearer end's round cap.
    float past_end = (travelled - sweep) * v_radius;
    float before_start = (TAU - travelled) * v_radius;
    along_px = past_end < before_start ? sweep * v_radius : 0.0;
    distance_px = length(vec2(min(past_end, before_start), across));
  }
  if (u_period > 0.0 && u_period * u_scale >= 4.0) {
    distance_px = max(distance_px, length(vec2(to_dash(along_px), across)));
  }
  float coverage = clamp(u_half_width + 0.5 - distance_px, 0.0, 1.0);
  out_color = vec4(u_color.rgb * u_color.a * coverage, u_color.a * coverage);
}
`;

function link(gl: WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
  const program = gl.createProgram();
  for (const [type, source] of [
    [gl.VERTEX_SHADER, vertex],
    [gl.FRAGMENT_SHADER, fragment],
  ] as const) {
    const shader = gl.createShader(type);
    if (!shader) throw new Error("cannot create a shader");
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(gl.getShaderInfoLog(shader) ?? "shader failed to compile");
    }
    gl.attachShader(program, shader);
  }
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? "program failed to link");
  }
  return program;
}

function generator(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

/** Segments as `x0, y0, x1, y1`, with the path distance at which each starts. */
interface Paths {
  segments: Float32Array;
  starts: Float32Array;
}

/**
 * Cuts every segment into the dashes of `pattern`: what the split candidate
 * uploads. A dash that crosses the end of a segment is cut there and carries
 * on in the next, so the pattern is continuous along a path either way.
 */
export function splitIntoDashes(paths: Paths, pattern: readonly number[]): Float32Array {
  const period = pattern.reduce((sum, length) => sum + length, 0);
  const dashes: number[] = [];
  const count = paths.segments.length / 4;
  for (let index = 0; index < count; index += 1) {
    const x0 = paths.segments[index * 4] ?? 0;
    const y0 = paths.segments[index * 4 + 1] ?? 0;
    const x1 = paths.segments[index * 4 + 2] ?? 0;
    const y1 = paths.segments[index * 4 + 3] ?? 0;
    const length = Math.hypot(x1 - x0, y1 - y0);
    if (length === 0) continue;
    const [ux, uy] = [(x1 - x0) / length, (y1 - y0) / length];
    const start = paths.starts[index] ?? 0;
    // The pattern position at the segment's start, then dash by dash.
    let at = -(start % period);
    while (at < length) {
      for (let element = 0; element < pattern.length && at < length; element += 2) {
        const dash = pattern[element] ?? 0;
        const from = Math.max(at, 0);
        const to = Math.min(at + dash, length);
        if (to >= from && at + dash >= 0) {
          dashes.push(x0 + ux * from, y0 + uy * from, x0 + ux * to, y0 + uy * to);
        }
        at += dash + (pattern[element + 1] ?? 0);
      }
    }
  }
  return new Float32Array(dashes);
}

interface Drawer {
  /** Draws `instances` segments with `starts` (or none) and the pattern (or solid). */
  segments(
    data: Float32Array,
    starts: Float32Array | undefined,
    pattern: readonly number[] | undefined,
  ): void;
  arcs(data: Float32Array, pattern: readonly number[] | undefined): void;
  clear(): void;
  sync(): void;
  view(originX: number, originY: number, scale: number): void;
}

function createDrawer(gl: WebGL2RenderingContext, width: number, height: number): Drawer {
  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);
  const pixel = new Uint8Array(4);
  let view = { x: 0, y: 0, scale: 1 };

  const pipeline = (vertex: string, fragment: string) => {
    const program = link(gl, vertex, fragment);
    const vao = gl.createVertexArray();
    const geometry = gl.createBuffer();
    const extra = gl.createBuffer();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, geometry);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribDivisor(1, 1);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);
    gl.vertexAttribDivisor(2, 1);
    const uniform = (name: string) => gl.getUniformLocation(program, name);
    const use = (pattern: readonly number[] | undefined) => {
      gl.useProgram(program);
      gl.bindVertexArray(vao);
      gl.uniform2f(uniform("u_origin"), view.x, view.y);
      gl.uniform1f(uniform("u_scale"), view.scale);
      gl.uniform2f(uniform("u_viewport"), width, height);
      gl.uniform1f(uniform("u_half_width"), 0.75);
      gl.uniform4f(uniform("u_color"), 0.95, 0.96, 0.97, 1);
      const padded = new Float32Array(MAX_PATTERN);
      padded.set(pattern ?? []);
      gl.uniform1fv(uniform("u_pattern[0]"), padded);
      gl.uniform1i(uniform("u_pattern_count"), pattern?.length ?? 0);
      gl.uniform1f(uniform("u_period"), pattern?.reduce((sum, value) => sum + value, 0) ?? 0);
    };
    return { geometry, extra, use };
  };
  const strokes = pipeline(VERTEX, FRAGMENT);
  const arcs = pipeline(ARC_VERTEX, ARC_FRAGMENT);
  /** Remembered so a frame can be redrawn without uploading again. */
  let uploaded: Float32Array | undefined;

  return {
    view(originX, originY, scale) {
      view = { x: originX, y: originY, scale };
    },
    clear() {
      gl.viewport(0, 0, width, height);
      gl.clearColor(0.03, 0.05, 0.09, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    },
    sync() {
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    },
    segments(data, starts, pattern) {
      strokes.use(pattern);
      if (uploaded !== data) {
        gl.bindBuffer(gl.ARRAY_BUFFER, strokes.geometry);
        gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
        gl.bindBuffer(gl.ARRAY_BUFFER, strokes.extra);
        if (starts) {
          gl.bufferData(gl.ARRAY_BUFFER, starts, gl.STATIC_DRAW);
          gl.enableVertexAttribArray(2);
          gl.vertexAttribPointer(2, 1, gl.FLOAT, false, 0, 0);
        } else {
          gl.disableVertexAttribArray(2);
        }
        uploaded = data;
      }
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, data.length / 4);
    },
    arcs(data, pattern) {
      arcs.use(pattern);
      // cx, cy, radius, start angle, sweep, start distance: six floats.
      gl.bindBuffer(gl.ARRAY_BUFFER, arcs.geometry);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 24, 0);
      gl.enableVertexAttribArray(2);
      gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 24, 16);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, data.length / 6);
      uploaded = undefined;
    },
  };
}

/** Segments per generated path: a polyline of four. */
const PATH_SEGMENTS = 4;

/**
 * `count` segments of 40 to 80 units, in paths of four joined end to end,
 * scattered over a 4000-unit square. The pattern has to carry on through
 * the three vertices inside each path.
 */
function scatter(count: number): Paths {
  const random = generator(2024);
  const segments = new Float32Array(count * 4);
  let [x, y, angle] = [0, 0, 0];
  for (let index = 0; index < count; index += 1) {
    if (index % PATH_SEGMENTS === 0) {
      [x, y, angle] = [random() * 4000, random() * 4000, random() * Math.PI * 2];
    } else {
      angle += (random() - 0.5) * 2;
    }
    const length = 40 + random() * 40;
    const [nextX, nextY] = [x + Math.cos(angle) * length, y + Math.sin(angle) * length];
    segments.set([x, y, nextX, nextY], index * 4);
    [x, y] = [nextX, nextY];
  }
  return { segments, starts: new Float32Array(count) };
}

/** The shader candidate's whole build: the running distance along each path. */
function accumulate(paths: Paths): void {
  let distance = 0;
  const count = paths.segments.length / 4;
  for (let index = 0; index < count; index += 1) {
    if (index % PATH_SEGMENTS === 0) distance = 0;
    paths.starts[index] = distance;
    const at = index * 4;
    distance += Math.hypot(
      (paths.segments[at + 2] ?? 0) - (paths.segments[at] ?? 0),
      (paths.segments[at + 3] ?? 0) - (paths.segments[at + 1] ?? 0),
    );
  }
}

export function probeLinetypes(segmentCount: number): LinetypeReport {
  const canvas = document.createElement("canvas");
  canvas.width = CANVAS_SIZE;
  canvas.height = CANVAS_SIZE;
  const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });
  if (!gl) throw new Error("WebGL2 is not available");
  const drawer = createDrawer(gl, CANVAS_SIZE, CANVAS_SIZE);

  // A dashed line: 6 on, 3 off. A segment of 60 units has about seven dashes.
  const pattern = [6, 3];
  const zooms = [0.25, 1, 4];
  const paths = scatter(segmentCount);

  const frame = (draw: () => void) =>
    zooms.map((zoom) => {
      drawer.view(2000, 2000, zoom);
      return time(() => {
        drawer.clear();
        draw();
        drawer.sync();
      });
    });

  // shader: the paths as they are, with a start distance each.
  const shaderBuild = time(() => accumulate(paths));
  const shaderFrames = frame(() => drawer.segments(paths.segments, paths.starts, pattern));

  // split: every dash an instance.
  let dashes = splitIntoDashes(paths, pattern);
  const splitBuild = time(() => {
    dashes = splitIntoDashes(paths, pattern);
  });
  const splitFrames = frame(() => drawer.segments(dashes, undefined, undefined));

  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return {
    segments: segmentCount,
    pattern,
    zooms,
    candidates: [
      {
        name: "shader",
        instances: segmentCount,
        bytes: paths.segments.byteLength + paths.starts.byteLength,
        buildMs: shaderBuild,
        frameMs: shaderFrames,
      },
      {
        name: "split",
        instances: dashes.length / 4,
        bytes: dashes.byteLength,
        buildMs: splitBuild,
        frameMs: splitFrames,
      },
    ],
  };
}

/**
 * A small scene drawn both ways at three zooms, as PNG data URLs: a path of
 * two lines, a half circle, and a third line, with one pattern running along
 * it, and a circle. Rows: shader, split (lines only: its arcs would be cut
 * the same way). The picture is the evidence that the pattern is continuous
 * through vertices and along curves, and stays put when the view changes.
 */
export function linetypePictures(): { zoom: number; shader: string; split: string }[] {
  const [width, height] = [640, 300];
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const gl = canvas.getContext("webgl2", {
    alpha: false,
    antialias: false,
    preserveDrawingBuffer: true,
  });
  if (!gl) throw new Error("WebGL2 is not available");
  const drawer = createDrawer(gl, width, height);

  // Dash, gap, dot, gap: a centre line.
  const pattern = [14, 5, 0, 5];
  // The path: right, up at an angle, round a half circle, and back left.
  const points: [number, number][] = [
    [-120, -40],
    [0, -40],
    [60, 0],
  ];
  const lengths = [120, Math.hypot(60, 40)];
  const radius = 30;
  const arcLength = Math.PI * radius;
  const segments = new Float32Array([
    ...(points[0] ?? []),
    ...(points[1] ?? []),
    ...(points[1] ?? []),
    ...(points[2] ?? []),
    // After the half circle, which ends at (60, 60).
    60,
    60,
    -120,
    60,
  ]);
  const before = (lengths[0] ?? 0) + (lengths[1] ?? 0);
  const starts = new Float32Array([0, lengths[0] ?? 0, before + arcLength]);
  const arcs = new Float32Array([
    // The half circle from (60, 0) up to (60, 60), bulging right.
    60,
    30,
    radius,
    -Math.PI / 2,
    Math.PI,
    before,
    // A separate circle, its pattern starting at its rightmost point.
    -60,
    10,
    22,
    0,
    Math.PI * 2,
    0,
  ]);
  const paths: Paths = { segments, starts };
  const dashes = splitIntoDashes(paths, pattern);

  return [0.6, 1.5, 4].map((zoom) => {
    drawer.view(zoom < 4 ? -20 : 40, zoom < 4 ? 10 : 5, zoom);
    drawer.clear();
    drawer.segments(segments, starts, pattern);
    drawer.arcs(arcs, pattern);
    drawer.sync();
    const shader = canvas.toDataURL("image/png");
    drawer.clear();
    drawer.segments(dashes, undefined, undefined);
    drawer.sync();
    const split = canvas.toDataURL("image/png");
    return { zoom, shader, split };
  });
}
