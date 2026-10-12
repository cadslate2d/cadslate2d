import {
  WORDS_PER_BATCH,
  type ClipTransform,
  type InstanceRange,
  type StyleRole,
} from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import { nextCapacity } from "./buffer-capacity";
import { DISTANCE_ATTRIBUTE_LOCATION } from "./linetypes";
import { STYLE_ATTRIBUTE_LOCATION } from "./style-table";

/**
 * Where a program's uniforms live. Those a fill does not use — the
 * viewport, the width, the pattern table — may be compiled away by the
 * driver, so they may be absent: setting an absent uniform does nothing.
 */
export interface StrokeUniforms {
  readonly scale: WebGLUniformLocation;
  readonly translate: WebGLUniformLocation;
  readonly viewport: WebGLUniformLocation | null;
  readonly halfWidth: WebGLUniformLocation | null;
  readonly color: WebGLUniformLocation;
  /** 1 when the colour comes from the style table, 0 when from `color`. */
  readonly styled: WebGLUniformLocation;
  /** The sampler the style table is bound to. */
  readonly styles: WebGLUniformLocation;
  /** The sampler the pattern table is bound to. */
  readonly patterns: WebGLUniformLocation | null;
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
  /**
   * Floats per instance in the path distance buffer of a styled layer
   * (ADR-0015); 0 for a primitive with no linetype, such as a fill.
   */
  readonly floatsPerDistance: number;
}

/**
 * GPU-side storage for one kind of primitive in one render lifetime: an
 * instance buffer plus the batches that say how to draw it. A *styled* layer
 * — committed geometry — has two more buffers: a style index per instance,
 * which picks its colour, width, and linetype from the style table
 * (ADR-0014), and the distance along its entity's path at which each
 * instance begins, which places the linetype on it (ADR-0015).
 */
export class InstanceLayer {
  readonly #gl: WebGL2RenderingContext;
  readonly #layout: InstanceLayout;
  readonly #vao: WebGLVertexArrayObject;
  readonly #buffer: WebGLBuffer;
  /** One style index per instance; absent for a layer styled by its batches. */
  readonly #styles: WebGLBuffer | undefined;
  /** The path distance of each instance; present exactly when `#styles` is. */
  readonly #distances: WebGLBuffer | undefined;
  readonly #usage: number;
  #capacityBytes: number;
  #styleCapacityBytes = 0;
  #distanceCapacityBytes = 0;
  // Batches are copied out of WASM memory; views must not outlive the upload.
  #batches = new Uint32Array(0);

  constructor(gl: WebGL2RenderingContext, layout: InstanceLayout, usage: number, styled = false) {
    this.#gl = gl;
    this.#layout = layout;
    this.#usage = usage;
    this.#vao = gl.createVertexArray();
    this.#buffer = gl.createBuffer();
    this.#styles = styled ? gl.createBuffer() : undefined;
    this.#distances = styled && layout.floatsPerDistance > 0 ? gl.createBuffer() : undefined;

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
    if (this.#distances) {
      gl.enableVertexAttribArray(DISTANCE_ATTRIBUTE_LOCATION);
      gl.vertexAttribDivisor(DISTANCE_ATTRIBUTE_LOCATION, 1);
    }
    this.#pointAt(0);
    gl.bindVertexArray(null);
  }

  /**
   * Copies instances and batches — and, for a styled layer, the style index
   * and path distance of each instance — to the GPU. With `ranges`, only
   * those runs of instances are copied (ADR-0022), unless a buffer had to
   * grow, when everything is. Returns how many buffers were reallocated.
   */
  upload(
    instances: Float32Array,
    batches: Uint32Array,
    styles?: Uint16Array,
    distances?: Float32Array,
    ranges?: readonly InstanceRange[],
  ): number {
    const gl = this.#gl;
    const floats = this.#layout.floatsPerInstance;
    let reallocated = 0;

    const fit = (buffer: WebGLBuffer, capacity: number, bytes: number): number => {
      const next = nextCapacity(capacity, bytes);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      if (next !== capacity) {
        gl.bufferData(gl.ARRAY_BUFFER, next, this.#usage);
        reallocated += 1;
      }
      return next;
    };
    this.#capacityBytes = fit(this.#buffer, this.#capacityBytes, instances.byteLength);
    if (this.#styles && styles) {
      this.#styleCapacityBytes = fit(this.#styles, this.#styleCapacityBytes, styles.byteLength);
    }
    if (this.#distances && distances) {
      this.#distanceCapacityBytes = fit(
        this.#distances,
        this.#distanceCapacityBytes,
        distances.byteLength,
      );
    }

    // A grown buffer holds nothing yet: everything goes up.
    const runs: readonly InstanceRange[] =
      ranges === undefined || reallocated > 0 ? [[0, instances.length / floats]] : ranges;
    const copy = <T extends Float32Array | Uint16Array>(
      buffer: WebGLBuffer,
      data: T,
      perInstance: number,
    ): void => {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      for (const [first, count] of runs) {
        if (count === 0) continue;
        const start = first * perInstance;
        const piece = data.subarray(start, start + count * perInstance);
        gl.bufferSubData(gl.ARRAY_BUFFER, start * data.BYTES_PER_ELEMENT, piece);
      }
    };
    copy(this.#buffer, instances, floats);
    if (this.#styles && styles) copy(this.#styles, styles, 1);
    if (this.#distances && distances) {
      copy(this.#distances, distances, this.#layout.floatsPerDistance);
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
    if (this.#distances) this.#gl.deleteBuffer(this.#distances);
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
    }
    if (this.#distances) {
      const size = this.#layout.floatsPerDistance;
      gl.bindBuffer(gl.ARRAY_BUFFER, this.#distances);
      gl.vertexAttribPointer(
        DISTANCE_ATTRIBUTE_LOCATION,
        size,
        gl.FLOAT,
        false,
        size * bytesPerFloat,
        first * size * bytesPerFloat,
      );
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, this.#buffer);
  }
}
