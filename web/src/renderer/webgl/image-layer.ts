// Images (ADR-0042 §2): pictures beneath the strokes of their tier.
//
// A picture is a parallelogram — a corner and two edges, relative to the
// frame origin — showing a texture. Pictures are few, and each has a
// texture of its own, so each is one draw of a unit quad with its corner
// and edges as uniforms, not an instanced run. The textures are kept by the
// picture's fingerprint: its bytes cross the WASM boundary once, are
// decoded by the browser, and stay until the picture leaves the drawing.

import { linkProgram } from "./program";

/** Floats per image instance: `x, y, ax, ay, bx, by`. */
export const FLOATS_PER_IMAGE = 6;
/** Words per image instance: `definition, fade`. */
export const WORDS_PER_IMAGE = 2;
/** Words per picture in `imageSources`: `definition, digest_low, digest_high, width, height, format`. */
export const WORDS_PER_IMAGE_SOURCE = 6;
/** How far apart two tiers lie in depth; the same as the strokes' and fills' (ADR-0032). */
const TIERS_PER_DEPTH = 32769;

export const IMAGE_VERTEX_SHADER = `#version 300 es
precision highp float;

uniform vec2 u_scale;      // frame units → clip space
uniform vec2 u_translate;
uniform vec2 u_corner;     // the picture's lower-left corner, frame units
uniform vec2 u_across;     // its bottom edge
uniform vec2 u_up;         // its left edge
uniform float u_depth;     // its tier's depth
out vec2 v_uv;

void main() {
  // A strip over the unit square: (0, 0), (1, 0), (0, 1), (1, 1).
  vec2 st = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 at = u_corner + u_across * st.x + u_up * st.y;
  // The picture's first row is its top.
  v_uv = vec2(st.x, 1.0 - st.y);
  gl_Position = vec4(at * u_scale + u_translate, u_depth, 1.0);
}
`;

export const IMAGE_FRAGMENT_SHADER = `#version 300 es
precision highp float;

uniform sampler2D u_image;
uniform float u_fade;        // 0–1: how far toward the background
uniform vec3 u_background;
uniform float u_alpha;       // 1 committed; less for a preview
in vec2 v_uv;
out vec4 out_color;

void main() {
  vec3 color = mix(texture(u_image, v_uv).rgb, u_background, u_fade);
  out_color = vec4(color * u_alpha, u_alpha); // premultiplied
}
`;

interface ImageUniforms {
  readonly scale: WebGLUniformLocation | null;
  readonly translate: WebGLUniformLocation | null;
  readonly corner: WebGLUniformLocation | null;
  readonly across: WebGLUniformLocation | null;
  readonly up: WebGLUniformLocation | null;
  readonly depth: WebGLUniformLocation | null;
  readonly image: WebGLUniformLocation | null;
  readonly fade: WebGLUniformLocation | null;
  readonly background: WebGLUniformLocation | null;
  readonly alpha: WebGLUniformLocation | null;
}

/** The program pictures are drawn with. */
export class ImageProgram {
  readonly program: WebGLProgram;
  readonly uniforms: ImageUniforms;
  readonly #vao: WebGLVertexArrayObject;

  constructor(gl: WebGL2RenderingContext) {
    this.program = linkProgram(gl, IMAGE_VERTEX_SHADER, IMAGE_FRAGMENT_SHADER);
    const at = (name: string) => gl.getUniformLocation(this.program, name);
    this.uniforms = {
      scale: at("u_scale"),
      translate: at("u_translate"),
      corner: at("u_corner"),
      across: at("u_across"),
      up: at("u_up"),
      depth: at("u_depth"),
      image: at("u_image"),
      fade: at("u_fade"),
      background: at("u_background"),
      alpha: at("u_alpha"),
    };
    // No attributes: the quad comes from gl_VertexID.
    this.#vao = gl.createVertexArray();
  }

