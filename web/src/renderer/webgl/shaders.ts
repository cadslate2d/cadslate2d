// Stroke shaders (DESIGN §23.1, ADR-0003).
//
// Each segment instance is extruded into a quad in the vertex shader, in
// device pixels, so lineweight is constant on screen at every zoom and the
// committed geometry never has to be regenerated when the view changes.
// The fragment shader computes the distance to the segment for antialiased
// edges and round caps, and to the nearest dash of its linetype (ADR-0015).
// Platform wide lines (GL_LINES) are never used.

import {
  DASH_FRAGMENT_DECLARATIONS,
  DASH_FRAGMENT_FUNCTIONS,
  DISTANCE_ATTRIBUTE_LOCATION,
} from "./linetypes";
import { STYLE_VERTEX_DECLARATIONS, STYLE_VERTEX_RESOLVE } from "./style-table";

export const STROKE_VERTEX_SHADER = `#version 300 es
precision highp float;

// Quad corner: x in {0,1} selects the segment end, y in {-1,+1} the side.
layout(location = 0) in vec2 a_corner;
// One instance: x0, y0, x1, y1, relative to the frame origin.
layout(location = 1) in vec4 a_segment;
// Where along its entity's path the segment starts, in drawing units.
layout(location = ${DISTANCE_ATTRIBUTE_LOCATION}) in float a_distance;

uniform vec2 u_scale;      // frame units → clip space
uniform vec2 u_translate;
uniform vec2 u_viewport;   // device pixels
${STYLE_VERTEX_DECLARATIONS}
// Position in the segment's own pixel frame: x along, y across.
out vec2 v_local;
out float v_length;
flat out float v_distance;

void main() {
  ${STYLE_VERTEX_RESOLVE}
  v_distance = a_distance;
  vec2 half_viewport = u_viewport * 0.5;
  vec2 start = (a_segment.xy * u_scale + u_translate) * half_viewport;
  vec2 end = (a_segment.zw * u_scale + u_translate) * half_viewport;

  vec2 delta = end - start;
  float len = length(delta);
  vec2 along = len > 1e-6 ? delta / len : vec2(1.0, 0.0);
  vec2 across = vec2(-along.y, along.x);

  // Grow the quad by the half width plus one pixel of antialiasing margin,
  // on the sides and past both ends (for the caps).
  float extent = v_half_width + 1.0;
  float end_sign = a_corner.x * 2.0 - 1.0;
  vec2 position = mix(start, end, a_corner.x)
    + along * end_sign * extent
    + across * a_corner.y * extent;

  v_local = vec2(a_corner.x * len + end_sign * extent, a_corner.y * extent);
  v_length = len;
  gl_Position = vec4(position / half_viewport, order_depth, 1.0);
}
`;

export const STROKE_FRAGMENT_SHADER = `#version 300 es
precision highp float;

flat in vec4 v_color;        // straight alpha
flat in float v_half_width;  // device pixels
in vec2 v_local;
in float v_length;
flat in float v_distance;    // drawing units
${DASH_FRAGMENT_DECLARATIONS}
out vec4 out_color;
${DASH_FRAGMENT_FUNCTIONS}
void main() {
  // Distance from this fragment to the segment (a capsule's medial axis).
  float nearest = clamp(v_local.x, 0.0, v_length);
  float distance_px = length(vec2(v_local.x - nearest, v_local.y));
  // Each dash is a capsule of its own, so dashes have round ends and a
  // dash of no length is a dot. On a continuous line this adds nothing.
  float dash_px = to_dash(v_distance + nearest / v_unit_px);
  distance_px = max(distance_px, length(vec2(dash_px, v_local.y)));
  float coverage = clamp(v_half_width + 0.5 - distance_px, 0.0, 1.0);
  float alpha = v_color.a * coverage;
  out_color = vec4(v_color.rgb * alpha, alpha); // premultiplied
}
`;
