// The WebGL2 adapter (DESIGN §12.1). TypeScript owns the context, the GPU
// buffers, and the draw calls; it never reads or writes CAD state.
import {
  DirtyKind,
  FLOATS_PER_ARC,
  FLOATS_PER_ARC_DISTANCE,
  FLOATS_PER_SEGMENT,
  FLOATS_PER_TRIANGLE,
  StyleRole,
  dirtyRanges,
  type ClipTransform,
  type FrameBuffers,
  type StyleTables,
} from "../../cad/engine";
import type { CanvasPalette } from "../palette";
import type { FrameSink } from "../render-frame";
import {
  ARC_FRAGMENT_SHADER,
  ARC_STRIP_VERTICES,
  ARC_VERTEX_SHADER,
  FAR_CURVE_RADIUS_CSS_PIXELS,
  FAR_CURVE_SHADER_FACTOR,
} from "./arc-shaders";
import { FILL_FRAGMENT_SHADER, FILL_VERTEX_SHADER } from "./fill-shaders";
import { InstanceLayer, type InstanceLayout, type StrokeUniforms } from "./instance-layer";
import { patternTexels } from "./linetypes";
import { linkProgram } from "./program";
import { STROKE_FRAGMENT_SHADER, STROKE_VERTEX_SHADER } from "./shaders";
import { styleTexels } from "./style-table";

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
  /** Of those, the uploads of only the ranges that changed (ADR-0022). */
  scenePartialUploads: number;
  /** Uploads of the style table: a few bytes per style, never geometry. */
  styleUploads: number;
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
  /** Filled triangles (ADR-0021), drawn first so strokes lie over them. */
  readonly fills: InstanceLayer;
  readonly segments: InstanceLayer;
  readonly arcs: InstanceLayer;

  /** `styled`: every instance has a style index, and colours come from the style table. */
  constructor(gl: WebGL2RenderingContext, pipelines: Pipelines, usage: number, styled: boolean) {
    this.fills = new InstanceLayer(gl, pipelines.fills.layout, usage, styled);
    this.segments = new InstanceLayer(gl, pipelines.segments.layout, usage, styled);
    this.arcs = new InstanceLayer(gl, pipelines.arcs.layout, usage, styled);
  }

  /**
   * Uploads the frame — only the ranges it says changed, when it says and
   * the GPU already holds the rest (ADR-0022). Returns how many of the
   * buffers had to be reallocated.
   */
  upload(frame: FrameBuffers, partial: boolean): number {
    const ranges = (kind: DirtyKind) =>
      partial && frame.dirty ? dirtyRanges(frame.dirty, kind) : undefined;
    const fills = this.fills.upload(
      frame.triangles,
      frame.triangleBatches,
      frame.triangleStyles,
      undefined,
      ranges(DirtyKind.Triangles),
    );
    const segments = this.segments.upload(
      frame.segments,
      frame.batches,
      frame.segmentStyles,
      frame.segmentDistances,
      ranges(DirtyKind.Segments),
    );
    const arcs = this.arcs.upload(
      frame.arcs,
      frame.arcBatches,
      frame.arcStyles,
      frame.arcDistances,
      ranges(DirtyKind.Arcs),
    );
    return fills + segments + arcs;
  }

  dispose(): void {
    this.fills.dispose();
    this.segments.dispose();
    this.arcs.dispose();
  }
}

interface Pipelines {
  readonly fills: Pipeline;
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
  /** What each committed instance is drawn with: two texels per style (ADR-0014). */
  readonly styles: WebGLTexture;
  /** The linetype patterns the styles name: three float texels per pattern (ADR-0015). */
  readonly patterns: WebGLTexture;
}

