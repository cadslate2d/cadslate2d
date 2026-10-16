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
| M8 — Compatibility Expansion | Complete and closed ([review](reviews/m8-exit-review.md)). |
| M9 — Paper Space | Complete and closed ([review](reviews/m9-exit-review.md)). |
| M10 — Daily Drafting | Complete and closed ([review](reviews/m10-exit-review.md)). |
| M11 — Polylines, Leaders, and Loose Ends | Complete and closed ([review](reviews/m11-exit-review.md)). |
| M12 — Drafting Conveniences | Complete and closed ([review](reviews/m12-exit-review.md)). |
| M13 — Drafting Aids and Reuse | Complete and closed ([review](reviews/m13-exit-review.md)). |
| M14 — Production Annotation | Complete and closed ([review](reviews/m14-exit-review.md)). The product is complete for its purpose. |
| M15 — v1 Verification (needs a person) | Open: verification on real GPUs, WebKit, AutoCAD, and a desktop PDF viewer, which needs a person. No work for an agent on its own. |
| M16 — Occasional Conveniences | Complete and closed ([review](reviews/m16-exit-review.md)): `OVERKILL`, named views, `LAYMRG` and `LAYDEL`, `SPLINEDIT`. |
| M17 — Multileaders | In progress: the entity, its style, `MLEADER`, block notes, `MLEADEREDIT`, `MLEADERSTYLE`, and DXF both ways are done (#240–#242); the exit is next. |

## M17, multileaders

After M16 the user chose the first long-term idea, `MLEADER`. The epic is #239; [ADR-0033](adr/0033-multileaders.md) decides how a multileader is kept: its leaders, one landing, its content, and its style are stored, and the style lays the rest out, as dimensions are (ADR-0020).

### Order of work

1. **The entity, its style table, and `MLEADER`** (#240), with DXF `MULTILEADER` and `MLEADERSTYLE` for text notes, so that saving keeps them.
2. **Block notes, `MLEADEREDIT`, and `MLEADERSTYLE`** (#241).
3. **DXF block notes** (#242): the rest of the file side, once a note can be a block.
4. **Exit** (#243).

## M16, the occasional conveniences

After the M14 review declared the product complete, the user asked to go on. M16 takes up the conveniences that review set aside; the epic is #233.

### Order of work

1. **`OVERKILL`** (#234): duplicates removed, overlapping collinear lines merged.
2. **Named views** (#235): `VIEW`, kept in the drawing and its DXF `VIEW` table.
3. **`LAYMRG` and `LAYDEL`** (#236): layers merged onto another, or deleted with what is on them.
4. **`SPLINEDIT`** (#237): close or open, reverse, fit points added and deleted, a polyline.
5. **Exit** (#238).

## M15, as broken down after M14

The [M14 review](reviews/m14-exit-review.md) sweeps the registry against the classic daily command set and DESIGN §47 once more. It finds every daily command there and every item of a successful v1 met: **the product is complete for its purpose.** No milestone of features follows. The epic is #231.

### What remains (needs a person)

1. **Real GPUs** (#110), and the text atlas decided from those figures (#175).
2. **WebKit** (#109).
3. **The R-tree crash, reported upstream** (#111).
4. **A check against AutoCAD and a desktop PDF viewer** (#232) of what the program writes: groups, wipeouts, ordinates, layer states, the draw order, and plotted layouts.

### Long-term ideas

- `MLEADER`, tables, fields; raster image underlays; SHX fonts; xrefs; scripting; plugins; native packaging; a DWG strategy; a native project format (§39); a WebGPU adapter (§38); associative dimensions; advanced hatch; plot style tables; searchable PDF text; the system clipboard; freezing in one viewport.
- Occasional conveniences, when drafters ask for them: `OVERKILL`, named views (`VIEW`), `LAYMRG` and `LAYDEL`, `SPLINEDIT`.

## M14, as broken down after M13

The drafting aids and reuse shipped in M13. The [M13 review](reviews/m13-exit-review.md) sweeps the registry again and finds what production sheets still reach for. M14 takes it up; the epic is #225.

### Order of work

1. **Draw order and wipeouts, by an ADR** (#226): `DRAWORDER`, `TEXTTOFRONT`, `WIPEOUT`; DXF `SORTENTSTABLE` and `WIPEOUT`.
2. **Ordinate dimensions and QDIM** (#227).
3. **Saved layer states** (#228): `LAYERSTATE`, kept in the drawing and its file.
4. **BOUNDARY** (#229): the loop HATCH finds, as a polyline.
5. **Exit** (#230): the scenario, the review with the registry swept again, and whether the product is complete.

### Not in M14

- `MLEADER`, tables, fields; raster image underlays; SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch, plot style tables, searchable PDF text; the system clipboard; freezing in one viewport.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream; the PDF of a layout in a desktop viewer.

## M13, as broken down after M12

Kept for the record; the [review](reviews/m13-exit-review.md) says how it went.

The conveniences shipped in M12. The [M12 review](reviews/m12-exit-review.md) sweeps the registry again and finds two drafting aids the design's own status bar names (§5) unbuilt, text styles that only a file can make, and no reuse across drawings. M13 takes them up; the epic is #218.

### Order of work

1. **Grid snap** (#219): `SNAP` and F9, in `Session::aim` after object snap.
2. **Object snap tracking** (#220): `OTRACK` and F11, alignment paths from acquired snap points.
3. **Text styles** (#221): `STYLE` with a height, a width factor, and an oblique angle.
4. **A drawing as a block, a block as a drawing** (#222): `INSERT` a DXF file, `WBLOCK`.
5. **The clipboard** (#223): `COPYCLIP`, `CUTCLIP`, `COPYBASE`, `PASTECLIP`, the session's own, kept across `OPEN`.
6. **Exit** (#224): the scenario, the review with the registry swept again, and what follows.

### Not in M13

- `DRAWORDER` and `WIPEOUT`, which need an ADR against ADR-0021's fills-beneath-strokes order; `DIMORDINATE`, `QDIM`, `BOUNDARY`; layer states, freezing in one viewport.
- `MLEADER`, tables, fields; SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch, plot style tables, searchable PDF text; the system clipboard.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream; the PDF of a layout in a desktop viewer.

## M12, as broken down after M11

Kept for the record; the [review](reviews/m12-exit-review.md) says how it went.

The loose ends shipped in M11. The [M11 review](reviews/m11-exit-review.md) sweeps the registry against the commands a production drawing uses daily, not against what earlier reviews accepted, and finds five gaps no review had listed. M12 takes them up; the epic is #211.

### Order of work

1. **Dimension chains** (#212): `DIMBASELINE` and `DIMCONTINUE` for linear, aligned, and angular dimensions; the style's baseline spacing, DXF `DIMDLI`.
2. **Layer tools by picking, and freeze kept** (#213): `LAYISO`, `LAYUNISO`, `LAYOFF`, `LAYON`, `LAYLCK`, `LAYULK`, `LAYMCUR`, `LAYFRZ`, `LAYTHW`; a frozen layer stays frozen through a DXF round trip.
3. **Groups** (#214): a group table changed in transactions, `GROUP` and `UNGROUP`, a pick selecting the whole group, DXF `GROUP` objects.
4. **Revision clouds** (#215): `REVCLOUD` from a rectangle, a polygon, or an object, as a closed bulged polyline.
5. **Find and replace** (#216): `FIND` across every kind of text, in one transaction.
6. **Exit** (#217): the scenario, the review, and what follows.

### Not in M12

- `DRAWORDER` and `WIPEOUT`, which need an ADR against ADR-0021's fills-beneath-strokes order; `DIMORDINATE`, `QDIM`; layer states, freezing in one viewport.
- `MLEADER`, tables, fields; SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch, plot style tables, searchable PDF text.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream; the PDF of a layout in a desktop viewer.

## M11, as broken down after M10

Kept for the record; the [review](reviews/m11-exit-review.md) says how it went. The overlay's far-curve pass, found by #206's benchmark, was fixed in M11 as #210.

The daily commands shipped in M10. The [M10 review](reviews/m10-exit-review.md) finds the product one milestone short of complete: the loose ends the M8 to M10 reviews accepted for now. M11 takes them up; the epic is #203.

### Order of work

1. **PEDIT, polyline width, and DONUT** (#204): the polyline edited as one, a constant width drawn as a filled band, DXF both ways.
2. **Leaders** (#205): a leader entity laid out by the dimension style, `LEADER`, grips, DXF both ways.
3. **Construction lines by an ADR** (#206): `XLINE` and `RAY`, after an ADR on indexing and drawing an entity without bounds (ADR-0003).
4. **Selection loose ends** (#207): `QSELECT` on several properties at once; the selection methods at the idle prompt.
5. **A layout's plot settings on the page** (#208): margins, scale, area, and rotation from the file, offered by `PLOT` and kept on the row.
6. **Exit** (#209): the scenario, the review, and what follows.

### Not in M11

- `MLEADER`, tables, fields; SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch, plot style tables, searchable PDF text.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream; the PDF of a layout in a desktop viewer.

## M10, as broken down after M9

Kept for the record; the [review](reviews/m10-exit-review.md) says how it went.

Every milestone DESIGN Part IV planned has shipped. The [M9 review](reviews/m9-exit-review.md) asks whether the product is complete for its purpose and answers: a drafter can open, draft on the model and on sheets, and print, but still cannot ask the drawing a question, put a point down, array a thing, match properties, purge a file, or set a length. M10 adds those classic commands on the machinery that exists; the epic is #196.

### Order of work

1. **Inquiry** (#197): `DIST`, `ID`, `AREA`, `LIST`; no transaction, the revision unchanged.
2. **Points** (#198): a point entity with the drawing's point style, the `NODe` snap, `POINT`, `DIVIDE`, `MEASURE`, DXF `POINT`.
3. **ARRAY** (#199): rectangular and polar, previewed, one transaction, not associative.
4. **MATCHPROP, PURGE, RENAME** (#200): through the one history; the history property test gains `PURGE`.
5. **LENGTHEN, ALIGN, POLYGON** (#201): the modify matrix grows.
6. **Exit** (#202): a day's drafting as a scenario, the review, and what follows.

### Not in M10

- `XLINE` and `RAY` — an entity without bounds does not fit a view-independent committed scene (ADR-0003) without an ADR; `PEDIT`, polyline width, `DONUT`; leaders, tables, fields.
- A layout's margins, scale, and plot area on the page (ADR-0027); `QSELECT` on several properties.
- SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch, plot style tables, searchable PDF text.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream; the PDF of a layout in a desktop viewer.

## M9, as broken down after M8

Kept for the record; the [review](reviews/m9-exit-review.md) says how it went.

Layouts were kept through a round trip in M8 (ADR-0025) and model space reached paper (ADR-0027). M9 draws the layouts: the spaces a file has, viewports onto model space, drafting in either space, a layout's sheet to PDF. The [M8 review](reviews/m8-exit-review.md) says what M8 left; the epic is #190.

### Order of work

1. **Spaces** (#191): every entity in the model or a layout; a `Layout` row; paper-space entities read and written as entities of their layout.
2. **Viewports** (#192): a window on paper onto model space at a scale, the committed scene drawn again, clipped; `MVIEW`; DXF `VIEWPORT`.
3. **Layout tabs and the current space** (#193): `LAYOUT`, `MODEL`, `PSPACE`, `MSPACE`; a view per space; the tabs of the visual identity.
4. **Plotting a layout** (#194): the sheet, the viewports at their scale, paper at one to one.
5. **Exit** (#195): the scenario, the review, and what follows.

### Not in M9

- SHX fonts, xrefs, scripting, plugins, native packaging, a DWG strategy, a native project format (§39), a WebGPU adapter (§38), associative dimensions, advanced hatch.

### Needs a person

- #109 WebKit, #110 real GPUs (and #175, which waits on it), #111 the R-tree crash upstream.

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
