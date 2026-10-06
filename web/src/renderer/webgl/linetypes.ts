// Linetypes on the GPU (ADR-0015): the pattern table and the dash test.
//
// A style row names a pattern — dash and gap lengths in drawing units, with
// LTSCALE applied — and every committed instance carries the distance along
// its entity's path at which it begins. The fragment shader works out, from
// those two, how far each pixel is from the nearest dash along the line and
// treats that dash as a capsule, so dashes have round ends like the lines
// themselves and a dash of no length is a dot. The geometry never changes:
// zooming, rescaling, or restyling touches the tables and nothing else.
import { FLOATS_PER_PATTERN } from "../../cad/engine";

/** Where the per-instance path distance is bound, in both the stroke and the arc program. */
export const DISTANCE_ATTRIBUTE_LOCATION = 3;

/** A pattern is three texels: `period, count`, then the lengths four at a time. */
const TEXELS_PER_PATTERN = 3;
/** Patterns per row of the texture; more wrap to further rows. */
export const PATTERNS_PER_ROW = 256;
/** Texels per row of the pattern texture. */
export const PATTERN_TABLE_WIDTH = PATTERNS_PER_ROW * TEXELS_PER_PATTERN;

/** Below this many device pixels per repeat, dashes are noise: the line is drawn solid. */
export const FINEST_PERIOD_PX = 4;

export interface PatternTexels {
  readonly width: number;
  readonly height: number;
  /** RGBA32F, `width × height` texels. */
  readonly data: Float32Array;
}

/**
 * Lays the pattern rows out as float texels, three per pattern, so the
 * fragment shader can fetch a pattern by its row number.
 */
export function patternTexels(rows: Float32Array): PatternTexels {
  const count = Math.max(1, Math.floor(rows.length / FLOATS_PER_PATTERN));
  const height = Math.ceil(count / PATTERNS_PER_ROW);
  const data = new Float32Array(PATTERN_TABLE_WIDTH * height * 4);
  for (let pattern = 0; pattern < count; pattern += 1) {
    const row = pattern * FLOATS_PER_PATTERN;
    const texel = pattern * TEXELS_PER_PATTERN * 4;
    // Period and count fill the first texel; the eight lengths the next two.
    data[texel] = rows[row] ?? 0;
    data[texel + 1] = rows[row + 1] ?? 0;
    for (let length = 0; length < FLOATS_PER_PATTERN - 2; length += 1) {
      data[texel + 4 + length] = rows[row + 2 + length] ?? 0;
    }
  }
  return { width: PATTERN_TABLE_WIDTH, height, data };
}

/**
 * Fragment-shader declarations for a program that draws dashes. The vertex
 * shader supplies the pattern row and the scale (see `STYLE_VERTEX_RESOLVE`).
 */
export const DASH_FRAGMENT_DECLARATIONS = `
uniform sampler2D u_patterns;
flat in float v_pattern;   // a row of the pattern table
flat in float v_unit_px;   // device pixels per drawing unit
`;

/**
 * Fragment-shader function \`to_dash(along)\`: how far, in device pixels
 * along the line, the point of the path \`along\` drawing units from where
 * its entity starts is from the nearest dash. Zero inside a dash, and
 * everywhere on a line that is drawn continuous.
 */
export const DASH_FRAGMENT_FUNCTIONS = `
float to_dash(float along) {
  int row = int(v_pattern);
  ivec2 at = ivec2((row % ${PATTERNS_PER_ROW}) * ${TEXELS_PER_PATTERN}, row / ${PATTERNS_PER_ROW});
  vec4 head = texelFetch(u_patterns, at, 0);
  float period = head.x;
  // No pattern, or dashes closer together than a few pixels, which could
  // not be told apart and would shimmer: the line is solid (ADR-0015 §4).
  if (period <= 0.0 || period * v_unit_px < ${FINEST_PERIOD_PX}.0) return 0.0;
  vec4 a = texelFetch(u_patterns, at + ivec2(1, 0), 0);
  vec4 b = texelFetch(u_patterns, at + ivec2(2, 0), 0);
  float lengths[8] = float[8](a.x, a.y, a.z, a.w, b.x, b.y, b.z, b.w);
  int count = int(head.y);
  float position = mod(along, period);
  // The first dash of the next repeat is a neighbour too.
  float nearest = period - position;
  float from = 0.0;
  for (int index = 0; index < 8; index += 2) {
    if (index >= count) break;
    float dash = lengths[index];
    nearest = min(nearest, max(max(from - position, position - (from + dash)), 0.0));
    from += dash + lengths[index + 1];
  }
  return nearest * v_unit_px;
}
`;
