# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | Complete and closed ([review](reviews/m1-exit-review.md)). |
| M2 — OSNAP, Grips, and Basic Modify | Every deliverable is implemented ([review](reviews/m2-exit-review.md)). Open only for the hands-on assessment of its exit condition, which needs a person. |
| M3 — Robust Modify Geometry | Complete and closed ([review](reviews/m3-exit-review.md)). |
| M4 — Drawing Organization | Complete and closed ([review](reviews/m4-exit-review.md)). |
| M5 — DXF Compatibility | Complete and closed ([review](reviews/m5-exit-review.md)). |
| M6 — Annotation and Hatch | Next. Rewritten after M5 and broken into issues. |
| M7 | PROVISIONAL placeholder, to be broken down at the M6 exit review. |
| M8 | FUTURE. Not planned against. |

## M6, as rewritten after M5

DESIGN §34 lists candidate work for annotation and hatch, in a dependency order it insists on: stable text metrics, then the dimension layout engine, then dimension rendering, then hatch. It forbids dimensions before text metrics are reliable enough to place text, gaps, arrowheads, and extension lines consistently.

M5 left M6 these pieces: text drawn with a stroke font whose metrics are plain functions on the host (ADR-0016); a document whose entities carry properties and whose tables change through one history; a block model where a reference's geometry is computed from a table whenever it is asked for (ADR-0017) — the pattern a dimension will follow; a renderer that draws lines and arcs and nothing filled; a DXF reader and writer at R2000, where `DIMENSION`, `DIMSTYLE`, and `HATCH` all live; and a corpus and a matrix that a new record joins as rows.

### Order of work

