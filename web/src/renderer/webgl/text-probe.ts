// The text rendering experiment (DESIGN §23.5; ADR-0016). Loaded on demand,
// only when the page is opened with `?debug`; it is not part of the normal
// bundle path.
//
// Three ways to draw CAD text, on the same generated texts, in the probe's
// own WebGL2 context:
//
//   stroke   a stroke font (Hershey Simplex, the font `cad-text` ships):
//            every glyph is a few polylines, and a text is their segments,
//            drawn as instances by the stroke shader every line uses.
//   atlas    glyphs rasterised once into a texture, by the browser's Canvas
//            2D here, and every glyph of every text one textured quad.
//   fill     glyphs as filled triangle meshes, as tessellated outlines of a
//            TrueType font would be. Stands in for that: the meshes are the
//            stroke font's segments as thin quads, two triangles each, so
//            the triangle count is a lower bound for a real outline font.
//
// For each: how many instances and bytes, how long the texts take to turn
// into GPU data, and how long a frame takes at several zooms. `textPictures`
// draws one text each way, at three zooms and turned, for the eye.
import SIMPLEX from "../../../../crates/cad-text/fonts/futural.jhf?raw";

const CANVAS_SIZE = 1024;
const MIN_TIMED_MS = 60;
/** Characters per generated text. */
const TEXT_LENGTH = 8;
/** Cap height of the generated texts, in drawing units. */
const TEXT_HEIGHT = 10;
/** Pixels per em at which the atlas is rasterised. */
const ATLAS_PX = 48;

export type TextCandidate = "stroke" | "atlas" | "fill";

export interface TextCandidateReport {
  name: TextCandidate;
  /** Segment instances, glyph quads, or triangles. */
  instances: number;
  bytes: number;
  /** Mean milliseconds to turn the texts into GPU data. */
  buildMs: number;
  /** Mean milliseconds per frame, one per zoom in `TextReport.zooms`. */
  frameMs: number[];
}

