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
| M6 — Annotation and Hatch | Complete and closed ([review](reviews/m6-exit-review.md)). |
| M7 — Rendering and Performance Hardening | Complete and closed ([review](reviews/m7-exit-review.md)); the text atlas (#175) waits on real GPUs and moved to M8 with the other person-blocked items. |
| M8 — Compatibility Expansion | Next. Broken into issues at the M7 exit. |

## M8, as broken down after M7

DESIGN §36 lists possible work and says nothing in it should constrain M0. M8 takes what a drafter opening files from other programs meets first — the entities still reported on import, the layouts a save loses, the view commands classic drafting assumes — and leaves the rest as ideas on the epic (#107). The [M7 review](reviews/m7-exit-review.md) says what M7 left.

### Order of work

1. **ZOOM and PAN commands** (#180): `Extents`, `Window`, `Previous`, a scale, `Center`; `PAN` by two points.
2. **Ellipses** (#181), then **splines** (#182): entities with primitives by chords at a tolerance, grips, a command each, DXF both ways; a hatch bounded by a spline.
3. **Block attributes** (#183): `ATTDEF`, values on references, the prompts on `INSERT`, `ATTEDIT`, DXF both ways.
4. **Layouts and paper space kept through a round trip** (#184): opaque records, named, not drawn, not lost.
5. **Advanced selection** (#185): fence, window and crossing polygons, previous, last, all, `QSELECT`.
6. **User aliases and settings** (#186), in the browser's storage.
7. **PDF export of model space** (#187), the first form of plotting, with an ADR on the writer.
8. **Chunk origins** (#188) for drawings that span ten million units, carried from #151.
9. **Exit** (#189): a scenario on a file from another program, the review, and what follows.

### Not in M8

- SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38).
- Drawing layouts; associative dimensions.

### Needs a person

- Real GPUs (#110), which the text atlas (#175) waits on; WebKit (#109); the R-tree crash upstream (#111).
- An interactive CAD program reading our R2000 files; the File System Access path in a real browser; how the program feels against classic CAD; the arc grip behaviour (ADR-0010); the M2 exit assessment (#101).

## M7, as it was planned

DESIGN §35 lists candidate hardening work — strokes, caps and joins, linetypes, curves, packing, buffer reuse, a text atlas, culling, batching, 100k benchmarks, fuzzing, crash recovery, workers — and §42 the targets: 10k entities effortless, 50k smooth for normal editing, 100k usable. §35 insists a worker be justified by profiling; the same rule is applied to everything here.

M6 left M7 these pieces: a scene of three committed instance kinds — segments, arcs, triangles — rebuilt whole on every edit; dimensions and hatches whose geometry is computed whenever asked for, measured at 24 ms for 10,000 dimensions and 23 ms for 1,000 hatches; text drawn as strokes and now the bulk of every dimension; and measurements that are each of one kind of entity, mostly lines.

### Order of work

1. **The bulk loader** (#108), carried since M0: index construction is most of a large import.
2. **A mixed 100k drawing measured against §42** (#171). Every later choice reads its numbers; they go in a new `docs/performance.md`.
3. **An edit re-packs what changed** (#172), with an ADR on chunking the scene, then **viewport culling if the numbers ask for it** (#173).
4. **Strokes hardened** (#174): joins, caps, dots, heavy lineweights, with baselines.
5. **Text at volume** (#175): measured, and an atlas decided with #110's hardware numbers.
6. **A panic leaves the drawing recoverable** (#176); **coverage-guided fuzzing** of the reader and the grammar (#177); **the worker experiment** for open, export, and autosave (#178).
7. **Curves at extreme zoom** (#112) and **drafting far from the origin** (#151), carried; the scene-origin rebase policy (ADR-0003) is decided there.
8. **Exit** (#179).

### Not in M7

- New entities, commands, or file formats (M8, §36); a native project format (§39).
- Filled arrowheads and the rest of annotation's refinements.

### Needs a person

- WebKit, for the end-to-end suite and the persistence measurement (#109).
- Rendering and transport on real GPUs (#110), which #175 also waits on.
- Deciding whether to report the R-tree bulk-load crash upstream (#111).
- An interactive CAD program (AutoCAD, BricsCAD, LibreCAD, QCAD) reading our R2000 files — blocks, lineweights, paragraphs, `$INSUNITS`, and now dimensions and hatches; ezdxf does, with no audit errors. One file saved by such a program belongs in the corpus.
- The File System Access path in a real browser ([M5 review](reviews/m5-exit-review.md)).
- How M3 and M4 feel against classic CAD ([M3 review](reviews/m3-exit-review.md), [M4 review](reviews/m4-exit-review.md)); the M2 exit assessment; the arc grip behaviour against AutoCAD (ADR-0010).

## M6, as it was planned

Kept for the record; the [review](reviews/m6-exit-review.md) says how it went.

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