  /** Readies the program to draw: texture unit `unit` for the pictures. */
  begin(gl: WebGL2RenderingContext, unit: number, background: readonly number[]): void {
    gl.useProgram(this.program);
    gl.bindVertexArray(this.#vao);
    gl.uniform1i(this.uniforms.image, unit);
    gl.uniform3f(
      this.uniforms.background,
      background[0] ?? 1,
      background[1] ?? 1,
      background[2] ?? 1,
    );
  }

  dispose(gl: WebGL2RenderingContext): void {
    gl.deleteProgram(this.program);
    gl.deleteVertexArray(this.#vao);
  }
}

/** A picture's texture, once its bytes are decoded. */
interface Picture {
  texture: WebGLTexture | undefined;
}

/** A missing picture's ground, or one not decoded yet: a light grey. */
const PLACEHOLDER = new Uint8Array([224, 224, 224, 255]);

/**
 * The textures of the drawing's pictures, by fingerprint (ADR-0042 §2). A
 * picture missing from the drawing (§4), or not decoded yet, is drawn as a
 * light grey ground in its frame; when one is ready, `onReady` asks for a
 * frame.
 */
export class ImageTextures {
  readonly #gl: WebGL2RenderingContext;
  readonly #byDigest = new Map<string, Picture>();
  readonly #placeholder: WebGLTexture;
  /** The digest each definition shows. */
  #digests = new Map<number, string>();
  onReady: (() => void) | undefined;

  constructor(gl: WebGL2RenderingContext) {
    this.#gl = gl;
    this.#placeholder = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.#placeholder);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, PLACEHOLDER);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  }

  /**
   * The drawing's pictures, as `imageSources` lists them: decodes each new
   * one from `bytes`, and lets go of those no longer listed.
   */
  update(sources: Uint32Array, bytes: (definition: number) => Uint8Array): void {
    const digests = new Map<number, string>();
    const listed = new Set<string>();
    for (let at = 0; at + WORDS_PER_IMAGE_SOURCE <= sources.length; at += WORDS_PER_IMAGE_SOURCE) {
      const definition = sources[at] ?? 0;
      const digest = `${sources[at + 2] ?? 0}:${sources[at + 1] ?? 0}`;
      const jpeg = sources[at + 5] === 1;
      digests.set(definition, digest);
      listed.add(digest);
      if (!this.#byDigest.has(digest)) {
        const picture: Picture = { texture: undefined };
        this.#byDigest.set(digest, picture);
        void this.#decode(picture, bytes(definition), jpeg ? "image/jpeg" : "image/png");
      }
    }
    for (const [digest, picture] of this.#byDigest) {
      if (listed.has(digest)) continue;
      if (picture.texture) this.#gl.deleteTexture(picture.texture);
      this.#byDigest.delete(digest);
    }
    this.#digests = digests;
  }

  /** The texture of `definition`, once it is decoded; the placeholder until then, or for good when it is missing. */
  texture(definition: number): WebGLTexture {
    const digest = this.#digests.get(definition);
    const texture = digest === undefined ? undefined : this.#byDigest.get(digest)?.texture;
    return texture ?? this.#placeholder;
  }

  async #decode(picture: Picture, bytes: Uint8Array, type: string): Promise<void> {
    if (bytes.length === 0 || typeof createImageBitmap !== "function") return;
    let bitmap: ImageBitmap;
    try {
      // A copy on an ArrayBuffer of its own: what a Blob takes.
      bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type }));
    } catch {
      return; // Not a picture the browser decodes: it draws nothing.
    }
    // Let go of while decoding.
    if (![...this.#byDigest.values()].includes(picture)) {
      bitmap.close();
      return;
    }
    const gl = this.#gl;
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    bitmap.close();
    picture.texture = texture;
    this.onReady?.();
  }

  dispose(): void {
    for (const picture of this.#byDigest.values()) {
      if (picture.texture) this.#gl.deleteTexture(picture.texture);
    }
    this.#byDigest.clear();
    this.#gl.deleteTexture(this.#placeholder);
  }
}

/** The pictures of one render lifetime: copies, drawn one by one. */
export class ImageLayer {
  #images = new Float32Array(0);
  #words = new Uint32Array(0);
  #styles: Uint32Array | undefined;

  get count(): number {
    return Math.floor(this.#images.length / FLOATS_PER_IMAGE);
  }

  /** Keeps copies: the arguments are views into WASM memory. */
  upload(images: Float32Array, words: Uint32Array, styles?: Uint32Array): void {
    this.#images = images.slice();
    this.#words = words.slice();
    this.#styles = styles?.slice();
  }

  /**
   * Draws the pictures `[first, count]` under `transform` (four floats at
   * `at`). Returns the draw calls.
   */
  draw(
    gl: WebGL2RenderingContext,
    program: ImageProgram,
    textures: ImageTextures,
    transform: Float32Array,
    at: number,
    first: number,
    count: number,
    alpha: number,
  ): number {
    const { uniforms } = program;
    gl.uniform2f(uniforms.scale, transform[at] ?? 0, transform[at + 1] ?? 0);
    gl.uniform2f(uniforms.translate, transform[at + 2] ?? 0, transform[at + 3] ?? 0);
    gl.uniform1f(uniforms.alpha, alpha);
    let drawCalls = 0;
    for (let index = first; index < first + count; index += 1) {
      const texture = textures.texture(this.#words[index * WORDS_PER_IMAGE] ?? 0);
      const f = index * FLOATS_PER_IMAGE;
      const image = this.#images;
      gl.uniform2f(uniforms.corner, image[f] ?? 0, image[f + 1] ?? 0);
      gl.uniform2f(uniforms.across, image[f + 2] ?? 0, image[f + 3] ?? 0);
      gl.uniform2f(uniforms.up, image[f + 4] ?? 0, image[f + 5] ?? 0);
      gl.uniform1f(uniforms.fade, (this.#words[index * WORDS_PER_IMAGE + 1] ?? 0) / 100);
      // The tier, sign and all, as the fills' depth (ADR-0032).
      const word = this.#styles?.[index] ?? 0;
      gl.uniform1f(uniforms.depth, this.#styles ? -(word >> 16) / TIERS_PER_DEPTH : 0);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      drawCalls += 1;
    }
    return drawCalls;
  }
}
