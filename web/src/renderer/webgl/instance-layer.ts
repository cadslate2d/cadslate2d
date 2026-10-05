import { WORDS_PER_BATCH, type ClipTransform, type StyleRole } from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import { nextCapacity } from "./buffer-capacity";
import { STYLE_ATTRIBUTE_LOCATION } from "./style-table";

export interface StrokeUniforms {
  readonly scale: WebGLUniformLocation;
  readonly translate: WebGLUniformLocation;
  readonly viewport: WebGLUniformLocation;
  readonly halfWidth: WebGLUniformLocation;
  readonly color: WebGLUniformLocation;
  /** 1 when the colour comes from the style table, 0 when from `color`. */
  readonly styled: WebGLUniformLocation;
  /** The sampler the style table is bound to. */
  readonly styles: WebGLUniformLocation;
}

/** One per-instance vertex attribute: where it is bound and where it sits in an instance. */
export interface InstanceAttribute {
  readonly location: number;
  /** Number of floats. */
  readonly size: number;
  /** Offset in floats from the start of the instance. */
  readonly offset: number;
}

/**
 * How one kind of primitive is drawn: what an instance looks like in memory
 * and how many vertices the shader generates for it. Segments and arcs are
 * two instances of this description (ADR-0008, ADR-0009).
 */
export interface InstanceLayout {
  readonly floatsPerInstance: number;
  readonly attributes: readonly InstanceAttribute[];
  /** Vertices drawn per instance, as a triangle strip. */
  readonly verticesPerInstance: number;
  /** A per-vertex buffer bound at location 0, for shaders that need one. */
  readonly perVertex?: { readonly buffer: WebGLBuffer; readonly size: number };
}

/**
 * GPU-side storage for one kind of primitive in one render lifetime: an
 * instance buffer plus the batches that say how to draw it. A *styled* layer
 * — committed geometry — has a second buffer with a style index per
 * instance, and takes its colours from the style table (ADR-0014).
 */
export class InstanceLayer {
  readonly #gl: WebGL2RenderingContext;
  readonly #layout: InstanceLayout;
  readonly #vao: WebGLVertexArrayObject;
  readonly #buffer: WebGLBuffer;
  /** One style index per instance; absent for a layer styled by its batches. */
  readonly #styles: WebGLBuffer | undefined;
  readonly #usage: number;
  #capacityBytes: number;
  // Batches are copied out of WASM memory; views must not outlive the upload.
  #batches = new Uint32Array(0);

  constructor(gl: WebGL2RenderingContext, layout: InstanceLayout, usage: number, styled = false) {
    this.#gl = gl;
    this.#layout = layout;
    this.#usage = usage;
    this.#vao = gl.createVertexArray();
    this.#buffer = gl.createBuffer();
    this.#styles = styled ? gl.createBuffer() : undefined;

    gl.bindVertexArray(this.#vao);
    if (layout.perVertex) {
      gl.bindBuffer(gl.ARRAY_BUFFER, layout.perVertex.buffer);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, layout.perVertex.size, gl.FLOAT, false, 0, 0);
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    // Allocate up front so small frames never reallocate, not even the first.
    this.#capacityBytes = nextCapacity(0, 0);
    gl.bufferData(gl.ARRAY_BUFFER, this.#capacityBytes, usage);
    for (const attribute of layout.attributes) {
      gl.enableVertexAttribArray(attribute.location);
      gl.vertexAttribDivisor(attribute.location, 1);
    }
    if (this.#styles) {
      gl.enableVertexAttribArray(STYLE_ATTRIBUTE_LOCATION);
      gl.vertexAttribDivisor(STYLE_ATTRIBUTE_LOCATION, 1);
    }
    this.#pointAt(0);
    gl.bindVertexArray(null);
  }

  /**
   * Copies instances and batches — and, for a styled layer, the style index
   * of each instance — to the GPU. Returns true when the instance buffer was
   * reallocated.
   */
  upload(instances: Float32Array, batches: Uint32Array, styles?: Uint16Array): boolean {
    const gl = this.#gl;
    const capacity = nextCapacity(this.#capacityBytes, instances.byteLength);
    const reallocated = capacity !== this.#capacityBytes;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    if (reallocated) {
      gl.bufferData(gl.ARRAY_BUFFER, capacity, this.#usage);
      this.#capacityBytes = capacity;
    }
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, instances);

    if (this.#styles && styles) {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.#styles);
      gl.bufferData(gl.ARRAY_BUFFER, styles, this.#usage);
    }

    if (this.#batches.length === batches.length) {
      this.#batches.set(batches);
    } else {
      this.#batches = batches.slice();
    }
    return reallocated;
  }

  /**
   * Issues one instanced draw per batch whose role `include` accepts, with
   * the currently bound program. Returns the number of draw calls.
   */
  draw(
    uniforms: StrokeUniforms,
    transform: ClipTransform,
    palette: CanvasPalette,
    devicePixelRatio: number,
    include: (role: StyleRole) => boolean,
  ): number {
    const gl = this.#gl;
    if (this.#batches.length === 0) return 0;

    gl.bindVertexArray(this.#vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    gl.uniform2f(uniforms.scale, transform[0] ?? 0, transform[1] ?? 0);
    gl.uniform2f(uniforms.translate, transform[2] ?? 0, transform[3] ?? 0);
    gl.uniform1f(uniforms.styled, this.#styles ? 1 : 0);

    let drawCalls = 0;
    for (let offset = 0; offset < this.#batches.length; offset += WORDS_PER_BATCH) {
      const role = this.#batches[offset] as StyleRole;
      const first = this.#batches[offset + 1] ?? 0;
      const count = this.#batches[offset + 2] ?? 0;
      const stroke = palette.strokes[role];
      if (stroke === undefined || count === 0 || !include(role)) continue;

      gl.uniform4f(uniforms.color, ...stroke.color);
      gl.uniform1f(uniforms.halfWidth, (stroke.width * devicePixelRatio) / 2);
      this.#pointAt(first);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, this.#layout.verticesPerInstance, count);
      drawCalls += 1;
    }
    gl.bindVertexArray(null);
    return drawCalls;
  }

  dispose(): void {
    this.#gl.deleteBuffer(this.#buffer);
    if (this.#styles) this.#gl.deleteBuffer(this.#styles);
    this.#gl.deleteVertexArray(this.#vao);
  }

  /** Points the instance attributes at the instance with index `first`. */
  #pointAt(first: number): void {
    const gl = this.#gl;
    const bytesPerFloat = Float32Array.BYTES_PER_ELEMENT;
    const stride = this.#layout.floatsPerInstance * bytesPerFloat;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    for (const attribute of this.#layout.attributes) {
      const offset = first * stride + attribute.offset * bytesPerFloat;
      gl.vertexAttribPointer(attribute.location, attribute.size, gl.FLOAT, false, stride, offset);
    }
    if (this.#styles) {
      // A 16-bit index per instance, read by the shader as a float.
      const bytesPerStyle = Uint16Array.BYTES_PER_ELEMENT;
      gl.bindBuffer(gl.ARRAY_BUFFER, this.#styles);
      gl.vertexAttribPointer(
        STYLE_ATTRIBUTE_LOCATION,
        1,
        gl.UNSIGNED_SHORT,
        false,
        bytesPerStyle,
        first * bytesPerStyle,
      );
      gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
    }
  }
}
