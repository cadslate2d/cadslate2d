// The style table (ADR-0014): what each committed instance is drawn with.
//
// Every committed instance carries the index of a style. The styles
// themselves — a colour each, for now — are rows of a small texture that the
// vertex shader looks its index up in. So one draw call draws a whole layer
// of geometry in any number of colours, and recolouring a layer uploads a
// few bytes of this table and none of the geometry.
import { BYTES_PER_STYLE, STYLE_FOREGROUND } from "../../cad/engine";
import type { Rgba } from "../palette";

/** Texels per row of the table texture; more styles than this wrap to further rows. */
export const STYLE_TABLE_WIDTH = 1024;

/** Where the per-instance style index is bound, in both the stroke and the arc program. */
export const STYLE_ATTRIBUTE_LOCATION = 2;

/**
 * Vertex-shader declarations for a styled program. `u_styled` chooses where
 * the colour comes from: the table, for committed geometry, or `u_color`,
 * for the overlay, whose instances have no style index.
 */
export const STYLE_VERTEX_DECLARATIONS = `
layout(location = ${STYLE_ATTRIBUTE_LOCATION}) in float a_style; // a row of the style table
uniform sampler2D u_styles;
uniform float u_styled;  // 1: colour from the style table; 0: from u_color
uniform vec4 u_color;    // straight alpha
flat out vec4 v_color;   // straight alpha
`;

/** Vertex-shader statements that set `v_color`. */
export const STYLE_VERTEX_RESOLVE = `
  if (u_styled > 0.5) {
    int style = int(a_style);
    v_color = texelFetch(u_styles, ivec2(style % ${STYLE_TABLE_WIDTH}, style / ${STYLE_TABLE_WIDTH}), 0);
  } else {
    v_color = u_color;
  }
`;

export interface StyleTexels {
  readonly width: number;
  readonly height: number;
  /** RGBA8, straight alpha, `width × height` texels. */
  readonly data: Uint8Array;
}

/**
 * Lays the style rows out as texels. A row whose kind is "foreground" has no
 * colour of its own: it takes `foreground`, the theme's colour for ordinary
 * geometry, which is how colour 7 is light on a dark canvas and dark on a
 * light one.
 */
export function styleTexels(rows: Uint8Array, foreground: Rgba): StyleTexels {
  const count = Math.floor(rows.length / BYTES_PER_STYLE);
  const height = Math.max(1, Math.ceil(count / STYLE_TABLE_WIDTH));
  const data = new Uint8Array(STYLE_TABLE_WIDTH * height * 4);
  const byte = (channel: number) => Math.round(Math.min(1, Math.max(0, channel)) * 255);
  const theme = [
    byte(foreground[0]),
    byte(foreground[1]),
    byte(foreground[2]),
    byte(foreground[3]),
  ];
  for (let style = 0; style < count; style += 1) {
    const at = style * BYTES_PER_STYLE;
    if (rows[at + 3] === STYLE_FOREGROUND) {
      data.set(theme, style * 4);
    } else {
      data.set([rows[at] ?? 0, rows[at + 1] ?? 0, rows[at + 2] ?? 0, 255], style * 4);
    }
  }
  return { width: STYLE_TABLE_WIDTH, height, data };
}