export class WebGlRenderer implements FrameSink {
  readonly stats: RendererStats = {
    frames: 0,
    drawCalls: 0,
    sceneUploads: 0,
    scenePartialUploads: 0,
    styleUploads: 0,
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
  #styleRevision: number | undefined;
  /** The style rows last received, kept so the table can be laid out again. */
  #styleRows = new Uint8Array(0);
  #patternRows = new Float32Array(0);
  #showLineweights = false;
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
    if (devicePixelRatio !== this.#devicePixelRatio) {
      // Widths in the style table are in device pixels.
      this.#devicePixelRatio = devicePixelRatio;
      this.#uploadStyles();
    }
    const width = Math.max(1, Math.round(cssWidth * devicePixelRatio));
    const height = Math.max(1, Math.round(cssHeight * devicePixelRatio));
    if (this.#canvas.width !== width || this.#canvas.height !== height) {
      this.#canvas.width = width;
      this.#canvas.height = height;
    }
  }

  updateScene(revision: number, frame: FrameBuffers): void {
    // The GPU holds the previous scene: the ranges that changed suffice.
    const partial = this.#sceneRevision !== undefined && frame.dirty !== undefined;
    this.stats.bufferReallocations += this.#resources.scene.upload(frame, partial);
    this.#sceneRevision = revision;
    this.stats.sceneUploads += 1;
    if (partial) this.stats.scenePartialUploads += 1;
  }

  get styleRevision(): number | undefined {
    return this.#styleRevision;
  }

  updateStyles(revision: number, tables: StyleTables): void {
    // The rows are views into WASM memory: copy them before anything else runs.
    this.#styleRows = tables.styles.slice();
    this.#patternRows = tables.patterns.slice();
    this.#styleRevision = revision;
    this.#uploadStyles();
    this.#uploadPatterns();
  }

  /**
   * Whether lineweights are shown (the LWT toggle). Off, every committed
   * line is drawn at the ordinary width. Switching it lays the style table
   * out again; no geometry is uploaded.
   */
  set showLineweights(show: boolean) {
    if (show === this.#showLineweights) return;
    this.#showLineweights = show;
    this.#uploadStyles();
  }

  get showLineweights(): boolean {
    return this.#showLineweights;
  }

  #uploadStyles(): void {
    const gl = this.#gl;
    const entity = this.#palette.strokes[StyleRole.Entity];
    const texels = styleTexels(this.#styleRows, {
      // The theme's colour and width for ordinary geometry stand in for
      // "foreground" and "default".
      foreground: entity.color,
      defaultWidth: entity.width,
      lineweights: this.#showLineweights,
      devicePixelRatio: this.#devicePixelRatio,
    });
    gl.bindTexture(gl.TEXTURE_2D, this.#resources.styles);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA8,
      texels.width,
      texels.height,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      texels.data,
    );
    this.stats.styleUploads += 1;
  }

  #uploadPatterns(): void {
    const gl = this.#gl;
    const texels = patternTexels(this.#patternRows);
    gl.bindTexture(gl.TEXTURE_2D, this.#resources.patterns);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA32F,
      texels.width,
      texels.height,
      0,
      gl.RGBA,
      gl.FLOAT,
      texels.data,
    );
  }

  updateOverlay(frame: FrameBuffers): void {
    this.stats.bufferReallocations += this.#resources.overlay.upload(frame, false);
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
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.#resources.patterns);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.#resources.styles);

    // The grid, then committed geometry, then transient geometry on top.
    const passes: [FrameLayers, ClipTransform, (role: StyleRole) => boolean][] = [
      [overlay, overlayTransform, isUnderlay],
      [scene, sceneTransform, everyRole],
      [overlay, overlayTransform, isNotUnderlay],
    ];
    for (const [layers, transform, include] of passes) {
      for (const [pipeline, layer] of [
        [pipelines.fills, layers.fills],
        [pipelines.segments, layers.segments],
        [pipelines.arcs, layers.arcs],
      ] as const) {
        this.stats.drawCalls += this.#drawLayer(pipeline, layer, transform, include);
      }
    }
    this.stats.frames += 1;
  }

  /**
   * Waits for the GPU to finish what has been drawn, for measurements:
   * reading a pixel back is what actually waits (`gl.finish` returns before
   * a queued command runs in another process).
   */
  waitForGpu(): void {
    const pixel = new Uint8Array(4);
    this.#gl.readPixels(0, 0, 1, 1, this.#gl.RGBA, this.#gl.UNSIGNED_BYTE, pixel);
  }

  dispose(): void {
    this.#canvas.removeEventListener("webglcontextlost", this.#onContextLost);
    this.#canvas.removeEventListener("webglcontextrestored", this.#onContextRestored);
    const { pipelines, quad, scene, overlay, styles, patterns } = this.#resources;
    scene.dispose();
    overlay.dispose();
    this.#gl.deleteTexture(styles);
    this.#gl.deleteTexture(patterns);
    this.#gl.deleteBuffer(quad);
    this.#gl.deleteProgram(pipelines.fills.program);
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
    gl.uniform1i(pipeline.uniforms.styles, 0); // texture unit 0: the style table
    gl.uniform1i(pipeline.uniforms.patterns, 1); // texture unit 1: the pattern table
    gl.uniform1f(
      pipeline.uniforms.maxRadius,
      FAR_CURVE_RADIUS_CSS_PIXELS * FAR_CURVE_SHADER_FACTOR * this.#devicePixelRatio,
    );
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
    this.#styleRevision = undefined;
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
      // Used by the strokes; a fill's driver may compile them away.
      viewport: gl.getUniformLocation(program, "u_viewport"),
      halfWidth: gl.getUniformLocation(program, "u_half_width"),
      color: uniform("u_color"),
      styled: uniform("u_styled"),
      styles: uniform("u_styles"),
      patterns: gl.getUniformLocation(program, "u_patterns"),
      maxRadius: gl.getUniformLocation(program, "u_max_radius"),
    },
  };
}

