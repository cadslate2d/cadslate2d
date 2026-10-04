import {
  FLOATS_PER_SEGMENT,
  WORDS_PER_BATCH,
  type ClipTransform,
  type FrameBuffers,
  type StyleRole,
} from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import { nextCapacity } from "./buffer-capacity";

export interface StrokeUniforms {
  readonly scale: WebGLUniformLocation;
  readonly translate: WebGLUniformLocation;
  readonly viewport: WebGLUniformLocation;
  readonly halfWidth: WebGLUniformLocation;
  readonly color: WebGLUniformLocation;
}

const CORNER_ATTRIBUTE = 0;
const SEGMENT_ATTRIBUTE = 1;
const QUAD_VERTICES = 4;

/**
 * GPU-side storage for one render lifetime (committed scene or overlay): an
 * instance buffer of segments plus the batches that say how to draw them.
 */
export class SegmentLayer {
  readonly #gl: WebGL2RenderingContext;
  readonly #vao: WebGLVertexArrayObject;
  readonly #buffer: WebGLBuffer;
  readonly #usage: number;
  #capacityBytes = 0;
  // Batches are copied out of WASM memory; views must not outlive the upload.
  #batches = new Uint32Array(0);

  constructor(gl: WebGL2RenderingContext, quad: WebGLBuffer, usage: number) {
    this.#gl = gl;
    this.#usage = usage;
    this.#vao = gl.createVertexArray();
    this.#buffer = gl.createBuffer();

    gl.bindVertexArray(this.#vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.enableVertexAttribArray(CORNER_ATTRIBUTE);
    gl.vertexAttribPointer(CORNER_ATTRIBUTE, 2, gl.FLOAT, false, 0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    // Allocate up front so small frames never reallocate, not even the first.
    this.#capacityBytes = nextCapacity(0, 0);
    gl.bufferData(gl.ARRAY_BUFFER, this.#capacityBytes, usage);
    gl.enableVertexAttribArray(SEGMENT_ATTRIBUTE);
    gl.vertexAttribPointer(SEGMENT_ATTRIBUTE, FLOATS_PER_SEGMENT, gl.FLOAT, false, 0, 0);
    gl.vertexAttribDivisor(SEGMENT_ATTRIBUTE, 1);
    gl.bindVertexArray(null);
  }

  /** Copies the frame to the GPU. Returns true when the buffer was reallocated. */
  upload(frame: FrameBuffers): boolean {
    const gl = this.#gl;
    const bytes = frame.segments.byteLength;
    const capacity = nextCapacity(this.#capacityBytes, bytes);
    const reallocated = capacity !== this.#capacityBytes;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    if (reallocated) {
      gl.bufferData(gl.ARRAY_BUFFER, capacity, this.#usage);
      this.#capacityBytes = capacity;
    }
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, frame.segments);

    if (this.#batches.length === frame.batches.length) {
      this.#batches.set(frame.batches);
    } else {
      this.#batches = frame.batches.slice();
    }
    return reallocated;
  }

  /** Issues one instanced draw per batch. Returns the number of draw calls. */
  draw(
    uniforms: StrokeUniforms,
    transform: ClipTransform,
    palette: CanvasPalette,
    devicePixelRatio: number,
  ): number {
    const gl = this.#gl;
    if (this.#batches.length === 0) return 0;

    gl.bindVertexArray(this.#vao);
    gl.uniform2f(uniforms.scale, transform[0] ?? 0, transform[1] ?? 0);
    gl.uniform2f(uniforms.translate, transform[2] ?? 0, transform[3] ?? 0);

    let drawCalls = 0;
    for (let offset = 0; offset < this.#batches.length; offset += WORDS_PER_BATCH) {
      const role = this.#batches[offset] as StyleRole;
      const first = this.#batches[offset + 1] ?? 0;
      const count = this.#batches[offset + 2] ?? 0;
      const stroke = palette.strokes[role];
      if (stroke === undefined || count === 0) continue;

      gl.uniform4f(uniforms.color, ...stroke.color);
      gl.uniform1f(uniforms.halfWidth, (stroke.width * devicePixelRatio) / 2);
      // Point the instance attribute at this batch's first segment.
      gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
      gl.vertexAttribPointer(
        SEGMENT_ATTRIBUTE,
        FLOATS_PER_SEGMENT,
        gl.FLOAT,
        false,
        0,
        first * FLOATS_PER_SEGMENT * Float32Array.BYTES_PER_ELEMENT,
      );
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, QUAD_VERTICES, count);
      drawCalls += 1;
    }
    gl.bindVertexArray(null);
    return drawCalls;
  }

  dispose(): void {
    this.#gl.deleteBuffer(this.#buffer);
    this.#gl.deleteVertexArray(this.#vao);
  }
}
