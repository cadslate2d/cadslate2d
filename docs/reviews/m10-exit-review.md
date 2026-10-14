# M10 exit review — Daily Drafting

**Date:** 2026-10-14  
**Scope:** the classic commands a day's drafting reaches for, as broken down at the M9 exit ([roadmap](../roadmap.md), epic #196)  
**Outcome:** Every item of the M10 scope is implemented and tested; an automated scenario drafts a day's work with the new commands — a polygon divided into points, a block arrayed round it, properties matched, a length asked and set, the file purged, saved, and reopened with it all; the milestone is closed. M11 takes up the loose ends the M8 to M10 reviews accepted for now, broken down below.

## The scenario

`web/e2e/m10-scenario.spec.ts`, in Chromium and Firefox, from an empty drawing:

1. **A hexagon** (`POLYGON`, inscribed, radius 50) with the point style set to an X (`PTYPE`) and **twelve points along it** (`DIVIDE`).
2. **A chair block arrayed round it** (`BLOCK`, then `ARRAY Polar` about the centre, six items over the whole turn): five copies made.
3. **A styled line** on a red layer, its properties **matched onto the hexagon** (`MATCHPROP`).
4. **A length asked** (`DIST`) and **set** (`LENGTHEN Total` to 150, at the end nearer the pick), then the line **listed** (`LIST`).
5. **A spare layer made and the walls layer renamed** (`RENAME`), the file **purged** of the spare (`PURGE`: "Purged 1 layer (SPARE).") and **saved**: twelve `POINT`s, six `INSERT`s, the hexagon on `Partitions`, no `SPARE` row, the line's new end, `$PDMODE` 3.
6. **Reopened:** the styled line (the chair's went into the block), a polyline, six block references, twelve points; nothing left to purge.

## Done when (epic #196)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `DIST`, `ID`, `AREA`, and `LIST` answer in the log without touching the document | #197 | `tests/inquiry.rs` (each command, `AREA Object` per kind, the revision unchanged, `U` after them undoing the edit before, the golden `LIST` of one of every kind), `a_polygons_area_by_points_is_its_polylines`; `Path::area` exact through arcs (`cad-geometry/src/path.rs`); `inquiry.spec.ts` |
| A point is an entity with a drawing-wide style, snapped to by `NODe`; `DIVIDE` and `MEASURE` place points along any curve; DXF both ways | #198 | `tests/points.rs` (`divide_and_measure_land_on_a_bulged_polyline_at_equal_arc_length`, the pick, window, snap, and grip), `bounds_hold_the_marker_and_the_hit_is_the_place`, the history property test's point style; `points_from_another_writer_and_the_point_style_round_trip`, `ezdxf-r2010-points.dxf`; the baselines `snap-marker-node.png` and `point-markers.png`; `points.spec.ts` |
| `ARRAY` makes rectangular and polar copies of a selection in one transaction, with a preview | #199 | `tests/array.rs` (every kind under both forms, the preview equal to the commit, one undo and redo, the refusals), `a_polar_array_is_n_items_at_one_distance_and_equal_angles`; `array.spec.ts` |
| `MATCHPROP` copies properties, `PURGE` removes unused rows, `RENAME` renames any row, through the one history | #200 | `tests/housekeeping.rs`; `purge_removes_what_nothing_uses_and_keeps_the_permanent_the_current_and_the_used`, `rows_of_every_table_are_renamed_with_the_refusals_and_references_hold`, the history property test's `Purge`; `a_purged_file_is_written_without_the_rows_that_went`; `housekeeping.spec.ts` |
| `LENGTHEN`, `ALIGN`, and `POLYGON` with their classic options, in the modify matrix | #201 | `tests/lengthen.rs`, `tests/align.rs`, `tests/polygon.rs`; `lengthens.txt` and its runner; `lengthening_there_and_back_is_the_identity`, `aligning_lands_the_points_as_asked`, `a_regular_polygons_corners_are_on_its_circle_with_equal_sides`; `docs/modify-matrix.md` (LENGTHEN, ALIGN); `align.spec.ts` |
| A scenario drafts a day's work with them | #202 | `m10-scenario.spec.ts` |

## What was built

- **Inquiry** (#197): `DIST`, `ID`, `AREA` (by corners, previewed as a polygon, or `Object` — a closed polyline with its arcs counted exactly, a circle, a whole ellipse, a hatch less its holes), `LIST` through the panel's own `geometry_of`, so one description serves both. `Path::area` is exact through arcs: the polygon of the vertices plus a circular segment per bulge, signed by its direction. Lengths carry the drawing's unit, an area its square.
- **Points** (#198): `PointEntity` and `PointStyle` — `$PDMODE`'s shapes and rings, a size in drawing units — as a drawing setting whose change marks everything shown; the marker walks as segments and arcs, a dot a segment of no length that round caps make a dot; `Tables` carries the style beside the tables; `Path::point_along` through the arcs by arc length the way they run; `POINT`, `DIVIDE`, `MEASURE` from the end nearer the pick, `PTYPE`; `SnapMode::Node`; DXF `POINT`, `$PDMODE`, `$PDSIZE`.
- **ARRAY** (#199): `Rectangular` and `Polar` as values that yield their transforms, the same list for the preview and the commit; `TransientEdit::commit_copies_under` for many transforms in one transaction; a non-rotated polar array moves each item by the selection's centre.
- **Housekeeping** (#200): `replace_with`, the `rename_*` builders for every table, `purge(PurgeScope)` sweeping blocks first and again while removing one frees what only it placed, then styles, layers, linetypes; the removal builders now count what a block's entities use; `MATCHPROP` with `Settings`, `PURGE` with a dry `?`, `RENAME` by table.
- **LENGTHEN, ALIGN, POLYGON** (#201): `lengthen_segment`, `lengthen_arc`, `Path::lengthened`; `Lengthening` as delta, percent, total, angle, or total angle; `DYnamic` sliding the end along the object's own line or circle with object snap off; `Transform::similarity` and `Transform::aligning` with the fixed point solved in closed form; `regular_polygon` and `polygon_from_edge`.

## Decisions made in M10

- A point's size is always in drawing units: a file's screen-relative `$PDSIZE` (zero or negative) is not held, because the committed scene is view-independent (ADR-0003) and a marker that followed the view would be rebuilt on every zoom. The import report says so when the file has points; the writer omits `$PDSIZE` then, so the round trip is stable (noted on #198).
- A block removed earlier in the same transaction no longer places anything, so `PURGE` sweeps a block and then what only it placed as one change. The test that encoded the opposite ("Pair still holds it until commit") was turned round with the reason.
- `Transform` gained a fifth kind, a similarity (a turn and a scaling about one point), rather than composition: ALIGN's two pairs are one transform with a fixed point, and every entity keeps its one `transformed`.
- `LIST` describes an entity through the Properties panel's `geometry_of`: one description, not two.
- The current space and point style stay session and drawing state respectively; no new table was needed for anything in M10.

## What testing found

- The arc side of a bulge: a positive bulge bows to the *right* of its chord's direction (counter-clockwise about a centre on the left). Two expectations written the other way round were corrected by the tests, not the code.
- A corpus value typed to six places lands a hair short of a full circle rather than on it: the cases that test the limit now step clearly past it, and a bulge is written to eight places where six left the arc a millionth off.
- `BLOCK` leaves a reference in the selection's place, so an e2e that then `INSERT`ed the same block at the same point found two; the test was wrong, not the command.
- The import report line about a screen-relative point size fired on every ezdxf file, which all write `$PDSIZE 0`; it now speaks only when the file has points to show at that size.
- The selection summary's kind list, the OSNAP keyword table, the snap-mode ranking, and a registry test that used `LENGTHEN` as an imaginary name all moved when the point and the Node snap arrived — each caught by its test.
- Nothing in the DXF corpus regressed; the four `unsupported` cases now import their `POINT` and say so in their expectations and in the generator.

## Measurements

- The suite at the exit: 1,573 Rust tests, 142 web unit tests, 442 end-to-end runs across Chromium and Firefox, the M10 scenario in both.
- The geometry corpus gained its seventh file, `lengthens.txt` (17 cases, polylines included).
- Not measured: nothing in M10 touched the frame or the scene's cost; the point style's rebuild is a whole rebuild, which ADR-0022 measured at 75 ms for 100k entities, and a style change is rare.

## What M10 taught us

- **Reuse the description.** `LIST` cost a page because the panel already described every kind; the one place to extend is `geometry_of`.
- **The builders hold the rules.** `PURGE` is a loop over `remove_*` calls and `RENAME` a dispatch to `rename_*`: the refusals, the permanents, and the in-use checks stayed in the document, and the commands only turn errors into words. Making the removals see block contents fixed a latent hole for every caller at once.
- **Values that yield transforms.** `Rectangular::transforms` and `Polar::transforms` made "the preview equals the commit" a one-line test.
- **Write the corpus case, then read what it says.** Three of seventeen lengthen cases were wrong as first written — the side of an arc, the hair at the limit — and the corpus's own invariants caught them before any command did.

## Open items

Needs a person (carried to M11 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF of a layout in a desktop viewer; an interactive CAD program reading our files with their points and purged tables.

Known limitations, accepted for now and taken up in M11:

- A polyline has no width; there is no `PEDIT`, no `DONUT`, no leader.
- `XLINE` and `RAY` are not entities: an entity without bounds needs an ADR against ADR-0003.
- `QSELECT` tests one property at a time; the selection methods need `SELECT` at the idle prompt.
- A layout's margins, scale, and plot area from its file shape `MVIEW Fit` only, not the page.
- A point's size is in drawing units only (above).

## Is the product complete for its purpose?

With M10 the drafter has the classic daily set: drawing, snapping, modifying, organizing, annotating, hatching, blocks, paper space, plotting, and now inquiry, points, arrays, matched properties, purge and rename, lengthen and align. What the reviews since M8 have listed as "accepted for now" is the remainder: the polyline's own editing and width, the leader annotation lacks, construction lines, two selection conveniences, and a layout's plot settings on the page. None is a subsystem; each is a day-one expectation of classic drafting, and together they fit one milestone. After them, what is left — SHX fonts, xrefs, scripting, DWG, a native format, WebGPU — is either a product of its own or waits on a person with hardware. So: not yet complete, by one milestone of loose ends.

## M11, broken down

Epic #203, in order, each blocked by the one before:

1. **PEDIT, polyline width, and DONUT** (#204): the polyline edited as one, a constant width drawn as a band, DXF both ways.
2. **Leaders** (#205): an entity laid out by the dimension style, `LEADER`, DXF both ways.
3. **Construction lines by an ADR** (#206): `XLINE` and `RAY`, after an ADR that says how an entity without bounds is indexed and drawn.
4. **Selection loose ends** (#207): `QSELECT` on several properties; the methods at the idle prompt.
5. **A layout's plot settings on the page** (#208): margins, scale, area, rotation from the file, kept on the row.
6. **Exit** (#209).

Not in M11: `MLEADER`, tables, fields, and the long-standing ideas — SHX fonts, xrefs, scripting, plugins, packaging, DWG, a native project format, WebGPU, associative dimensions, advanced hatch, plot style tables, searchable PDF text — on the epic.
