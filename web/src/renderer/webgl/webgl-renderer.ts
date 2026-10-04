// The WebGL2 adapter (DESIGN §12.1). TypeScript owns the context, the GPU
// buffers, and the draw calls; it never reads or writes CAD state.
import type { ClipTransform, FrameBuffers } from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import type { FrameSink } from "../render-frame";
import { linkProgram } from "./program";
import { SegmentLayer, type StrokeUniforms } from "./segment-layer";
import { STROKE_FRAGMENT_SHADER, STROKE_VERTEX_SHADER } from "./shaders";

/** Counters that let tests prove what the renderer did and did not do. */
export interface RendererStats {
  frames: number;
  drawCalls: number;
  /** Uploads of committed geometry. Must not grow with pointer activity. */
  sceneUploads: number;
  /** Uploads of transient geometry. */
  overlayUploads: number;
  /** GPU buffer reallocations, for either layer. */
  bufferReallocations: number;
}

interface GpuResources {
  readonly program: WebGLProgram;
  readonly uniforms: StrokeUniforms;
  readonly quad: WebGLBuffer;
  /** Committed geometry: uploaded rarely, drawn often. */
  readonly scene: SegmentLayer;
  /** Transient geometry: small, rewritten on most frames. */
  readonly overlay: SegmentLayer;
}

export class WebGlRenderer implements FrameSink {
  readonly stats: RendererStats = {
    frames: 0,
    drawCalls: 0,
    sceneUploads: 0,
    overlayUploads: 0,
    bufferReallocations: 0,
  };

  /** Called after a lost context is restored; the host should request a frame. */
  onContextRestored: (() => void) | undefined;

  readonly #canvas: HTMLCanvasElement;
  readonly #gl: WebGL2RenderingContext;
  readonly #palette: CanvasPalette;
  #resources: GpuResources;
  #sceneRevision: number | undefined;
  #devicePixelRatio = 1;

  constructor(canvas: HTMLCanvasElement, palette: CanvasPalette) {
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });
    if (!gl) throw new Error("WebGL2 is not available; check webGl2Available() first");
    this.#canvas = canvas;
    this.#gl = gl;
    this.#palette = palette;
    this.#resources = createResources(gl);

    canvas.addEventListener("webglcontextlost", this.#onContextLost);
    canvas.addEventListener("webglcontextrestored", this.#onContextRestored);
  }

  get sceneRevision(): number | undefined {
    return this.#sceneRevision;
  }

  /** Sizes the backing store for the given CSS size and device pixel ratio. */
  resize(cssWidth: number, cssHeight: number, devicePixelRatio: number): void {
    this.#devicePixelRatio = devicePixelRatio;
    const width = Math.max(1, Math.round(cssWidth * devicePixelRatio));
    const height = Math.max(1, Math.round(cssHeight * devicePixelRatio));
    if (this.#canvas.width !== width || this.#canvas.height !== height) {
      this.#canvas.width = width;
      this.#canvas.height = height;
    }
  }

  updateScene(revision: number, frame: FrameBuffers): void {
    if (this.#resources.scene.upload(frame)) this.stats.bufferReallocations += 1;
    this.#sceneRevision = revision;
    this.stats.sceneUploads += 1;
  }

  updateOverlay(frame: FrameBuffers): void {
    if (this.#resources.overlay.upload(frame)) this.stats.bufferReallocations += 1;
    this.stats.overlayUploads += 1;
  }

  draw(sceneTransform: ClipTransform, overlayTransform: ClipTransform): void {
    const gl = this.#gl;
    const { program, uniforms, scene, overlay } = this.#resources;
    const [red, green, blue] = this.#palette.background;

    gl.viewport(0, 0, this.#canvas.width, this.#canvas.height);
    gl.clearColor(red, green, blue, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    gl.useProgram(program);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA); // premultiplied alpha
    gl.uniform2f(uniforms.viewport, this.#canvas.width, this.#canvas.height);

    const ratio = this.#devicePixelRatio;
    this.stats.drawCalls += scene.draw(uniforms, sceneTransform, this.#palette, ratio);
    this.stats.drawCalls += overlay.draw(uniforms, overlayTransform, this.#palette, ratio);
    this.stats.frames += 1;
  }

  dispose(): void {
    this.#canvas.removeEventListener("webglcontextlost", this.#onContextLost);
    this.#canvas.removeEventListener("webglcontextrestored", this.#onContextRestored);
    const { program, quad, scene, overlay } = this.#resources;
    scene.dispose();
    overlay.dispose();
    this.#gl.deleteBuffer(quad);
    this.#gl.deleteProgram(program);
  }

  readonly #onContextLost = (event: Event): void => {
    // Signal that we will handle restoration ourselves.
    event.preventDefault();
  };

  readonly #onContextRestored = (): void => {
    // Every GPU object died with the context: rebuild, and forget the uploaded
    // revision so the next frame re-uploads from the current scene.
    this.#resources = createResources(this.#gl);
    this.#sceneRevision = undefined;
    this.onContextRestored?.();
  };
}

function createResources(gl: WebGL2RenderingContext): GpuResources {
  const program = linkProgram(gl, STROKE_VERTEX_SHADER, STROKE_FRAGMENT_SHADER);
  const uniform = (name: string): WebGLUniformLocation => {
    const location = gl.getUniformLocation(program, name);
    if (!location) throw new Error(`stroke shader is missing uniform ${name}`);
    return location;
  };

  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  // Triangle strip corners: (end selector, side).
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);

  return {
    program,
    quad,
    uniforms: {
      scale: uniform("u_scale"),
      translate: uniform("u_translate"),
      viewport: uniform("u_viewport"),
      halfWidth: uniform("u_half_width"),
      color: uniform("u_color"),
    },
    scene: new SegmentLayer(gl, quad, gl.STATIC_DRAW),
    overlay: new SegmentLayer(gl, quad, gl.DYNAMIC_DRAW),
  };
}
