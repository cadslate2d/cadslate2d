# ADR-0021: Filled regions — triangles as a third committed instance kind, cut from the boundary when asked

**Status:** Accepted (2026-10-06)  
**Date:** 2026-10-06  
**Kind:** Evidence (resolves how an area reaches a renderer that draws lines; DESIGN §23 "triangle geometry" and §34 "basic hatch, solid fill")  
**Design refs:** §11, §12, §23, §34; ADR-0003, ADR-0008, ADR-0009, ADR-0014, ADR-0017, ADR-0020

## Context

A hatch is a closed region drawn filled — solid, or (#168) with a pattern of lines. It is the first thing in the drawing that is an area rather than a curve, and the renderer draws only segment and arc instances (ADR-0003, ADR-0009). Two ways were open:

1. **Triangles.** Cut the region into triangles in Rust, as a new committed instance kind beside segments and arcs, with its own buffer, batches, and style indices; the adapter draws them with a third program.
2. **A family of lines.** Draw the fill as parallel segments through the existing stroke pipeline, close enough together to look solid.

The second looks cheap, but committed geometry may not depend on the view (ADR-0003, §12.3): a line fill solid at one zoom shows gaps when zoomed in and piles up when zoomed out, unless it is regenerated as the view moves — which is exactly what the committed lifetime forbids. Lines at a fixed spacing in drawing units are a *pattern*, which is what #168 wants, not a solid. So the choice was triangles unless the experiment found them unaffordable.

## The experiment

`cargo test -p cad-geometry measure_fills -- --ignored --nocapture`, in release, on the development machine, for star-shaped regions with two star-shaped holes:

| Vertices (outer + 2 holes) | Triangles | Bytes on the wire | Time to cut | Lines a unit apart | Bytes | Time to clip |
| --- | --- | --- | --- | --- | --- | --- |
| 10 + 2×3 | 18 | 432 | 1.3 µs | 225 | 4,500 | 5.9 µs |
| 100 + 2×25 | 152 | 3,648 | 35 µs | 363 | 7,260 | 35 µs |
| 1,000 + 2×250 | 1,502 | 36,048 | 2.2 ms | 2,700 | 54,000 | 0.3 ms |

A triangle is six `f32`s (24 bytes) and a segment four plus a path distance (20 bytes). The triangle count is exact: a ring of *n* vertices with holes of *h₁, h₂ …* vertices cuts into *n + Σ(hᵢ + 2) − 2* triangles, independent of the region's size; the line count grows with the region's height over the spacing and with every ring crossed, and at one unit apart is already larger for every region measured — and it would have to grow with the zoom to stay solid. Ear clipping is O(n²): two milliseconds for a thousand-vertex boundary, which is a boundary nobody draws by hand, and it is paid once per scene rebuild, never per frame.

## Decision

### 1. A fill is triangle instances in a third committed buffer

`PackedFrame` gains `triangles` — six `f32`s per instance, the three corners as offsets from the frame origin, like a segment's ends — with their own batches, and `RenderScene` a style index per triangle (ADR-0014). Triangles carry no path distance: a fill has no linetype. The WASM boundary gains five coarse calls, one buffer each: `scene_triangles`, `scene_triangle_batches`, `scene_triangle_styles`, `overlay_triangles`, `overlay_triangle_batches`. The adapter draws fills with a third program — one vertex per corner, `gl_VertexID` choosing it, the colour from the style table or the batch's role — and draws them before the strokes of the same lifetime, so boundaries lie over fills. The committed/overlay model is unchanged: a fill being previewed or highlighted travels in the overlay frame like any preview.

### 2. The triangles are cut from the boundary whenever they are asked for

`HatchEntity` stores its boundary loops — closed paths of straight and arc segments, in any order and either orientation — and how it is filled. Nothing derived is stored, as for a block reference (ADR-0017) and a dimension (ADR-0020): `HatchEntity::fill` flattens each loop into a ring (arcs as chords at most 2π/64 apart), works out how deep each ring lies in the others, and for every ring at even depth cuts the region between it and the rings one level inside it into triangles — `cad_geometry::tessellate`, ear clipping with the holes bridged into the outer ring first. The even-odd rule makes a loop inside another a hole and a loop inside a hole an island, with no flags in the entity.

### 3. A hatch is an area, not a curve

A hatch is picked by its fill and its boundary (`contains` by the even-odd rule over the rings; `distance_to` is zero inside and the distance to the nearest boundary outside), selected by a window when its bounds are inside and by a crossing when its boundary crosses or the window lies in the fill. It has no primitives: nothing snaps to, intersects, trims against, or extends to it, and the curve commands refuse it by name; the boundary objects it was made from are still there for that. Its grips are the loop vertices, which move as a polyline's do. It is moved, rotated, scaled, and mirrored whole; a mirror flips each arc segment's bulge with the vertex order, so holes stay holes. A solid hatch has nothing to come apart into: EXPLODE refuses it.

### 4. The boundary is found from loops, not faces

`HATCH` takes a point inside the region: the loops the shown objects make — every closed polyline and circle as it is, and every chain of lines, arcs, and open polylines that meets end to end (`cad_document::join`) — are candidates; the smallest one around the point is the boundary, and every loop inside it that does not contain the point is a hole or an island. `Select` takes the loops the selected objects make instead. Lines that cross without meeting at their ends bound nothing yet: a face found by splitting every primitive at its intersections and walking the planar graph is the general answer, and is not needed for the drawings M6 makes.

## Alternatives considered

- **A fill of lines through the stroke pipeline.** Zoom-dependent, or a pattern; see above. Rejected for the solid; patterns (#168) are exactly this, at a spacing in drawing units, and are what the line path is for.
- **Tessellating in the adapter (TypeScript).** Geometry would then live on both sides of the boundary, against ADR-0002, and the spatial index and picking would still need the rings in Rust. Rejected.
- **A sweep-line or Delaunay triangulation.** O(n log n), but far more code for a step that costs two milliseconds at a thousand vertices. Revisit if boundaries that large become common.
- **Storing the triangles in the entity.** Fast to read, wrong after a grip edit unless rewritten, and a second copy of the geometry in the file and the history. Rejected, as for blocks and dimensions.
- **Antialiased fill edges.** The boundary is drawn as strokes over the fill, which antialias themselves; a fill edge of its own would double the work for no visible gain.

## Evidence

- `crates/cad-geometry/src/polygon.rs`: a square is two triangles and a square with a hole eight; concave rings and two holes fill without overlap; a path with arcs flattens to a ring that closes; under property tests over star-shaped rings with a star-shaped hole, the triangles cover exactly the ring less its hole, every centroid is inside the ring and outside the hole, every triangle is counter-clockwise with area, and the count is *n + Σ(hᵢ + 2) − 2*.
- `crates/cad-document/src/hatch.rs`: the even-odd region (a hole, an island in a hole), picking inside and outside, crossing, the refusals, grips, transforms; under property tests, the fill lies inside the boundary and adds up to its area, and moves with the hatch.
- `crates/cad-commands/src/tests/hatch.rs`: a point inside finds the smallest loop around it and the loops within; chains of lines and arcs bound a region and an open chain does not; `Select`; the preview is what the click makes and touches nothing; picking by the fill, MOVE, MIRROR, OFFSET's and EXPLODE's refusals.
- `web/e2e/hatch.spec.ts`: a rectangle with a circle in it hatched solid from a point inside — every pixel between them lit, none in the hole — picked by its fill and written to the file as a `HATCH`.
- `web/src/cad/render-boundary.test.ts`: the five new calls are the reviewed set.

## Consequences

- `Entity::Hatch`; `PackedFrame::push_triangle`, `triangles`, `triangle_batches`; `RenderScene::triangle_styles`; `FLOATS_PER_TRIANGLE`; an `InstanceLayout` may have no path distance.
- The modify matrix gains a Hatch column; `kind_name` "hatch"; the Properties panel shows a hatch's fill, loops, and area.
- DXF writes a solid `HATCH` with polyline boundary paths; reading `HATCH` is #169, until which a file's hatches are reported by name.
- A pattern hatch (#168) is this entity with a pattern style: its fill is the pattern's line families clipped to the rings, drawn as segments with the pattern's dashes — the line path, at a spacing in drawing units.

## Revisit when

- A drawing's boundaries run to thousands of vertices and the scene rebuild is slow: a sweep-line triangulation, or caching the triangles beside the spatial index entry, both inside this model.
- Regions bounded by crossing lines are wanted from a point: face finding over the planar graph of the shown primitives, split at their intersections.
- Gradient fills or transparency are wanted: a fill then needs more than a style row's colour.
