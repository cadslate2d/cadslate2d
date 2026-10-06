// Fill shaders (ADR-0021).
//
// A filled region reaches the GPU as triangles: one instance per triangle,
// its three corners relative to the frame origin, one vertex per corner.
// A fill has a colour and nothing else — no width, no linetype — so the
// fragment is the colour, premultiplied, with no antialiasing of its own:
// the boundary is drawn as strokes over it.

import { STYLE_VERTEX_DECLARATIONS, STYLE_VERTEX_RESOLVE } from "./style-table";

export const FILL_VERTEX_SHADER = `#version 300 es
precision highp float;

// One instance: the first two corners …
layout(location = 1) in vec4 a_corners;
// … and the third, relative to the frame origin.
layout(location = 4) in vec2 a_third;

uniform vec2 u_scale;      // frame units → clip space
uniform vec2 u_translate;
uniform vec2 u_viewport;   // device pixels, unused here but shared with the strokes
${STYLE_VERTEX_DECLARATIONS}

void main() {
  ${STYLE_VERTEX_RESOLVE}
  vec2 corner = gl_VertexID == 0 ? a_corners.xy : (gl_VertexID == 1 ? a_corners.zw : a_third);
  gl_Position = vec4(corner * u_scale + u_translate, 0.0, 1.0);
}
`;

export const FILL_FRAGMENT_SHADER = `#version 300 es
precision highp float;

flat in vec4 v_color;        // straight alpha
flat in float v_half_width;  // unused: a fill has no width
flat in float v_pattern;     // unused: a fill has no linetype
flat in float v_unit_px;
out vec4 out_color;

void main() {
  float alpha = v_color.a;
  out_color = vec4(v_color.rgb * alpha, alpha); // premultiplied
}
`;
