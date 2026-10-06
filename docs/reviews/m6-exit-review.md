# M6 exit review — Annotation and Hatch

**Date:** 2026-10-06  
**Scope:** DESIGN §34, as rewritten at the M5 exit ([roadmap](../roadmap.md))  
**Outcome:** Every item of the rewritten M6 scope is implemented and tested, an automated scenario exercises them together, and the milestone is closed. M6 put the first things on the sheet that are not geometry of the drawing: dimensions, laid out from their points by a style whenever they are asked for, so that a style change is one row however many dimensions it redraws; and hatches, the first areas, which reached a renderer that drew only lines as a third kind of committed instance. Both are saved and reopened byte for byte, read from other writers, and written so that a reader which does not lay dimensions out still draws them. M7 is broken down below.

§34 has no exit sentence beyond its ordering rule — text metrics, then the layout engine, then rendering, then hatch — which the work followed. The epic's "done when" list stood in for one.

## The scenario

`web/e2e/m6-scenario.spec.ts`, in Chromium and Firefox:

1. A plate is drawn — a closed polyline with a rounded end — and a hole in it.
2. A dimension style of its own, `Part` (text 5, one decimal), is made current with `DIMSTYLE`.
3. The plate is **dimensioned every way**: its length and height with `DIMLINEAR` (horizontal and vertical by where the line is placed), a slope with `DIMALIGNED`, the hole with `DIMRADIUS` and `DIMDIAMETER` picked by the pointer, and a corner with `DIMANGULAR` from a vertex and two points.
4. The region between the outline and the hole is **hatched** with `ANSI31` at scale 2, from a point inside it: lines appear between the two, none in the hole.
5. The style's text height is changed: the text **redraws** larger; one `U` puts it back, pixel for pixel.
6. `DXFOUT`: six `DIMENSION`s in `Part`, their values 180, 100, 70.711, 20, 40, and 90; the `DIMSTYLE` row with its height and decimals; `$DIMSTYLE` naming it; one `HATCH`, `ANSI31`, two loops, scale 2.
7. The file is reopened — "Imported 1 circle, 1 polyline, 6 dimensions, 1 hatch.", no `LOSSY` — and saved again: **the same bytes**.

## Done when (epic #105)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| The text metrics a layout engine relies on are named and held under property tests, and the renderer's strokes lie inside the document's text box | #161 | `cad-text`: `metrics_are_linear_in_the_height_and_bound_every_glyph`, `strokes_lie_within_the_metrics_box_at_any_angle`; `the_scenes_text_segments_lie_inside_the_documents_text_box` |
| Dimension styles are a table that changes through the one history, with `DIMSTYLE` and the DXF table both ways | #162 | `cad-document/src/drawing.rs` history property test with dimension-style operations; `cad-commands/src/tests/dimstyle.rs`; `dimension_styles_from_another_writer` |
| A dimension is definition points and a style, its geometry computed when asked for; five kinds with previews, grips, and one transaction each; a style change redraws every dimension | #163, #164, #165 | [ADR-0020](../adr/0020-dimensions.md); `cad-document/src/dimension.rs` property tests; `tests/dimension.rs`, `tests/dimlinear.rs`, `tests/dimcurve.rs`; `changing_the_style_redraws_every_dimension_in_one_step`; `web/e2e/dimensions.spec.ts`; the scenario |
| Hatches exist, solid and patterned, with `HATCH` picking a region or a boundary; the fill lies inside the boundary | #167, #168 | [ADR-0021](../adr/0021-filled-regions.md); `the_fill_lies_inside_the_boundary`, `every_piece_lies_inside_and_the_length_is_the_area_over_the_spacing`; `tests/hatch.rs`; `web/e2e/hatch.spec.ts` and the `hatch-ansi31.png` baseline |
| Dimensions and hatches survive DXF export and import | #166, #169 | `dimensions_from_another_writer`, `hatches_from_another_writer`; the corpus's `dimensions` and `hatches` cases for every release that has them, byte-identical round trips |

## What was built

