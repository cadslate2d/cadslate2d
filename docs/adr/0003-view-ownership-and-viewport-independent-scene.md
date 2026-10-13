# ADR-0003: View ownership and viewport-independent committed geometry

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Bootstrap (fills a gap in the baseline; changes no LOCKED decision)  
**Design refs:** §4.1, §7, §12.2–12.5, §18.3, §19.1, §23.1, §23.4, §26.3 (criteria 5–6)

## Context

M0 includes pan, zoom, line rendering, and click selection. The baseline says:

- pointer movement must not regenerate or re-upload committed geometry (§12.3, criterion 5);
- strokes are triangles with screen-space lineweight, not `GL_LINES` (§23.1);
- document coordinates are `f64`, GPU geometry may be camera-relative `f32` (§23.4);
- Rust owns "viewport-independent render preparation" (§7.2);
- viewport is not undoable (§19.1).

Three things are left unstated, and a naive reading produces a design that fails criterion 5:

1. **Who owns the camera?** §7 lists it on neither side.
2. **Where does screen-space stroke width get applied?** If Rust expands strokes into pixel-width triangles, every zoom step regenerates and re-uploads the whole committed scene. Panning and zooming are pointer-driven.
3. **What are `f32` vertices relative to?** "Camera-relative" taken literally means the scene changes whenever the camera moves.

## Decision

### 1. Rust owns the view

```rust
pub struct View {
    center: Point2,      // world, f64
    scale: f64,          // CSS pixels per world unit
    size: (f64, f64),    // viewport size in CSS pixels
}
```

Screen coordinates are CSS pixels throughout the core. The device pixel ratio is the adapter's concern alone: it sizes the canvas backing store and nothing in Rust depends on it. *(Amended on acceptance; the draft carried `dpr` and device pixels in `View`, which no core computation needed.)*

`View` is session state (ADR-0001). It is not part of `Drawing`, not in undo history, and not written to DXF. TypeScript forwards raw pointer, wheel, and resize events and holds no competing camera. All screen↔world conversion — picking tolerance, snap radius, zoom-about-cursor, window selection — goes through this one type.

### 2. View changes never change the committed scene

Pan, zoom, and resize raise `VIEW | OVERLAY` (ADR-0002). They do not change `SceneRevision`. A `RenderScene` is a function of the document only.

### 3. Stroke width is applied in the vertex shader

Rust emits each stroke segment in world space with enough information for the shader to extrude it: both endpoints plus a style reference. The vertex shader projects the endpoints to screen space and offsets by half the lineweight in pixels.

M0 starting layout: one **instance** per segment (`p0`, `p1`, style) drawn over a static four-vertex quad with `drawArraysInstanced`. Byte-level packing stays PROVISIONAL under the transport ADR.

Joins, caps, and miter limits (§23.1) are added within this model when polylines arrive in M1.

### 4. Vertices are `f32` offsets from a per-scene `f64` origin

Each `RenderScene` carries `origin: Point2` (f64), chosen by Rust when the scene is built. Vertex positions are `f32` offsets from it. Per frame, Rust computes the translation `(origin − view.center) × scale` in `f64` and hands the result to the shader as `f32` uniforms, so the large-magnitude subtraction never happens in single precision.

`OverlayFrame` uses the same convention with its own origin, re-chosen freely because it is rebuilt anyway.

### 5. Scene revision identifies committed buffer contents

`SceneRevision` changes on every canonical document mutation. It may also change when Rust rebuilds the scene around a new origin to restore precision. It never changes because of pointer movement inside an unchanged transient interaction.

M0 uses a single origin and implements no rebase. The rebase trigger and policy are deferred until large-coordinate drawings are exercised.

### 6. Curves are out of scope

Circle and arc representation stays open (§12.5). The curve-rendering ADR must either fit these constraints or explicitly amend this ADR. In particular, zoom-adaptive tessellation makes committed curve geometry depend on a zoom bucket; reconciling that with decision 2 is that ADR's job.

## Alternatives considered

- **TypeScript owns the camera and passes it with each event.** Rejected: two sources of truth for screen↔world, and headless tests could not exercise picking or zoom-about-cursor.
- **Rust expands strokes to pixel-width triangles on the CPU.** Rejected: re-upload on every zoom step; violates the intent of §12.3.
- **Absolute `f32` world coordinates.** Rejected: visible jitter far from the origin, which is common in survey-coordinate drawings.
- **Literal camera-relative vertices rebuilt as the camera moves.** Rejected: same regeneration problem as CPU expansion.
- **Four expanded vertices per segment instead of instancing.** Viable, roughly four times the data. Left to the transport measurement.

## Consequences

- Pan and zoom cost one uniform update and a redraw, regardless of drawing size.
- Lineweight in pixels is a uniform or style-table lookup, so zoom-stable lineweights need no CPU work.
- Linetype dash patterns (§23.2) will need accumulated path distance per segment in the instance data; the layout must leave room for it.
- With a single origin, a drawing whose extents are very large relative to the detail being viewed will lose sub-pixel precision at deep zoom. This is a known M0 limitation.
- Anything that needs screen-space geometry computed on the CPU (crosshair, snap markers, grips) belongs in `OverlayFrame`, which is rebuilt on view change.

## Revisit when

- The curve-rendering spike completes.
- Precision artifacts are observed on real drawings with large coordinates.
- The transport measurement shows instancing is a poor fit for batching or join geometry.

## Extended in M7 (2026-10-12): the single scene origin, far from the origin (#151)

The committed scene's one origin is the centre of the drawing's extents, and every instance is `f32` offsets from it. A drawing that lies wholly 10⁷ or 10¹⁰ units out is therefore drawn exactly: its offsets are small (the far-from-origin scenarios, in `cad-commands` and end to end). What is *not* exact is a drawing that spans such a distance — a title block at the origin and a site at 10⁷ — whose far end is 10⁷ from the scene origin, where an `f32` resolves to a unit: the site is placed to within a unit, which is a pixel at a pixel per unit and a hundred at a hundred (`web/e2e/far-from-origin.spec.ts` holds the first; `crates/cad-render/src/scene.rs` holds the bound). The rebase this ADR deferred is needed for that case and no other, and the shape it would take is now plain: the scene is already packed in chunks of entity IDs (ADR-0022), and a chunk can carry an origin of its own — its extents' centre — with the adapter drawing each chunk's run under its own translation. It is not taken up in M7: no drawing of the kind has been asked for, and the chunk origins are a day's work when one is.

