# M8 exit review — Compatibility Expansion

**Date:** 2026-10-13  
**Scope:** DESIGN §36, as broken down at the M7 exit ([roadmap](../roadmap.md), epic #107)  
**Outcome:** Every item of the M8 scope is implemented and tested; an automated scenario opens a file from another program with everything it holds, edits it with the commands M8 added, plots it, and saves it with its layouts; the milestone is closed. M9 is paper space, broken down below.

## The scenario

`web/e2e/m8-scenario.spec.ts`, in Chromium and Firefox: the layouts fixture (`test-data/dxf/valid/layouts.dxf`: two layouts, a viewport, a title line in paper space) with an ellipse, a spline, and a block with an attribute added to it, as another program would write them.

1. **Opened whole:** "Imported 1 line, 1 ellipse, 1 spline, 1 block reference.", "Kept 2 layouts, not drawn: Layout1, Layout2.", nothing reported as not imported, no LOSSY, the status bar's layouts chip.
2. **The attribute edited** in place (`ATTEDIT`: "Room <101>:" → 202), a click through the fitted view's mapping.
3. **An alias of the drafter's own** (`ALIASEDIT`: `QQ, QSELECT`), then **quick select** by kind, the Properties panel naming the method, and the ellipse **moved** by a typed displacement.
4. **The view by name** (`ZOOM Extents`) and a **fence** through the spline and the line (`SELECT`).
5. **Plotted** to a PDF, A3 to fit, downloaded under the drawing's name.
6. **Saved** with the moved ellipse, the spline, the edited attribute, and the layouts with their viewport and paper-space block.

## Done when (epic #107)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `ZOOM` and `PAN` with their classic options; the view is session state and `Previous` its own stack | #180 | `tests/zoom.rs`; `web/e2e/zoom.spec.ts`; `CommandContext.view` and `previous_views`, `Dirty::VIEW` |
| Ellipses and splines as entities with primitives, grips, commands, DXF both ways; a hatch bounded by a spline fills | #181, #182 | `cad_geometry::{Ellipse, Spline}` property tests, `tests/ellipse.rs`, `tests/spline.rs`, fixtures `ellipses.dxf`, `splines.dxf`, the corpus's `hatches` case with a spline-bounded hatch |
| Block attributes defined, prompted for on insert, edited, written and read | #183 | `tests/attributes.rs`; `web/e2e/attributes.spec.ts`; fixture `ezdxf-r2010-block-attributes.dxf`; the corpus's `attributes` case |
| A file with layouts and paper space saves with them and is not LOSSY for them | #184 | `layouts_are_kept_through_a_round_trip`, `web/e2e/layouts.spec.ts`; [ADR-0025](../adr/0025-kept-layouts.md) |
| Fence, polygon, previous, last, all, and quick select pick objects | #185 | `tests/advanced_selection.rs` (every method over shown entities with locked layers; a fence through a polyline's arc), `cad_geometry::SelectionShape` tests, `web/e2e/advanced-selection.spec.ts` |
| A drafter's aliases and settings persist in the browser | #186 | `tests/aliases.rs`, `cad-app.test.ts` "settings", `web/e2e/settings.spec.ts`; [ADR-0026](../adr/0026-user-aliases-and-settings.md) |
| Model space exports to a vector PDF at a scale, with an ADR on the writer | #187 | `cad-plot` tests (the page, the scale per unit, lineweights at their width, every entity kind, the xref table, the property that every primitive lands inside the page), `web/e2e/plot.spec.ts`; [ADR-0027](../adr/0027-pdf-plot.md), with the sample opened by poppler and Ghostscript |
| A drawing spanning ten million units placed to a pixel at a hundred pixels per unit | #188 | `chunks_out_of_reach_of_the_previous_origin_take_their_own_and_make_a_region`, `regions_tile_the_buffers_and_an_ordinary_drawing_is_one`, `web/e2e/far-from-origin.spec.ts`; [ADR-0028](../adr/0028-chunk-origins.md) with the re-measured figures |

## What was built

- **ZOOM and PAN** (#180): `Extents`, `All`, `Window`, `Previous` (a stack of ten), a scale (`2x`), `Center` with a height; `PAN` by two points; a middle double-click zooms extents. The view is session state the commands may move, reported as `Dirty::VIEW`.
- **Ellipses** (#181): centre, major axis, ratio, parameters; chords at a third of a thousandth of the major axis for everything that reads primitives; exact bounds; `closest_point` by chord then Newton; break and trim by parameter; `ELLIPSE`; DXF `ELLIPSE`.
- **Splines** (#182): a NURBS by control points, knots, and weights, or through fit points (chord-length parameters, averaged knots, a banded solve); adaptive chords flat to a quarter of a thousandth of the extent; `SPLINE`; DXF `SPLINE` by raw groups; a hatch bounded by a spline edge.
- **Block attributes** (#183): `AttributeDefinitionEntity` in a definition, `Attribute` values on a reference; `ATTDEF`, the prompts on `INSERT`, `ATTEDIT`; `flatten_into` turns definitions into text with the reference's values; DXF `ATTDEF`, `ATTRIB`, `SEQEND`, `$ATTMODE`.
- **Layouts kept** (#184, ADR-0025): `KeptLayout` — the LAYOUT object, the paper-space block, its entities — as the file's groups, handles remapped on the way out, the first kept layout taking the default handles so our own files stay byte-stable; named in the report and the status bar, not drawn.
- **Advanced selection** (#185): `SelectionShape` in geometry judged on an entity's edges and what it encloses; the session owns the selection keywords at every selection prompt, draws a fence or polygon point by point, keeps the previous set, applies Remove mode to clicks, boxes, and methods; `SELECT` and `QSELECT`; the selection carries its method for the panel.
- **User aliases and settings** (#186, ADR-0026): user aliases under the built-in table, edited as text in the paragraph editor; a `localStorage` record of the toggles, the unit, the aliases, the autosave delay, and the theme, restored in one call; a settings panel; a light theme the renderer follows.
- **PDF plot** (#187, ADR-0027): the frame's entity walk as `walk_entity` over a `GeometrySink`, the frame and the page two sinks of it; `cad-plot` writes PDF 1.4 as ASCII — looks resolved as the scene resolves them, arcs as quarter-turn Béziers, a solid hatch filled even-odd; `PLOT` with its prompts kept as the next defaults; the host downloads it.
- **Chunk origins** (#188, ADR-0028): a chunk is packed against the previous chunk's origin while that is within reach, read off the offsets just packed, and against its own centre otherwise; regions of chunks drawn under their own transforms; an ordinary drawing is one region.

## Decisions made in M8

- [ADR-0025](../adr/0025-kept-layouts.md): layouts kept opaque, their handles remapped, and what drawing one would need.
- [ADR-0026](../adr/0026-user-aliases-and-settings.md): user aliases under the built-in table; settings in `localStorage`, restored in one call; the theme is UI state.
- [ADR-0027](../adr/0027-pdf-plot.md): the PDF plot is the renderer's walk written as a page; text as strokes, no fonts; `cad-plot` a crate of its own (ADR-0001 amended).
- [ADR-0028](../adr/0028-chunk-origins.md): chunk origins within reach; regions; ADR-0003's single-origin limit superseded for the spanning case.
- JOIN's keyword is `CLose`, so `C` is the crossing polygon of every selection prompt.
- The shell refreshes its UI snapshot synchronously after typed input, so the next keystroke knows the prompt (whether Space submits) before a frame arrives.

## What testing found

- The ellipse's exact bounds took the wrong sign at an extreme parameter; the spline's adaptive flattening checked only the midpoint and missed a bend between samples — both found by property tests before they shipped.
- A first version of chunk origins read every entity's bounds before packing and cost a fifth more on a repack and a rebuild; measured, replaced by a reach check on the packed offsets, which costs nothing measurable.
- The end-to-end suite showed a race the shell always had: the UI snapshot that says whether Space submits was refreshed on the next frame, and a keystroke typed before it could split a prompt's words. Fixed in the app, not the test.
- The Firefox pixel check for the spanning drawing needed a band two rows wide: the 1.25-pixel line sits a half-pixel differently in the two rasterisers.
- Nothing in the DXF corpus regressed; `every_valid_fixture_round_trips_stably` held through kept layouts, ellipses, splines, and attributes.

## Measurements

- The suite at the exit: 1,487 Rust tests, 140 web unit tests, 420 end-to-end runs across Chromium and Firefox, plus the M8 scenario in both.
- The PDF of one of everything is 8.9 KB; poppler and Ghostscript open it (ADR-0027).
- Chunk origins: native repack 0.30 / 0.29 / 0.33 ms and whole rebuild 7.0 / 37 / 75 ms at 10k / 50k / 100k, the same as ADR-0022; Firefox frames the same within noise on the commit before and after ([ADR-0028](../adr/0028-chunk-origins.md), both columns; [performance.md](../performance.md)).

## What M8 taught us

- **One walk, many consumers.** Turning `push_entity` into `walk_entity` over a sink paid for itself the same day: the plot got every entity kind for free, and a new kind will reach the screen and the page from one place.
- **Keep what you cannot draw.** Opaque records with remapped handles cost a few hundred lines and made a whole class of files safe to open and save; drawing layouts is a milestone, losing them was a bug.
- **The session owns selection.** Putting the selection keywords in the session, not in each command, meant every selection prompt gained fence, polygons, previous, last, all, and remove mode at once, and `SELECT` was fifty lines.
- **Measure the cheap thing too.** The bounds pass for chunk origins looked free and cost a fifth; the reach check on the packed floats cost nothing. Two benches, same hour, same machine, decided it.

## Open items

Needs a person (carried to M9 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF plot in a desktop viewer (ADR-0027 records poppler and Ghostscript); an interactive CAD program reading our files with ellipses, splines, attributes, and kept layouts.

Known limitations, accepted for now:

- Layouts are kept, not drawn (M9).
- A chunk whose own geometry spans more than 100,000 units is placed to its centre's resolution at its ends (ADR-0028, "Revisit when").
- The PDF carries no fonts: text is strokes, not searchable.
- `QSELECT` tests one property at a time; `SELECT` is the only way to use the selection methods at the idle prompt.

## M9, broken down

ADR-0025 said what drawing a layout needs; ADR-0027 said what plotting one needs; the visual identity shows the tabs. M9 makes them real, in order, each blocked by the one before (epic #190):

1. **Spaces** (#191): every entity in the model or a layout; a `Layout` row with its plot settings; paper-space entities read as entities of their layout and written back; the kept-opaque path retired for what the drawing now holds.
2. **Viewports** (#192): a window on paper onto model space at a scale — the committed scene drawn again under another transform, clipped; `MVIEW`; DXF `VIEWPORT`.
3. **Layout tabs and the current space** (#193): `LAYOUT`, `MODEL`, `PSPACE`, `MSPACE`; a view per space; the tabs.
4. **Plotting a layout** (#194): the sheet from the layout's settings, viewports at their scale, paper at one to one.
5. **Exit** (#195).

Not in M9: SHX fonts, xrefs, scripting, plugins, native packaging, DWG, a native project format, a WebGPU adapter, associative dimensions, advanced hatch — ideas on the epic.
