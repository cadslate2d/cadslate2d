// The style table (ADR-0014): what each committed instance is drawn with.
//
// Every committed instance carries the index of a style. The styles
// themselves — a colour and a width each — are texels of a small texture
// that the vertex shader looks its index up in. So one draw call draws a
// whole layer of geometry in any number of colours and weights, and
// restyling a layer uploads a few bytes of this table and none of the
// geometry.
import { BYTES_PER_STYLE, STYLE_FOREGROUND, STYLE_WEIGHT_DEFAULT } from "../../cad/engine";
import type { Rgba } from "../palette";

/** Texels per row of the table texture. */
export const STYLE_TABLE_WIDTH = 1024;
/** A style is two texels: its colour, then its width. */
const TEXELS_PER_STYLE = 2;
/** Styles per row of the texture; more wrap to further rows. */
export const STYLES_PER_ROW = STYLE_TABLE_WIDTH / TEXELS_PER_STYLE;

/** Where the per-instance style index is bound, in both the stroke and the arc program. */
export const STYLE_ATTRIBUTE_LOCATION = 2;

/** A half width is stored in sixteenths of a device pixel, in one byte. */
const HALF_WIDTH_STEPS = 16;

/**
 * Vertex-shader declarations for a styled program. `u_styled` chooses where
 * the colour and the width come from: the table, for committed geometry, or
 * `u_color` and `u_half_width`, for the overlay, whose instances have no
 * style index.
 */
export const STYLE_VERTEX_DECLARATIONS = `
layout(location = ${STYLE_ATTRIBUTE_LOCATION}) in float a_style; // a row of the style table
uniform sampler2D u_styles;
uniform float u_styled;       // 1: style from the table; 0: from the uniforms
uniform vec4 u_color;         // straight alpha
uniform float u_half_width;   // device pixels
flat out vec4 v_color;        // straight alpha
flat out float v_half_width;  // device pixels
`;

/** Vertex-shader statements that set `v_color` and `v_half_width`. */
export const STYLE_VERTEX_RESOLVE = `
  if (u_styled > 0.5) {
    int style = int(a_style);
    ivec2 texel = ivec2((style % ${STYLES_PER_ROW}) * ${TEXELS_PER_STYLE}, style / ${STYLES_PER_ROW});
    v_color = texelFetch(u_styles, texel, 0);
    v_half_width = texelFetch(u_styles, texel + ivec2(1, 0), 0).r * 255.0 / ${HALF_WIDTH_STEPS}.0;
  } else {
    v_color = u_color;
    v_half_width = u_half_width;
  }
`;

/** What the theme and the session say about how styles look. */
export interface StyleLook {
  /** The theme's colour for ordinary geometry: what a "foreground" row is drawn in. */
  readonly foreground: Rgba;
  /** The theme's ordinary line width, in CSS pixels. */
  readonly defaultWidth: number;
  /** Whether lineweights are shown (the LWT toggle); off, every line has the ordinary width. */
  readonly lineweights: boolean;
  readonly devicePixelRatio: number;
}

/** CSS pixels per millimetre: a CSS pixel is 1/96 of an inch by definition. */
const PIXELS_PER_MILLIMETRE = 96 / 25.4;

/**
 * The width on screen, in CSS pixels, of a lineweight in hundredths of a
 * millimetre: its true size, and never thinner than the ordinary width — a
 * 0.05 mm line would otherwise vanish.
 */
export function lineweightWidth(hundredths: number, defaultWidth: number): number {
  return Math.max(defaultWidth, (hundredths / 100) * PIXELS_PER_MILLIMETRE);
}

export interface StyleTexels {
  readonly width: number;
  readonly height: number;
  /** RGBA8, straight alpha, `width × height` texels. */
  readonly data: Uint8Array;
}

/**
 * Lays the style rows out as texels: two per style, its colour and then its
 * half width in the red channel.
 *
 * A row whose kind is "foreground" has no colour of its own: it takes the
 * theme's, which is how colour 7 is light on a dark canvas and dark on a
 * light one. A row with the default weight — or any row while lineweights
 * are not shown — has the theme's ordinary width.
 */
export function styleTexels(rows: Uint8Array, look: StyleLook): StyleTexels {
  const count = Math.floor(rows.length / BYTES_PER_STYLE);
  const height = Math.max(1, Math.ceil(count / STYLES_PER_ROW));
  const data = new Uint8Array(STYLE_TABLE_WIDTH * height * 4);
  const byte = (channel: number) => Math.round(Math.min(1, Math.max(0, channel)) * 255);
  const theme = look.foreground.map(byte);
  for (let style = 0; style < count; style += 1) {
    const row = style * BYTES_PER_STYLE;
    const texel = style * TEXELS_PER_STYLE * 4;
    if (rows[row + 3] === STYLE_FOREGROUND) {
      data.set(theme, texel);
    } else {
      data.set([rows[row] ?? 0, rows[row + 1] ?? 0, rows[row + 2] ?? 0, 255], texel);
    }
    const weight = rows[row + 4] ?? STYLE_WEIGHT_DEFAULT;
    const width =
      look.lineweights && weight !== STYLE_WEIGHT_DEFAULT
        ? lineweightWidth(weight, look.defaultWidth)
        : look.defaultWidth;
    const halfWidth = (width * look.devicePixelRatio) / 2;
    data[texel + 4] = Math.min(255, Math.round(halfWidth * HALF_WIDTH_STEPS));
    data[texel + 7] = 255;
  }
  return { width: STYLE_TABLE_WIDTH, height, data };
}
