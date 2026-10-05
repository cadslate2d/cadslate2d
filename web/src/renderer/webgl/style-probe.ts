// The colour-transport experiment (ADR-0014). Loaded on demand, only when
// the page is opened with `?debug`; it is not part of the normal bundle path.
//
// It draws the same generated segments three ways and times each, in its own
// WebGL2 context:
//
//   batches   one draw call per run of instances with the same style, the
//             colour in a uniform — how style roles are drawn today;
//   colour    the colour in every instance, one draw call;
//   table     a style index in every instance, looked up in a small texture
//             by the vertex shader, one draw call.
//
// For each: the bytes per instance, the time to upload everything, the time
// to draw a frame, and the time to recolour one style — what changing a
// layer's colour costs.

const FLOATS_PER_SEGMENT = 4;
const CANVAS_SIZE = 1024;
const TABLE_WIDTH = 1024;
const MIN_TIMED_MS = 60;

export interface StyleCandidateReport {
  name: string;
  bytesPerInstance: number;
  drawCalls: number;
  /** Mean milliseconds. */
  ms: { upload: number; frame: number; recolour: number };
}

export interface StyleReport {
  segments: number;
  styles: number;
  /** Whether instances of a style are contiguous, or interleaved as drawn. */
  order: "grouped" | "interleaved";
  candidates: StyleCandidateReport[];
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

const VERTEX_HEAD = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec4 a_segment;
uniform vec2 u_viewport;
out vec4 v_color;
`;

const VERTEX_BODY = `
  vec2 start = a_segment.xy;
  vec2 end = a_segment.zw;
  vec2 delta = end - start;
  float len = length(delta);
  vec2 along = len > 1e-6 ? delta / len : vec2(1.0, 0.0);
  vec2 across = vec2(-along.y, along.x);
  vec2 position = mix(start, end, a_corner.x) + across * a_corner.y * 1.5;
  gl_Position = vec4(position / (u_viewport * 0.5) - 1.0, 0.0, 1.0);
}
`;

const VERTEX_UNIFORM = `${VERTEX_HEAD}
uniform vec4 u_color;
void main() {
  v_color = u_color;${VERTEX_BODY}`;

const VERTEX_COLOUR = `${VERTEX_HEAD}
layout(location = 2) in vec4 a_color;
void main() {
  v_color = a_color;${VERTEX_BODY}`;

const VERTEX_TABLE = `${VERTEX_HEAD}
layout(location = 2) in float a_style;
uniform sampler2D u_styles;
void main() {
  int style = int(a_style);
  v_color = texelFetch(u_styles, ivec2(style % ${TABLE_WIDTH}, style / ${TABLE_WIDTH}), 0);${VERTEX_BODY}`;

const FRAGMENT = `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 out_color;
void main() {
  out_color = vec4(v_color.rgb * v_color.a, v_color.a);
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

/** A small deterministic generator, so every run draws the same thing. */
function generator(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

export function probeStyles(
  segmentCount: number,
  styleCount: number,
  order: "grouped" | "interleaved",
): StyleReport {
  const canvas = document.createElement("canvas");
  canvas.width = CANVAS_SIZE;
  canvas.height = CANVAS_SIZE;
  const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });
  if (!gl) throw new Error("WebGL2 is not available");

  // Short segments scattered over the canvas, each with a style.
  const random = generator(12345);
  const segments = new Float32Array(segmentCount * FLOATS_PER_SEGMENT);
  const styleOf = new Uint16Array(segmentCount);
  for (let index = 0; index < segmentCount; index += 1) {
    const x = random() * CANVAS_SIZE;
    const y = random() * CANVAS_SIZE;
    const angle = random() * Math.PI * 2;
    segments.set([x, y, x + Math.cos(angle) * 12, y + Math.sin(angle) * 12], index * 4);
    styleOf[index] =
      order === "grouped"
        ? Math.floor((index * styleCount) / segmentCount)
        : Math.floor(random() * styleCount);
  }
  const colours = new Uint8Array(styleCount * 4);
  for (let style = 0; style < styleCount; style += 1) {
    colours.set([64 + random() * 191, 64 + random() * 191, 64 + random() * 191, 255], style * 4);
  }

  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);

  const pixel = new Uint8Array(4);
  // Reading a pixel back is what waits for the GPU process (see transport-probe).
  const sync = () => gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
  const clear = () => {
    gl.viewport(0, 0, CANVAS_SIZE, CANVAS_SIZE);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
  };

  /** A vertex array over the quad and an instance buffer of segments. */
  const vertexArray = (geometry: WebGLBuffer) => {
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, geometry);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribDivisor(1, 1);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);
    return vao;
  };
  const uploadTo = (buffer: WebGLBuffer, data: ArrayBufferView) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  };

  const candidates: StyleCandidateReport[] = [];

  // ── batches: runs of one style, the colour in a uniform ──
  {
    const program = link(gl, VERTEX_UNIFORM, FRAGMENT);
    const viewport = gl.getUniformLocation(program, "u_viewport");
    const colour = gl.getUniformLocation(program, "u_color");
    const geometry = gl.createBuffer();
    const vao = vertexArray(geometry);
    const runs: [style: number, first: number, count: number][] = [];
    for (let index = 0; index < segmentCount; index += 1) {
      const style = styleOf[index] ?? 0;
      const last = runs[runs.length - 1];
      if (last && last[0] === style) last[2] += 1;
      else runs.push([style, index, 1]);
    }
    const upload = () => {
      uploadTo(geometry, segments);
      sync();
    };
    const frame = () => {
      clear();
      gl.useProgram(program);
      gl.uniform2f(viewport, CANVAS_SIZE, CANVAS_SIZE);
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, geometry);
      for (const [style, first, count] of runs) {
        const at = style * 4;
        gl.uniform4f(
          colour,
          (colours[at] ?? 0) / 255,
          (colours[at + 1] ?? 0) / 255,
          (colours[at + 2] ?? 0) / 255,
          1,
        );
        gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, first * 16);
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);
      }
      sync();
    };
    candidates.push({
      name: "batches",
      bytesPerInstance: 16,
      drawCalls: runs.length,
      // Recolouring a style changes a number the next frame reads: no upload.
      ms: { upload: time(upload), frame: time(frame), recolour: 0 },
    });
  }

  // ── colour: four bytes of colour in every instance ──
  {
    const program = link(gl, VERTEX_COLOUR, FRAGMENT);
    const viewport = gl.getUniformLocation(program, "u_viewport");
    const geometry = gl.createBuffer();
    const colourBuffer = gl.createBuffer();
    const vao = vertexArray(geometry);
    gl.bindBuffer(gl.ARRAY_BUFFER, colourBuffer);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribDivisor(2, 1);
    gl.vertexAttribPointer(2, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    const perInstance = new Uint8Array(segmentCount * 4);
    const fill = () => {
      for (let index = 0; index < segmentCount; index += 1) {
        const at = (styleOf[index] ?? 0) * 4;
        perInstance.set(colours.subarray(at, at + 4), index * 4);
      }
    };
    fill();
    const upload = () => {
      uploadTo(geometry, segments);
      uploadTo(colourBuffer, perInstance);
      sync();
    };
    const frame = () => {
      clear();
      gl.useProgram(program);
      gl.uniform2f(viewport, CANVAS_SIZE, CANVAS_SIZE);
      gl.bindVertexArray(vao);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, segmentCount);
      sync();
    };
    // Recolouring a style rewrites the colour of every instance that has it
    // and uploads the colour buffer again.
    const recolour = () => {
      colours[0] = ((colours[0] ?? 0) + 1) % 256;
      fill();
      uploadTo(colourBuffer, perInstance);
      sync();
    };
    candidates.push({
      name: "colour",
      bytesPerInstance: 20,
      drawCalls: 1,
      ms: { upload: time(upload), frame: time(frame), recolour: time(recolour) },
    });
  }

  // ── table: a style index in every instance, looked up in a texture ──
  {
    const program = link(gl, VERTEX_TABLE, FRAGMENT);
    const viewport = gl.getUniformLocation(program, "u_viewport");
    const geometry = gl.createBuffer();
    const styleBuffer = gl.createBuffer();
    const vao = vertexArray(geometry);
    gl.bindBuffer(gl.ARRAY_BUFFER, styleBuffer);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribDivisor(2, 1);
    // Two bytes per instance, read by the shader as a float.
    gl.vertexAttribPointer(2, 1, gl.UNSIGNED_SHORT, false, 0, 0);
    const rows = Math.ceil(styleCount / TABLE_WIDTH);
    const table = new Uint8Array(TABLE_WIDTH * rows * 4);
    table.set(colours);
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    const uploadTable = () => {
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.RGBA8,
        TABLE_WIDTH,
        rows,
        0,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        table,
      );
    };
    const upload = () => {
      uploadTo(geometry, segments);
      uploadTo(styleBuffer, styleOf);
      uploadTable();
      sync();
    };
    const frame = () => {
      clear();
      gl.useProgram(program);
      gl.uniform2f(viewport, CANVAS_SIZE, CANVAS_SIZE);
      gl.bindVertexArray(vao);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, segmentCount);
      sync();
    };
    // Recolouring a style changes one row and uploads the table again.
    const recolour = () => {
      table[0] = ((table[0] ?? 0) + 1) % 256;
      uploadTable();
      sync();
    };
    candidates.push({
      name: "table",
      bytesPerInstance: 18,
      drawCalls: 1,
      ms: { upload: time(upload), frame: time(frame), recolour: time(recolour) },
    });
  }

  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return { segments: segmentCount, styles: styleCount, order, candidates };
}