1. **Text metrics for dimensions** (#161). §34's first item, taken literally: the width, cap height, ascent, descent, and box of a string are named as the API the layout engine may rely on and held under property tests, and the renderer's strokes are shown to lie inside the document's box. §34's second item, an improved text renderer, is not taken up: ADR-0016's stroke font stands until #110 measures rendering on hardware.
2. **Dimension styles** (#162). A table like the others — a row type, a `Change` variant, builder methods that refuse inconsistency — with `DIMSTYLE` and the DXF table both ways. Before the entity, because a dimension is drawn by its style.
3. **The dimension entity and its layout engine** (#163), with an ADR: definition points, a style, an optional text override; the geometry computed by pure functions whenever it is asked for, so a style change redraws every dimension as one row changes (§19.6 again); what each kind's grips hold fixed. Property tests over every kind before any command.
4. **The commands**: `DIMLINEAR` and `DIMALIGNED` (#164), then `DIMANGULAR`, `DIMRADIUS`, and `DIMDIAMETER` (#165), each with previews through the layout engine and one transaction; **dimensions in DXF** (#166), with the anonymous block readers expect.
5. **Hatch** (#167): an experiment and an ADR on how a filled region reaches a renderer that draws lines — triangles as a new committed instance kind, or a dense family of lines — then the entity, solid fill, and `HATCH`; **patterns** as line families clipped to the boundary (#168); **hatches in DXF** (#169).
6. **Exit** (#170): scenario, review, and the M7 breakdown, folding in the hardening issues that have waited since M2 (#108–#112, #151).

### Not in M6

- Associative dimensions, ordinate and arc-length dimensions, leaders, tolerances, alternate units.
- Gradient fills; hatch boundaries from splines or ellipses, which the model lacks.
- A second font or an atlas text renderer (ADR-0016; #110).
- `MLEADER`, tables, fields, and the rest of annotation beyond dimensions and hatches.

### Needs a person

- An interactive CAD program (AutoCAD, BricsCAD, LibreCAD, QCAD) reading our R2000 files — blocks, lineweights, paragraphs, `$INSUNITS`; ezdxf does, with no audit errors ([ADR-0018](adr/0018-dxf-release-r2000.md)). One file saved by such a program belongs in the corpus.
- The File System Access path in a real browser: open, edit, Ctrl+S, the file changed; the permission prompt; the handle after a reload ([M5 review](reviews/m5-exit-review.md)).
- WebKit, for the persistence measurement and the end-to-end suite.
- How M4 feels against classic CAD: `BLOCK`'s order of questions, `INSERT`'s prompts, `EXPLODE` one level at a time, the lineweight display rule, the stroke font ([M4 review](reviews/m4-exit-review.md)).
- How the M3 commands feel against classic CAD: round corners on polyline offsets, JOIN and gaps between arcs, the STRETCH window, BREAK on circles ([M3 review](reviews/m3-exit-review.md)).
- The hands-on assessment of the M2 exit condition.
- Confirming the arc grip behaviour against AutoCAD (ADR-0010).
- Measuring rendering on real GPU hardware.
- Deciding whether to report the R-tree bulk-load crash upstream.

## M5, as it was planned

Kept for the record; the review says how it went.

DESIGN §33 lists candidate work for DXF compatibility and local persistence. The order of work, after M4, was:

1. **The DXF release**, by ADR — R2000, so that lineweights and paragraphs are saved rather than reported — with **true colour** decided alongside.
2. **The compatibility matrix** and the **fixture corpus** from another writer.
3. **Parser hardening** against hostile files (§43).
4. **The import report and the lossy-export warning in the shell**.
5. **Local recovery persistence**, by ADR, then **saving to a file handle** where the browser has the API.
6. **Exit.**

Left out on purpose: DWG and opaque passthrough; entities the model lacks (counted and reported); a native project format; a persistence Worker; cloud storage.

## M4, as it was planned

Kept for the record; the review says how it went.

DESIGN §32 lists candidate work and fixes two things about how: every table changes through the unified transaction system, and redefining a block is one semantic change. The order of work, after M3, was:

1. **Entity properties and the generalized change model**, with an ADR, since everything else stands on it.
2. **Layers**, the **`LAYER` command** with text input for names, and **drawing units**.
3. **Colours across the render boundary**, by measurement, then **object colour** and **lineweights**.
4. **Linetypes**: an experiment and ADR on how dashes are drawn, then the table and commands.
5. **DXF for properties and tables**; the **Layers panel** and the **Properties panel** with `CHPROP`.
6. **Text**: an experiment and ADR on rendering and metrics, then `TEXT`, `MTEXT`, and their DXF.
7. **Blocks**: the model and ADR, then drawing, picking, and snapping, then `BLOCK`, `INSERT`, `EXPLODE`, and DXF.
8. **Exit.**

Left out on purpose: freeze and thaw, layer transparency, states and filters; true colour (decided with M5's release); complex linetypes and a linetype scale per object; text formatting codes, attributes, dynamic blocks, external references, non-uniform block scale; a grip menu; dynamic input near the cursor.

## M3, as it was planned

Kept for the record; the review says how it went.

DESIGN §31 lists candidate work and says the exact scope should be rewritten once real M2 geometry exists. Its order of work was:

1. **The polyline bulge model**, then DXF bulges and arc segments in `PLINE`.
2. **Perpendicular and tangent snaps.**
3. **Hover feedback for object picks**, then **TRIM and EXTEND** with several edges at once and with polylines as targets.
4. **OFFSET**: arcs, then polylines after an experiment and an ADR.
5. **FILLET and CHAMFER** with arcs and circles, then on polylines.
6. **BREAK, JOIN, STRETCH.**
7. **Exit.** The regression corpus and stress tests grew alongside, with one issue for the harness.

## Standing rules

- A PROVISIONAL or EXPERIMENT item is resolved by an ADR with evidence, not by default.
- Every milestone ends with a review like M0's: criteria linked to tests, lessons recorded, the next milestone's issues adjusted.
- Performance targets are checked with the generated drawings in `benches/` at 10k, 50k, and 100k entities.
