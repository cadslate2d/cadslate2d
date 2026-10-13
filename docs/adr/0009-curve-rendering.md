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
- **Precision holds at a radius of 80,000 pixels,** and degrades gradually beyond it (measured below).

### Precision limit, measured in the application

After the arc layer was integrated, a circle of radius 100 was drawn with the CIRCLE command and zoomed in on its edge, counting lit pixels on a 580-pixel-high canvas. A correct result is a near-vertical stroke of about 930 pixels.

| On-screen radius | Lit pixels | What is seen |
| --- | --- | --- |
| 7×10² – 1.5×10⁴ px | 920–940 | Correct |
| 4×10⁴ px | 974 | Correct to the eye |
| 1×10⁵ px | 1074 | Stroke about 15% heavier |
| 3×10⁵ – 2×10⁶ px | 1160 | Stroke about 25% heavier, edges no longer antialiased |
| 6×10⁶ px | 580 | One aliased pixel wide |
| 1.6×10⁷ px | 0 | The curve disappears |

So the analytic path is exact up to an on-screen radius of roughly 10⁵ pixels, visibly coarser from there, and fails near 10⁷ — the limit of single-precision arithmetic, where one unit in the last place is about a pixel.

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
- Very large on-screen radii are bounded by `f32`: strokes coarsen above about 10⁵ pixels and vanish near 10⁷ (measured above). Lines have a comparable limit from the single scene origin (ADR-0003). The remedy for curves is a small per-view pass that tessellates only the visible span of such arcs in `f64` — a hybrid for the extreme case — tracked as a follow-up.
- Overdraw grows with on-screen radius: the covering strip of a circle 80,000 px in radius is hundreds of pixels wide where it crosses the view. Fine for a few such arcs; worth measuring if a drawing has many.

## Revisit when

- Measurements on a real GPU are available (follow-up issue on real-hardware measurement).
- Precision artifacts appear at extreme zoom.
- Linetypes are implemented.

## Extended in M7 (2026-10-12): the span of a curve past single precision (#112)

The follow-up above is done. `cad_render::FAR_CURVE_RADIUS_PIXELS` is 50,000: a committed circle or arc whose on-screen radius is past it — the polyline's arc segments and a block's arcs included, read through `Entity::primitives` — has the part of it that crosses the view tessellated into the overlay, in `f64` from the view centre, with a chord error under a quarter of a pixel, and the arc shader collapses an instance past 50,500 pixels (a per cent later, so the band between is drawn twice rather than not at all). The committed scene is untouched: pan and zoom upload nothing, which `zooming_far_in_on_a_curve_uploads_nothing_committed` holds, and `web/e2e/curves.spec.ts` sweeps the zoom of ADR-0009's table from 10² to past 10⁹ pixels of radius counting the stroke's pixels at every third e-fold. What the span does not carry: the entity's own colour, lineweight, and linetype, since the overlay has no style table — a far curve is drawn in the theme's entity colour at the ordinary width, continuous. A preview of such a curve, being overlay, is still the shader's and vanishes past the limit. Lines keep their limit from the single scene origin (ADR-0003); that is #151's.