export interface TextReport {
  texts: number;
  characters: number;
  /** Pixels per drawing unit at which frames were timed. */
  zooms: number[];
  /** Bytes of the stroke font's data, as shipped. */
  fontBytes: number;
  candidates: TextCandidateReport[];
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

// ── the stroke font, as `cad-text` reads it ──

interface Glyph {
  /** Polylines in cap heights, on a baseline at y = 0. */
  strokes: [number, number][][];
  advance: number;
}

const CAP_HEIGHT = 21;
const BASELINE = 9;

/** Parses a `.jhf` file: one glyph per line, from the space onwards. */
export function parseJhf(text: string): Glyph[] {
  const glyphs: Glyph[] = [];
  for (const line of text.split("\n")) {
    if (line.trim() === "") continue;
    const count = Number(line.slice(5, 8).trim());
    const value = (at: number) => line.charCodeAt(at) - "R".charCodeAt(0);
    const left = value(8);
    const right = value(9);
    const strokes: [number, number][][] = [];
    let current: [number, number][] = [];
    for (let pair = 1; pair < count; pair += 1) {
      const at = 8 + pair * 2;
      if (line[at] === " " && line[at + 1] === "R") {
        if (current.length > 1) strokes.push(current);
        current = [];
        continue;
      }
      current.push([(value(at) - left) / CAP_HEIGHT, (BASELINE - value(at + 1)) / CAP_HEIGHT]);
    }
    if (current.length > 1) strokes.push(current);
    glyphs.push({ strokes, advance: (right - left) / CAP_HEIGHT });
  }
  return glyphs;
}

const FONT = parseJhf(SIMPLEX);
const glyphOf = (c: string): Glyph | undefined => FONT[c.charCodeAt(0) - 32];

/** Width of `text` in cap heights, by the stroke font. */
export function strokeAdvance(text: string): number {
  let pen = 0;
  for (const c of text) pen += glyphOf(c)?.advance ?? 0;
  return pen;
}

// ── the generated texts ──

interface Placed {
  text: string;
  x: number;
  y: number;
  /** Radians from +X. */
  angle: number;
}

function generator(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.-";

/** `count` texts of eight characters at random places and angles over a 4000-unit square. */
function scatter(count: number): Placed[] {
  const random = generator(2026);
  const texts: Placed[] = [];
  for (let index = 0; index < count; index += 1) {
    let text = "";
    for (let c = 0; c < TEXT_LENGTH; c += 1) {
      text += ALPHABET[Math.floor(random() * ALPHABET.length)];
    }
    texts.push({ text, x: random() * 4000, y: random() * 4000, angle: random() * Math.PI * 2 });
  }
  return texts;
}

// ── stroke: segments ──

/** Every segment of every text: `x0, y0, x1, y1`. */
export function strokeSegments(texts: readonly Placed[], height: number): Float32Array {
  const out: number[] = [];
  for (const placed of texts) {
    const [sin, cos] = [Math.sin(placed.angle), Math.cos(placed.angle)];
    let pen = 0;
    for (const c of placed.text) {
      const glyph = glyphOf(c);
      if (!glyph) continue;
      for (const stroke of glyph.strokes) {
        for (let index = 1; index < stroke.length; index += 1) {
          const [ax, ay] = stroke[index - 1] ?? [0, 0];
          const [bx, by] = stroke[index] ?? [0, 0];
          const [x0, y0] = [(pen + ax) * height, ay * height];
          const [x1, y1] = [(pen + bx) * height, by * height];
          out.push(
            placed.x + x0 * cos - y0 * sin,
            placed.y + x0 * sin + y0 * cos,
            placed.x + x1 * cos - y1 * sin,
            placed.y + x1 * sin + y1 * cos,
          );
        }
      }
      pen += glyph.advance;
    }
  }
  return new Float32Array(out);
}

// ── fill: triangles ──

/** Every stroke segment as a thin quad of two triangles: `x, y` per vertex. */
export function fillTriangles(texts: readonly Placed[], height: number): Float32Array {
  const segments = strokeSegments(texts, height);
  const halfWidth = height * 0.045;
  const out = new Float32Array((segments.length / 4) * 12);
  for (let index = 0; index < segments.length / 4; index += 1) {
    const x0 = segments[index * 4] ?? 0;
    const y0 = segments[index * 4 + 1] ?? 0;
    const x1 = segments[index * 4 + 2] ?? 0;
    const y1 = segments[index * 4 + 3] ?? 0;
    const length = Math.hypot(x1 - x0, y1 - y0) || 1;
    const [nx, ny] = [(-(y1 - y0) / length) * halfWidth, ((x1 - x0) / length) * halfWidth];
    out.set(
      [
        x0 + nx,
        y0 + ny,
        x0 - nx,
        y0 - ny,
        x1 + nx,
        y1 + ny,
        x1 + nx,
        y1 + ny,
        x0 - nx,
        y0 - ny,
        x1 - nx,
        y1 - ny,
      ],
      index * 12,
    );
  }
  return out;
}

// ── atlas: glyph quads ──

interface Atlas {
  texture: WebGLTexture;
  /** Per character: `u0, v0, u1, v1` in texels, and the advance in cap heights. */
  cells: Map<string, { u0: number; v0: number; u1: number; v1: number; advance: number }>;
  size: number;
  /** Cap height in texels. */
  capPx: number;
}

/** Rasterises the alphabet with Canvas 2D into one texture. */
function buildAtlas(gl: WebGL2RenderingContext): Atlas {
  const size = 512;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Canvas 2D is not available");
  context.font = `${ATLAS_PX}px sans-serif`;
  context.fillStyle = "white";
  context.textBaseline = "alphabetic";
  const cellWidth = ATLAS_PX;
  const cellHeight = Math.round(ATLAS_PX * 1.3);
  const capPx = ATLAS_PX * 0.72;
  const cells: Atlas["cells"] = new Map();
  const perRow = Math.floor(size / cellWidth);
  [...`${ALPHABET} `].forEach((c, index) => {
    const column = index % perRow;
    const row = Math.floor(index / perRow);
    const x = column * cellWidth;
    const y = row * cellHeight;
    context.fillText(c, x + 2, y + ATLAS_PX);
    cells.set(c, {
      u0: x,
      v0: y,
      u1: x + cellWidth,
      v1: y + cellHeight,
      advance: context.measureText(c).width / capPx,
    });
  });
  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, canvas);
  return { texture, cells, size, capPx };
}

/**
 * One quad per glyph: its origin, its along and up vectors in drawing
 * units (so rotation costs nothing), and its texel rectangle — twelve floats.
 */
export function atlasQuads(texts: readonly Placed[], height: number, atlas: Atlas): Float32Array {
  const out: number[] = [];
  const scale = height / atlas.capPx;
  for (const placed of texts) {
    const [sin, cos] = [Math.sin(placed.angle), Math.cos(placed.angle)];
    let pen = 0;
    for (const c of placed.text) {
      const cell = atlas.cells.get(c);
      if (!cell) continue;
      // The cell's bottom-left in text space: the pen, less the descent.
      const descent = (cell.v1 - cell.v0 - ATLAS_PX) * scale;
      const [ox, oy] = [pen * height - 2 * scale, -descent];
      const [w, h] = [(cell.u1 - cell.u0) * scale, (cell.v1 - cell.v0) * scale];
      out.push(
        placed.x + ox * cos - oy * sin,
        placed.y + ox * sin + oy * cos,
        w * cos,
        w * sin,
        -h * sin,
        h * cos,
        cell.u0 / atlas.size,
        cell.v1 / atlas.size,
        cell.u1 / atlas.size,
        cell.v0 / atlas.size,
        0,
        0,
      );
      pen += cell.advance;
    }
  }
  return new Float32Array(out);
}

// ── shaders ──

const COMMON_VERTEX_UNIFORMS = `
uniform vec2 u_origin;
uniform float u_scale;
uniform vec2 u_viewport;
vec4 clip(vec2 world) {
  vec2 px = (world - u_origin) * u_scale;
  return vec4(px / (u_viewport * 0.5), 0.0, 1.0);
}
`;

const STROKE_VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec4 a_segment;
${COMMON_VERTEX_UNIFORMS}
uniform float u_half_width;
out vec2 v_local;
out float v_length;
void main() {
  vec2 start = (a_segment.xy - u_origin) * u_scale;
  vec2 end = (a_segment.zw - u_origin) * u_scale;
  vec2 delta = end - start;
  float len = length(delta);
  vec2 along = len > 1e-6 ? delta / len : vec2(1.0, 0.0);
  vec2 across = vec2(-along.y, along.x);
  float extent = u_half_width + 1.0;
  float end_sign = a_corner.x * 2.0 - 1.0;
  vec2 position = mix(start, end, a_corner.x) + along * end_sign * extent + across * a_corner.y * extent;
  v_local = vec2(a_corner.x * len + end_sign * extent, a_corner.y * extent);
  v_length = len;
  gl_Position = vec4(position / (u_viewport * 0.5), 0.0, 1.0);
}
`;

const STROKE_FRAGMENT = `#version 300 es
precision highp float;
uniform float u_half_width;
uniform vec4 u_color;
in vec2 v_local;
in float v_length;
out vec4 out_color;
void main() {
  float inside = clamp(v_local.x, 0.0, v_length);
  float distance_px = length(vec2(v_local.x - inside, v_local.y));
  float coverage = clamp(u_half_width + 0.5 - distance_px, 0.0, 1.0);
  out_color = vec4(u_color.rgb * u_color.a * coverage, u_color.a * coverage);
}
`;

const ATLAS_VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_corner;       // 0|1, 0|1
layout(location = 1) in vec4 a_origin_along; // x, y, along.x, along.y
layout(location = 2) in vec4 a_up_uv;        // up.x, up.y, u0, v0
layout(location = 3) in vec4 a_uv1;          // u1, v1, -, -
${COMMON_VERTEX_UNIFORMS}
out vec2 v_uv;
void main() {
  vec2 world = a_origin_along.xy + a_origin_along.zw * a_corner.x + a_up_uv.xy * a_corner.y;
  v_uv = mix(a_up_uv.zw, a_uv1.xy, a_corner);
  gl_Position = clip(world);
}
`;

const ATLAS_FRAGMENT = `#version 300 es
precision highp float;
uniform sampler2D u_atlas;
uniform vec4 u_color;
in vec2 v_uv;
out vec4 out_color;
void main() {
  float coverage = texture(u_atlas, v_uv).a;
  out_color = vec4(u_color.rgb * u_color.a * coverage, u_color.a * coverage);
}
`;

const FILL_VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_position;
${COMMON_VERTEX_UNIFORMS}
void main() {
  gl_Position = clip(a_position);
}
`;

const FILL_FRAGMENT = `#version 300 es
precision highp float;
uniform vec4 u_color;
out vec4 out_color;
void main() {
  out_color = vec4(u_color.rgb * u_color.a, u_color.a);
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

interface Drawer {
  stroke(segments: Float32Array): void;
  atlas(quads: Float32Array): void;
  fill(triangles: Float32Array): void;
  clear(): void;
  sync(): void;
  view(originX: number, originY: number, scale: number): void;
  readonly atlasTexture: Atlas;
}

function createDrawer(gl: WebGL2RenderingContext, width: number, height: number): Drawer {
  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);
  const unitQuad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, unitQuad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 0, 1, 1, 0, 1, 1]), gl.STATIC_DRAW);
  const pixel = new Uint8Array(4);
  let view = { x: 0, y: 0, scale: 1 };
  const atlas = buildAtlas(gl);

  const pipeline = (vertex: string, fragment: string) => {
    const program = link(gl, vertex, fragment);
    const vao = gl.createVertexArray();
    const buffer = gl.createBuffer();
    const uniform = (name: string) => gl.getUniformLocation(program, name);
    const use = () => {
      gl.useProgram(program);
      gl.bindVertexArray(vao);
      gl.uniform2f(uniform("u_origin"), view.x, view.y);
      gl.uniform1f(uniform("u_scale"), view.scale);
      gl.uniform2f(uniform("u_viewport"), width, height);
      gl.uniform4f(uniform("u_color"), 0.95, 0.96, 0.97, 1);
    };
    return { program, vao, buffer, uniform, use };
  };

  const stroke = pipeline(STROKE_VERTEX, STROKE_FRAGMENT);
  gl.bindVertexArray(stroke.vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, stroke.buffer);
  gl.enableVertexAttribArray(1);
  gl.vertexAttribDivisor(1, 1);
  gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);

  const glyphs = pipeline(ATLAS_VERTEX, ATLAS_FRAGMENT);
  gl.bindVertexArray(glyphs.vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, unitQuad);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, glyphs.buffer);
  for (const location of [1, 2, 3]) {
    gl.enableVertexAttribArray(location);
    gl.vertexAttribDivisor(location, 1);
    gl.vertexAttribPointer(location, 4, gl.FLOAT, false, 48, (location - 1) * 16);
  }

  const fill = pipeline(FILL_VERTEX, FILL_FRAGMENT);
  gl.bindVertexArray(fill.vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, fill.buffer);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindVertexArray(null);

  const uploaded = new Map<WebGLBuffer, Float32Array>();
  const upload = (buffer: WebGLBuffer, data: Float32Array) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    if (uploaded.get(buffer) !== data) {
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      uploaded.set(buffer, data);
    }
  };

  return {
    atlasTexture: atlas,
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
    stroke(segments) {
      stroke.use();
      gl.uniform1f(stroke.uniform("u_half_width"), 0.75);
      upload(stroke.buffer, segments);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, segments.length / 4);
    },
    atlas(quads) {
      glyphs.use();
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, atlas.texture);
      gl.uniform1i(glyphs.uniform("u_atlas"), 0);
      upload(glyphs.buffer, quads);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, quads.length / 12);
    },
    fill(triangles) {
      fill.use();
      upload(fill.buffer, triangles);
      gl.drawArrays(gl.TRIANGLES, 0, triangles.length / 2);
    },
  };
}