- **Text metrics** (#161): `Font::metrics` — width, ascent, descent at a height — named as what a layout engine may rely on, with every stroke inside the box at any angle.
- **Dimension styles** (#162): the fifth table, `DIMSTYLE` with every variable and a default each, the current style a drawing setting, the `DIMSTYLE` table and `$DIMSTYLE` both ways.
- **The dimension entity** (#163, ADR-0020): what it measures, where it is placed, a style, and perhaps the drafter's words and text angle; a layout engine in `cad-document` over geometry in `cad-geometry`; `Tables` in place of the block table wherever geometry is resolved, so a reference and a dimension are read the same way; a style change re-lays out its dimensions in `apply`. The stroke font gained `°` and `Ø`.
- **Five dimension commands** (#164, #165): `DIMLINEAR` and `DIMALIGNED` from two points or a picked line or arc; `DIMANGULAR` from an arc, a circle, two lines, or a vertex and two points; `DIMRADIUS` and `DIMDIAMETER`; `Text` and `Angle` shared by all five.
- **Dimensions in DXF** (#166): every kind read from any release; written with the anonymous `*D` block holding its geometry as our layout draws it.
- **Filled regions** (#167, ADR-0021): triangles as a third committed instance kind, cut by ear clipping with holes bridged; `HatchEntity` with even-odd loops; `HATCH` from a point inside or selected boundaries.
- **Patterns** (#168): the classic set as `.pat` data with `SOLID` first; line families clipped and dashed in `cad-geometry`; `Pattern`, `Angle`, `SCale` remembered by the session; `HATCHEDIT`; `EXPLODE` of a pattern hatch.
- **Hatches in DXF** (#169): read from their groups in order — polyline and edge paths, arcs either way round; splines, ellipses, gradients, and patterns the table lacks reported.

## Decisions made in M6

| Decision | Where |
| --- | --- |
| A dimension stores what it measures and where it is placed; its lines, arrowheads, and text are laid out whenever asked for, never stored | ADR-0020 |
| Geometry that depends on a table is read through `Tables` — blocks and dimension styles — and `apply` re-indexes what follows a changed row | ADR-0020 |
| A dimension is an annotation: no primitives, nothing snaps to its lines; it offers its points, centre, and text | ADR-0020 |
| A filled region is triangles, a third committed instance kind, not a family of lines, which would depend on the zoom | ADR-0021 |
| A hatch's boundary is found from loops — closed curves and chains that meet end to end — not from faces of crossing lines | ADR-0021 |
| A pattern's dashes are cut in Rust, not by the linetype shader: they are the pattern's, not a style row's | ADR-0021; #168 |
| A pattern too fine for its region at its scale is refused (200,000 pieces), and filled solid when read from a file | #168, #169 |
| One invocation of a table command is one undo step, however many options it ran | `Session`; #162 |

## What testing found

- **The undo is per invocation.** DIMSTYLE's first tests expected one undo per option, as each option is its own transaction; the session groups an invocation into one step, as LAYER's and LINETYPE's tests already said. The tests were wrong, not the code.
- **An ear clipper's triangle test must not assume orientation.** With two holes bridged in, a reflex vertex of the bridge lay inside a clockwise candidate the test read as empty, and the fill came out at 404.5 units of area against 359. A test with two holes found it; the property test with one hole could not.
- **A repeated point is a segment of no length, and "on the segment" must say so.** A circle flattened to a ring ended a hair from its start; the on-edge test treated that zero-length edge as a line through the whole plane, and every point on its row was "inside the hole". The fix measures distance to the nearest point of the segment.
- **Drivers compile away what a shader does not use.** Firefox dropped the fill program's unused width and viewport uniforms; looking them up threw, and the canvas never appeared. Chromium kept them. Uniforms a program may not use are optional now.
- **Half-open crossings are a rule, not a bug.** A pattern line lying on a region's lower edge counts and one on the upper edge does not; the unit tests were changed to say so, and the property test (every piece's midpoint inside) holds.
- **Other writers' conventions.** ezdxf 1.4.4 writes an aligned dimension as a rotated one, does not write `DIMPOST`, stores a clockwise hatch arc with its angles negated, and needs R2004 for a gradient; each is in a fixture or a sidecar.

## Measurements

| What | Result |
| --- | --- |
| Cutting a region into triangles (ADR-0021), 10 / 100 / 1,000 vertices with two holes | 18 / 152 / 1,502 triangles, 432 B / 3.6 KB / 36 KB, 1.3 µs / 35 µs / 2.2 ms; a line fill a unit apart would be 225 / 363 / 2,700 segments and grow with the zoom |
| 10,000 dimensions: scene build (`cargo bench --bench annotation`) | 24 ms |
| 10,000 dimensions: a style change, every dimension re-laid out for the index, and the scene rebuilt | 49 ms |
| 1,000 hatches, half solid with a round hole, half ANSI31: scene build | 23 ms |
| ezdxf 1.4.4 auditing our re-export of the corpus | 52 files, 0 errors, 0 fixes; it reads our `*D` blocks' geometry, and our pattern lines are the ones it writes itself for the same pattern, angle, and scale |

**Not established:** any of it in WebKit or on a real GPU; how an interactive CAD program draws our `*D` blocks and hatches; scene builds on a drawing that mixes every kind at 100k (#171).

## What M6 taught us

- **The block model was the dimension model.** ADR-0017 had already answered "where does derived geometry live" for references; dimensions took the same answer and generalised its parameter from a block table to `Tables`. The compiler found every site in one pass.
- **Write the measure, not the picture.** A dimension that stores its points and a style round-trips through DXF, survives a style change, and grips naturally; everything a reader needs drawn goes in the anonymous block, written from the same layout.
- **An area is a different primitive, and the renderer said so.** Lines could not be a solid fill without depending on the zoom; a third instance kind cost one buffer, one program, and five coarse calls, and the line path stayed what patterns are made of.
- **Position-dependent records want their own reader.** HATCH's groups mean different things where they stand; keeping them in order and walking them was simpler and safer than teaching the field-absorbing reader about context.
- **Property tests over star-shaped regions catch less than one test with two holes.** The generators made one hole at a time; the bridge bug needed two. A generator is a hypothesis about where bugs are.

## Open items

Needs a person:

1. **An interactive CAD program reading our dimensions and hatches** — whether it re-lays out our `DIMENSION`s or draws our `*D` blocks, and draws our patterns as ezdxf reads them. Carried with the R2000 item from M5.
2. Still open from before: an interactive CAD program reading our R2000 files and a file from one in the corpus; the File System Access path in a real browser; WebKit (#109); real GPUs (#110); the R-tree crash upstream (#111); how M3 and M4 feel against classic CAD; the M2 exit assessment (#101); the arc grip behaviour (ADR-0010).

Carried into M7 as issues: #108, #112, #151 (agent work), with #109–#111 needing a person.

Known limitations, accepted for now:

- Dimensions are not associative; ordinate and arc-length dimensions, leaders, tolerances, and alternate units are not in the model (ordinate dimensions are reported on import).
- Arrowheads are closed outlines; now that fills exist they could be filled, which is annotation work for after M7.
- A hatch's boundary comes from loops; lines that cross without meeting at their ends bound nothing from a point. Spline and ellipse boundaries and gradients are reported, not held.
- A mirrored pattern hatch reflects its pattern's angle; families within a pattern keep their own, so an asymmetric pattern is not mirrored exactly. EXPLODE of a pattern hatch drops its dots.

## M7, broken down

§35 lists candidate hardening work and §42 the targets — 10k effortless, 50k smooth, 100k usable. Every measurement so far is of one kind of entity; M7 starts by measuring a mixed drawing and lets the numbers choose ([`docs/roadmap.md`](../roadmap.md) has the order):

1. **The bulk loader** (#108), carried since M0.
2. **A mixed 100k drawing measured against §42** (#171): open, first frame, pan and zoom frames, edit-to-frame, native and in two browsers, in a new `docs/performance.md`.
3. **An edit re-packs what changed** (#172), with an ADR on chunking; **viewport culling if the numbers ask for it** (#173).
4. **Strokes hardened** (#174): joins, caps, dots, and heavy lineweights, with baselines.
5. **Text at volume** (#175): the stroke font measured, an atlas decided.
6. **A panic leaves the drawing recoverable** (#176); **coverage-guided fuzzing** (#177); **the worker experiment** (#178).
7. **Curves at extreme zoom** (#112) and **drafting far from the origin** (#151), carried.
8. **Exit** (#179).
