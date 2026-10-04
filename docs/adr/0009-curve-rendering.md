# ADR-0009: Curve rendering — analytic arc instances under a covering strip

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves an EXPERIMENT item)  
**Design refs:** §11, §12.3, §12.5, §23.1–23.3; constrained by ADR-0003 and ADR-0008

## Context

DESIGN §12.5 and §23.3 leave the representation of circles and arcs open until an experiment compares:

- **A. zoom-adaptive tessellation** — turn each curve into straight segments within a screen-space chord error for the current zoom, cached by tolerance bucket with hysteresis;
- **B. analytic rendering** — send the curve itself and evaluate it in the shader;
- a hybrid.

Two earlier decisions constrain the answer. ADR-0003: pan and zoom must not regenerate or re-upload committed geometry. ADR-0008: geometry crosses the boundary as instance data in bulk. "A circle remains a circle in the document" (§11) holds either way; this is only about what the renderer receives.

## Decision

**Analytic arc instances, drawn under a covering strip.**

1. One instance per circle or arc: `cx, cy, radius, start, sweep` — five `f32`s, 20 bytes — with the centre relative to the frame origin and angles in radians, counter-clockwise. A circle is an arc with a sweep of 2π.
2. The vertex shader lays a thin ring-shaped triangle strip (32 quads) over the arc. The strip is only a conservative cover: its outer edge circumscribes the stroke and its ends are widened by the cap angle.
3. The fragment shader computes the exact distance from each pixel to the arc — radial inside the sweep, to the nearer endpoint outside it — and turns it into antialiased coverage. Lineweight is in pixels, and caps are round, exactly as for line strokes.
4. Arc instances travel in their own buffer with their own batch list, next to the segment buffer, in both render lifetimes.
5. The pixel position comes from `gl_FragCoord`, not from an interpolated attribute, so precision does not degrade across very large strip triangles.

Instance data does not depend on the view. Zooming changes uniforms only, so this fits ADR-0003 without amendment.

## Evidence

`pnpm --dir web bench:curves`: an in-page probe (`web/src/renderer/webgl/curve-probe.ts`) draws the same generated circles and arcs both ways on a 1280×800 off-screen canvas, in headless Chromium on SwiftShader (software GL), Intel i7-12650H. Half the arcs are full circles; radii are 1–40 units. Three views: *overview* (the whole drawing), *working* (4 px per unit, radii up to 160 px, heavy overlap), and *deep* (2000 px per unit on the edge of the largest circle, whose radius is then about 80,000 px). Tessellation targets a 0.25 px chord error. Timings wait for the GPU by reading a pixel back.

| 10,000 arcs | Overview | Working | Deep |
| --- | --- | --- | --- |
| **Analytic:** data (constant) | 0.2 MB | 0.2 MB | 0.2 MB |
| Analytic: draw one frame | 175 ms | 98 ms | 71 ms |
| **Tessellated:** segments needed | 96,801 | 295,176 | 6,495,042 |
| Tessellated: data | 1.5 MB | 4.7 MB | 104 MB |
| Tessellated: tessellate / upload | 5.2 ms / 0.4 ms | 13.5 ms / 0.8 ms | not attempted |
| Tessellated: draw one frame | 1952 ms | 1957 ms | not attempted |

At 1,000 arcs the same pattern holds: analytic 16 / 17 / 9 ms per frame; tessellated 313 / 329 ms, and 641,505 segments (10 MB) at deep zoom.

Agreement between the analytic image and a reference tessellated to 0.01 px, over 1,024,000 pixels:

| | Overview | Working | Deep (radius ≈ 80,000 px) |
| --- | --- | --- | --- |
| Pixels where one image is lit and the other dark, 1k arcs | 1 | 0 | 0 |
| Same, 10k arcs | 43 | 0 | 0 |

Reading the numbers:

- **Tessellation is view-dependent by construction.** The same 10,000 arcs need 97 thousand segments at one zoom and 6.5 million at another. Every bucket change would re-tessellate and re-upload the committed scene — megabytes, on a pointer-driven action — which is what ADR-0003 forbids. Making it tolerable needs viewport culling and clipping each arc to its visible span, on every zoom.
- **Analytic data is 20 bytes per arc at every zoom** and is uploaded once.
- **Analytic draws 11–20× faster here.** This is software rasterisation, where cost follows shaded pixels: thousands of short capped segments overlap each other at every joint, while the ring strip shades each pixel of the curve about once. The ratio on a real GPU is not established.
- **The shapes agree.** There is no hard disagreement at working or deep zoom, and at most 43 pixels in a million where circles are about a pixel across. The reference is the less exact of the two there: its segment joints blend twice.
- **Precision holds at a radius of 80,000 pixels.** Beyond roughly 10⁶ pixels, single-precision arithmetic in the shader is expected to wobble; that was not measured.

## Assessment against §23.3

| Criterion | Analytic arcs | Zoom-adaptive tessellation |
| --- | --- | --- |
| Visual quality | Exact curve, antialiased | Faceting bounded by the tolerance; visible while a bucket is stale |
| Zoom stability | No change in data; nothing to pop | Buckets and hysteresis; re-upload on change |
| Lineweight | Pixels, uniform-driven | Same, via the stroke shader |
| Linetype compatibility | Arc length is `radius × angle`, available per fragment; dashes can be computed in the shader | Accumulated distance per segment, as for lines |
| Selection rendering | Same instance, other style | Same |
| GPU/browser portability | WebGL2 only: instancing, `gl_VertexID`, `flat` varyings | Reuses the stroke shader |
| Implementation complexity | One more shader pair and instance buffer | Tessellator, cache, hysteresis, culling, span clipping |

## Alternatives considered

- **Zoom-adaptive tessellation (A).** Rejected for the committed scene on the ADR-0003 conflict and the data volumes above.
- **Tessellate once at a fixed fine angular step.** View-independent, but facets appear as soon as a circle is larger on screen than the step allows, and small circles carry hundreds of wasted segments.
- **A full-bounding-box quad per arc with the same fragment shader.** Simplest geometry, but it shades the whole interior of every circle; a few large circles would fill the screen many times over.
- **Tessellate in the vertex shader with a fixed number of segments and no analytic fragment stage.** Facets at large on-screen radii.

## Consequences

- The renderer gains a second program and a second kind of instance buffer; `PackedFrame` and the WASM boundary gain arc buffers and arc batches (bulk, same pull pattern).
- Draw order is segments, then arcs, within each lifetime. Only overlapping geometry of different styles could tell.
- Polyline arc segments (bulge, M3) can reuse the arc instance; joins between lines and arcs rely on round caps.
- Linetypes on arcs need shader work of their own when linetypes arrive (M4).
- Very large on-screen radii are bounded by `f32`. If real drawings hit that, the remedy is a small per-view pass that tessellates only the visible span of such arcs in `f64` — a hybrid for the extreme case, added on evidence.
- Overdraw grows with on-screen radius: the covering strip of a circle 80,000 px in radius is hundreds of pixels wide where it crosses the view. Fine for a few such arcs; worth measuring if a drawing has many.

## Revisit when

- Measurements on a real GPU are available (follow-up issue on real-hardware measurement).
- Precision artifacts appear at extreme zoom.
- Linetypes are implemented.