export function probeText(textCount: number): TextReport {
  const canvas = document.createElement("canvas");
  canvas.width = CANVAS_SIZE;
  canvas.height = CANVAS_SIZE;
  const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });
  if (!gl) throw new Error("WebGL2 is not available");
  const drawer = createDrawer(gl, CANVAS_SIZE, CANVAS_SIZE);

  // Software WebGL takes a minute over a large frame: the large run is
  // timed once, at one zoom, rather than repeatedly at three.
  const large = textCount > 20_000;
  const zooms = large ? [1] : [0.25, 1, 4];
  const texts = scatter(textCount);
  const once = (run: () => void) => {
    const start = performance.now();
    run();
    return performance.now() - start;
  };
  const frame = (draw: () => void) =>
    zooms.map((zoom) => {
      drawer.view(2000, 2000, zoom);
      return (large ? once : time)(() => {
        drawer.clear();
        draw();
        drawer.sync();
      });
    });

  let segments = strokeSegments(texts, TEXT_HEIGHT);
  const strokeBuild = time(() => {
    segments = strokeSegments(texts, TEXT_HEIGHT);
  });
  const strokeFrames = frame(() => drawer.stroke(segments));

  let quads = atlasQuads(texts, TEXT_HEIGHT, drawer.atlasTexture);
  const atlasBuild = time(() => {
    quads = atlasQuads(texts, TEXT_HEIGHT, drawer.atlasTexture);
  });
  const atlasFrames = frame(() => drawer.atlas(quads));

  let triangles = fillTriangles(texts, TEXT_HEIGHT);
  const fillBuild = time(() => {
    triangles = fillTriangles(texts, TEXT_HEIGHT);
  });
  const fillFrames = frame(() => drawer.fill(triangles));

  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return {
    texts: textCount,
    characters: textCount * TEXT_LENGTH,
    zooms,
    fontBytes: new TextEncoder().encode(SIMPLEX).byteLength,
    candidates: [
      {
        name: "stroke",
        instances: segments.length / 4,
        bytes: segments.byteLength,
        buildMs: strokeBuild,
        frameMs: strokeFrames,
      },
      {
        name: "atlas",
        instances: quads.length / 12,
        bytes: quads.byteLength,
        buildMs: atlasBuild,
        frameMs: atlasFrames,
      },
      {
        name: "fill",
        instances: triangles.length / 6,
        bytes: triangles.byteLength,
        buildMs: fillBuild,
        frameMs: fillFrames,
      },
    ],
  };
}

/**
 * One text drawn each way at three zooms, turned twenty degrees, as PNG data
 * URLs. The picture is the evidence for sharpness across zoom and under
 * rotation: a stroke stays one line wide, a filled outline stays sharp, a
 * rasterised atlas blurs as it grows.
 */
export function textPictures(): { zoom: number; stroke: string; atlas: string; fill: string }[] {
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
  // Sixteen units tall: 8, 32, and 128 pixels at the three zooms.
  const size = 16;
  const angle = (10 * Math.PI) / 180;

  return [0.5, 2, 8].map((zoom) => {
    // The text starts near the bottom left whatever the zoom.
    const placed: Placed = { text: "A12.5", x: -290 / zoom, y: -110 / zoom, angle };
    const picture = (draw: () => void) => {
      drawer.view(0, 0, zoom);
      drawer.clear();
      draw();
      drawer.sync();
      return canvas.toDataURL("image/png");
    };
    return {
      zoom,
      stroke: picture(() => drawer.stroke(strokeSegments([placed], size))),
      atlas: picture(() => drawer.atlas(atlasQuads([placed], size, drawer.atlasTexture))),
      fill: picture(() => drawer.fill(fillTriangles([placed], size))),
    };
  });
}
