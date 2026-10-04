// The WebGL2 adapter (DESIGN §12.1). TypeScript owns the context, the GPU
// buffers, and the draw calls; it never reads or writes CAD state.
import {
  FLOATS_PER_ARC,
  FLOATS_PER_SEGMENT,
  StyleRole,
  type ClipTransform,
  type FrameBuffers,
} from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import type { FrameSink } from "../render-frame";
import { ARC_FRAGMENT_SHADER, ARC_STRIP_VERTICES, ARC_VERTEX_SHADER } from "./arc-shaders";
import { InstanceLayer, type InstanceLayout, type StrokeUniforms } from "./instance-layer";
import { linkProgram } from "./program";
import { STROKE_FRAGMENT_SHADER, STROKE_VERTEX_SHADER } from "./shaders";

/**
 * Roles that travel in the overlay frame, because they depend on the view,
 * but are drawn beneath the committed scene: the background of the drawing.
 */
const UNDERLAY_ROLES: ReadonlySet<StyleRole> = new Set([StyleRole.GridMinor, StyleRole.GridMajor]);
const isUnderlay = (role: StyleRole): boolean => UNDERLAY_ROLES.has(role);
const isNotUnderlay = (role: StyleRole): boolean => !UNDERLAY_ROLES.has(role);
const everyRole = (): boolean => true;

/** Counters that let tests prove what the renderer did and did not do. */
export interface RendererStats {
  frames: number;
  drawCalls: number;
  /** Uploads of committed geometry. Must not grow with pointer activity. */
  sceneUploads: number;
  /** Uploads of transient geometry. */
  overlayUploads: number;
  /** GPU buffer reallocations, for either lifetime. */
  bufferReallocations: number;
}

/** One kind of primitive: its program and where its uniforms live. */
interface Pipeline {
  readonly program: WebGLProgram;
  readonly uniforms: StrokeUniforms;
  readonly layout: InstanceLayout;
}

/** The GPU buffers of one render lifetime: one layer per kind of primitive. */
class FrameLayers {
  readonly segments: InstanceLayer;
  readonly arcs: InstanceLayer;

  constructor(gl: WebGL2RenderingContext, pipelines: Pipelines, usage: number) {
    this.segments = new InstanceLayer(gl, pipelines.segments.layout, usage);
    this.arcs = new InstanceLayer(gl, pipelines.arcs.layout, usage);
  }

  /** Returns how many of the buffers had to be reallocated. */
  upload(frame: FrameBuffers): number {
    const segments = this.segments.upload(frame.segments, frame.batches);
    const arcs = this.arcs.upload(frame.arcs, frame.arcBatches);
    return Number(segments) + Number(arcs);
  }

  dispose(): void {
    this.segments.dispose();
    this.arcs.dispose();
  }
}

interface Pipelines {
  readonly segments: Pipeline;
  readonly arcs: Pipeline;
}

interface GpuResources {
  readonly pipelines: Pipelines;
  readonly quad: WebGLBuffer;
  /** Committed geometry: uploaded rarely, drawn often. */
  readonly scene: FrameLayers;
  /** Transient geometry: small, rewritten on most frames. */
  readonly overlay: FrameLayers;
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
    this.stats.bufferReallocations += this.#resources.scene.upload(frame);
    this.#sceneRevision = revision;
    this.stats.sceneUploads += 1;
  }

  updateOverlay(frame: FrameBuffers): void {
    this.stats.bufferReallocations += this.#resources.overlay.upload(frame);
    this.stats.overlayUploads += 1;
  }

  draw(sceneTransform: ClipTransform, overlayTransform: ClipTransform): void {
    const gl = this.#gl;
    const { pipelines, scene, overlay } = this.#resources;
    const [red, green, blue] = this.#palette.background;

    gl.viewport(0, 0, this.#canvas.width, this.#canvas.height);
    gl.clearColor(red, green, blue, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA); // premultiplied alpha

    // The grid, then committed geometry, then transient geometry on top.
    const passes: [FrameLayers, ClipTransform, (role: StyleRole) => boolean][] = [
      [overlay, overlayTransform, isUnderlay],
      [scene, sceneTransform, everyRole],
      [overlay, overlayTransform, isNotUnderlay],
    ];
    for (const [layers, transform, include] of passes) {
      for (const [pipeline, layer] of [
        [pipelines.segments, layers.segments],
        [pipelines.arcs, layers.arcs],
      ] as const) {
        this.stats.drawCalls += this.#drawLayer(pipeline, layer, transform, include);
      }
    }
    this.stats.frames += 1;
  }

  dispose(): void {
    this.#canvas.removeEventListener("webglcontextlost", this.#onContextLost);
    this.#canvas.removeEventListener("webglcontextrestored", this.#onContextRestored);
    const { pipelines, quad, scene, overlay } = this.#resources;
    scene.dispose();
    overlay.dispose();
    this.#gl.deleteBuffer(quad);
    this.#gl.deleteProgram(pipelines.segments.program);
    this.#gl.deleteProgram(pipelines.arcs.program);
  }

  #drawLayer(
    pipeline: Pipeline,
    layer: InstanceLayer,
    transform: ClipTransform,
    include: (role: StyleRole) => boolean,
  ): number {
    const gl = this.#gl;
    gl.useProgram(pipeline.program);
    gl.uniform2f(pipeline.uniforms.viewport, this.#canvas.width, this.#canvas.height);
    return layer.draw(pipeline.uniforms, transform, this.#palette, this.#devicePixelRatio, include);
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

function createPipeline(
  gl: WebGL2RenderingContext,
  vertexShader: string,
  fragmentShader: string,
  layout: InstanceLayout,
): Pipeline {
  const program = linkProgram(gl, vertexShader, fragmentShader);
  const uniform = (name: string): WebGLUniformLocation => {
    const location = gl.getUniformLocation(program, name);
    if (!location) throw new Error(`shader is missing uniform ${name}`);
    return location;
  };
  return {
    program,
    layout,
    uniforms: {
      scale: uniform("u_scale"),
      translate: uniform("u_translate"),
      viewport: uniform("u_viewport"),
      halfWidth: uniform("u_half_width"),
      color: uniform("u_color"),
    },
  };
}

function createResources(gl: WebGL2RenderingContext): GpuResources {
  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  // Triangle strip corners: (end selector, side).
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);

  const pipelines: Pipelines = {
    // A segment is extruded from a four-corner quad (ADR-0003).
    segments: createPipeline(gl, STROKE_VERTEX_SHADER, STROKE_FRAGMENT_SHADER, {
      floatsPerInstance: FLOATS_PER_SEGMENT,
      attributes: [{ location: 1, size: 4, offset: 0 }],
      verticesPerInstance: 4,
      perVertex: { buffer: quad, size: 2 },
    }),
    // An arc generates its own covering strip from gl_VertexID (ADR-0009).
    arcs: createPipeline(gl, ARC_VERTEX_SHADER, ARC_FRAGMENT_SHADER, {
      floatsPerInstance: FLOATS_PER_ARC,
      attributes: [
        { location: 0, size: 4, offset: 0 },
        { location: 1, size: 1, offset: 4 },
      ],
      verticesPerInstance: ARC_STRIP_VERTICES,
    }),
  };

  return {
    pipelines,
    quad,
    scene: new FrameLayers(gl, pipelines, gl.STATIC_DRAW),
    overlay: new FrameLayers(gl, pipelines, gl.DYNAMIC_DRAW),
  };
}