function createResources(gl: WebGL2RenderingContext): GpuResources {
  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  // Triangle strip corners: (end selector, side).
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, -1, 0, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);

  const pipelines: Pipelines = {
    // A filled triangle is its three corners, one per vertex (ADR-0021).
    fills: createPipeline(gl, FILL_VERTEX_SHADER, FILL_FRAGMENT_SHADER, {
      floatsPerInstance: FLOATS_PER_TRIANGLE,
      attributes: [
        { location: 1, size: 4, offset: 0 },
        { location: 4, size: 2, offset: 4 },
      ],
      verticesPerInstance: 3,
      floatsPerDistance: 0,
    }),
    // A segment is extruded from a four-corner quad (ADR-0003).
    segments: createPipeline(gl, STROKE_VERTEX_SHADER, STROKE_FRAGMENT_SHADER, {
      floatsPerInstance: FLOATS_PER_SEGMENT,
      attributes: [{ location: 1, size: 4, offset: 0 }],
      verticesPerInstance: 4,
      perVertex: { buffer: quad, size: 2 },
      floatsPerDistance: 1,
    }),
    // An arc generates its own covering strip from gl_VertexID (ADR-0009).
    arcs: createPipeline(gl, ARC_VERTEX_SHADER, ARC_FRAGMENT_SHADER, {
      floatsPerInstance: FLOATS_PER_ARC,
      attributes: [
        { location: 0, size: 4, offset: 0 },
        { location: 1, size: 1, offset: 4 },
      ],
      verticesPerInstance: ARC_STRIP_VERTICES,
      floatsPerDistance: FLOATS_PER_ARC_DISTANCE,
    }),
  };

  // One texel each until the first tables arrive, so the samplers are
  // always complete: an opaque white style, and a pattern with no period.
  const lookup = (): WebGLTexture => {
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return texture;
  };
  const styles = lookup();
  gl.texImage2D(
    gl.TEXTURE_2D,
    0,
    gl.RGBA8,
    1,
    1,
    0,
    gl.RGBA,
    gl.UNSIGNED_BYTE,
    new Uint8Array([255, 255, 255, 255]),
  );
  const patterns = lookup();
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, 1, 1, 0, gl.RGBA, gl.FLOAT, new Float32Array(4));

  return {
    pipelines,
    quad,
    scene: new FrameLayers(gl, pipelines, gl.STATIC_DRAW, true),
    overlay: new FrameLayers(gl, pipelines, gl.DYNAMIC_DRAW, false),
    styles,
    patterns,
  };
}
