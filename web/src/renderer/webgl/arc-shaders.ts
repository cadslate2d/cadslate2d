// Analytic arc shaders (ADR-0009).
//
// One instance is one circle or arc: centre, radius, start angle, sweep.
// The vertex shader lays a thin ring-shaped strip over the arc so that only
// fragments near the curve are shaded; the fragment shader then computes the
// exact distance to the arc. The strip is only a conservative cover — the
// curve itself is evaluated analytically, so there is no faceting at any
// zoom, and the instance data does not depend on the view. A linetype runs
// round the arc by arc length, in the direction its entity's path does
// (ADR-0015).

import {
  DASH_FRAGMENT_DECLARATIONS,
  DASH_FRAGMENT_FUNCTIONS,
  DISTANCE_ATTRIBUTE_LOCATION,
} from "./linetypes";
import { STYLE_VERTEX_DECLARATIONS, STYLE_VERTEX_RESOLVE } from "./style-table";

/** Quads in the covering strip. More means less overdraw on large circles. */
export const ARC_STRIP_SEGMENTS = 32;
export const ARC_STRIP_VERTICES = 2 * (ARC_STRIP_SEGMENTS + 1);

export const ARC_VERTEX_SHADER = `#version 300 es
precision highp float;

layout(location = 0) in vec4 a_arc;    // cx, cy, radius, start angle
layout(location = 1) in float a_sweep; // radians, counter-clockwise; 2π for a circle
// The path distance at the arc's start, drawing units; ±1 for the path's direction.
layout(location = ${DISTANCE_ATTRIBUTE_LOCATION}) in vec2 a_distance;

uniform vec2 u_scale;       // frame units → clip space
uniform vec2 u_translate;
uniform vec2 u_viewport;    // device pixels
${STYLE_VERTEX_DECLARATIONS}
flat out vec2 v_center;     // device pixels from the viewport centre
flat out float v_radius;    // device pixels
flat out float v_start;
flat out float v_sweep;
flat out vec2 v_distance;

const float TAU = 6.283185307179586;
const float SEGMENTS = ${ARC_STRIP_SEGMENTS}.0;

void main() {
  ${STYLE_VERTEX_RESOLVE}
  vec2 half_viewport = u_viewport * 0.5;
  vec2 center = (a_arc.xy * u_scale + u_translate) * half_viewport;
  float radius = a_arc.z * u_scale.x * half_viewport.x;
  float extent = v_half_width + 1.0;

  // Cover the round caps by widening the angular range by the cap's angle.
  float pad = extent / max(radius, 1e-3);
  float from = a_arc.w - pad;
  float span = a_sweep + 2.0 * pad;
  if (a_sweep >= TAU - 1e-4 || span >= TAU) {
    from = a_arc.w;
    span = TAU;
  }

  float along = float(gl_VertexID >> 1) / SEGMENTS;
  bool outer = (gl_VertexID & 1) == 1;
  float angle = from + span * along;
  // The outer edge circumscribes the stroke; the inner edge's chords only cut inwards.
  float outer_radius = (radius + extent) / cos(span / SEGMENTS * 0.5);
  float inner_radius = max(radius - extent, 0.0);
  vec2 position = center + (outer ? outer_radius : inner_radius) * vec2(cos(angle), sin(angle));

  v_center = center;
  v_radius = radius;
  v_start = a_arc.w;
  v_sweep = a_sweep;
  v_distance = a_distance;
  gl_Position = vec4(position / half_viewport, 0.0, 1.0);
}
`;

export const ARC_FRAGMENT_SHADER = `#version 300 es
precision highp float;

uniform vec2 u_viewport;    // device pixels

flat in vec4 v_color;        // straight alpha
flat in float v_half_width;  // device pixels
flat in vec2 v_center;
flat in float v_radius;
flat in float v_start;
flat in float v_sweep;
flat in vec2 v_distance;
${DASH_FRAGMENT_DECLARATIONS}
out vec4 out_color;

const float TAU = 6.283185307179586;
${DASH_FRAGMENT_FUNCTIONS}
void main() {
  // gl_FragCoord is exact, unlike an interpolated position on a huge triangle.
  vec2 from_center = gl_FragCoord.xy - u_viewport * 0.5 - v_center;
  float across = length(from_center) - v_radius;
  float distance_px = abs(across);

  // The angle turned from the arc's start, counter-clockwise.
  float turn = mod(atan(from_center.y, from_center.x) - v_start, TAU);
  if (v_sweep < TAU - 1e-4 && turn > v_sweep) {
    // Past an end: distance to the nearer endpoint gives a round cap.
    vec2 start = v_radius * vec2(cos(v_start), sin(v_start));
    vec2 end = v_radius * vec2(cos(v_start + v_sweep), sin(v_start + v_sweep));
    float to_start = distance(from_center, start);
    float to_end = distance(from_center, end);
    distance_px = min(to_start, to_end);
    turn = to_start < to_end ? 0.0 : v_sweep;
  }

  // Along the path by arc length, the way the path runs round the arc.
  float along = v_distance.x + v_distance.y * turn * v_radius / v_unit_px;
  distance_px = max(distance_px, length(vec2(to_dash(along), across)));

  float coverage = clamp(v_half_width + 0.5 - distance_px, 0.0, 1.0);
  float alpha = v_color.a * coverage;
  out_color = vec4(v_color.rgb * alpha, alpha); // premultiplied
}
`;
