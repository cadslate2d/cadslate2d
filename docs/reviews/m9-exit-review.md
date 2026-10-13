# M9 exit review — Paper Space

**Date:** 2026-10-13  
**Scope:** DESIGN §36's paper space, layouts, and plotting, as broken down at the M8 exit ([roadmap](../roadmap.md), epic #190)  
**Outcome:** Every item of the M9 scope is implemented and tested; an automated scenario opens the M8 scenario file with its layouts drawn, adds and scales a viewport, draws a title block on paper, plots the layout, and saves and reopens the file with it all; the milestone is closed. Every milestone DESIGN Part IV planned has now shipped. M10 is daily drafting, broken down below.

## The scenario

`web/e2e/m9-scenario.spec.ts`, in Chromium and Firefox, on the M8 scenario file (the layouts fixture with an ellipse, a spline, and a block with an attribute; now built by `support.ts` for both scenarios):

1. **Opened with its layouts as layouts:** three tabs, the file's viewport an entity of `Layout1` ("1 viewport." in the report), nothing kept opaque, no LOSSY.
2. **On `Layout1`:** the `PAPER` chip, `SELECT ALL` finds its two entities — the title line and the viewport.
3. **A viewport added** (`MVIEW` by two corners) **and scaled** to half by a click on its frame and `SCALE`.
4. **A title block drawn on the paper:** a rectangle and a line of text.
5. **The layout plotted:** `scenario-Layout1.pdf`, the A3 sheet as the page, a clip per viewport, "Plotting the sheet of layout "Layout1" on A3, to fit."
6. **Saved:** two `VIEWPORT`s in the paper block, the new one at half its width, the text flagged paper, the rectangle, the model's ellipse and spline as they were.
7. **Reopened:** three tabs, "2 viewports." in the report, `SELECT ALL` on `Layout1` finds five.

## Done when (epic #190)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| Every entity is in a space; the model and each layout have their own entities, extents, and view; a file's paper-space entities are entities of their layout; the kept-opaque path retired for what the drawing holds | #191 | `the_models_reads_see_nothing_of_a_layout_and_the_reverse`, `a_removed_layout_takes_its_entities_and_undo_brings_both_back`, `moving_an_entity_between_spaces_is_one_change`, the history property test's layout operations; `layouts_are_kept_through_a_round_trip`, `a_layout_made_here_and_a_renamed_one_are_written_and_read_back`; [ADR-0029](../adr/0029-spaces.md) |
| A viewport is an entity: a window on paper onto model space at a scale and a centre, drawn as the model's scene clipped to it, with grips and `MVIEW`; DXF both ways | #192 | `tests/viewport.rs` (two corners, `Fit`, the pick on the frame, corner and centre grips, refused in the model and in a block); the bounds and hit-test property tests; `a_viewport_round_trips_and_one_in_the_model_is_reported`; the baseline `layout-two-viewports.png`; [ADR-0030](../adr/0030-viewports.md) |
| Layout tabs and `LAYOUT`, `MODEL`, `PSPACE`, `MSPACE`: the current space is session state, commands act in it, a viewport can be worked through | #193 | `tests/layouts.rs` (new, go, rename, delete with undo, a view per space, a line drawn on a layout is its, `MSPACE` at the viewport's scale and `PSPACE`'s write-back, the pick among several); `a_layouts_scene_holds_the_model_and_the_paper_in_their_own_regions`, `a_layouts_scene_is_the_model_through_its_viewports_and_the_paper_over_it`; `paper-space.spec.ts` |
| `PLOT` of a layout: the sheet from the layout's plot settings, viewports at their scale, paper entities at one to one | #194 | `a_layout_plots_its_paper_at_one_to_one_and_the_model_through_each_viewport_at_its_scale`, `a_layouts_page_is_its_sheet_and_a_window_or_a_ratio_scales_the_paper`, `every_primitive_of_a_layout_lands_on_the_sheet_and_the_model_through_each_viewport`; `on_a_layout_the_paper_is_the_sheet_and_a_paper_named_becomes_it`; `a_layouts_sheet_set_here_is_written_as_its_paper_size_and_read_back`; `paper-space.spec.ts` on the fixture's `Layout1`; [ADR-0027](../adr/0027-pdf-plot.md) §5 |
| The M8 scenario file, opened, has its layouts drawn and plots them | #195 | `m9-scenario.spec.ts` |

## What was built

- **Spaces** (#191, ADR-0029): `Layout` is a table whose first row is the model; every entity record carries its space; `Drawing::shown`, `query_shown`, and `extents` are the model's and the `_in` forms take a space, over one spatial index filtered by space; `add_layout`, `rename_layout`, `remove_layout` (taking the layout's entities, undone together), `set_space`; `TransactionBuilder::in_space`. DXF: a paper block's entities are entities of their layout, flagged `67 1` on the way out; the kept records narrowed to what the drawing has no entity for.
- **Viewports** (#192, ADR-0030): `ViewportEntity` — centre, width, height, the model point at its centre, the scale, on or off — with its frame, corners, edges, `model_point` and `paper_point`, grips, `transformed`; `viewport_placeable` refuses one in the model or a block; `MVIEW` by two corners or `Fit` to the sheet's printable area; DXF `VIEWPORT` both ways, the first viewport the paper's own.
- **Layout tabs and the current space** (#193): the session's space with a view per space and a `Through` for the model worked through a viewport; `SpaceRequest` applied after a command's step; `LAYOUT` (`?`, `New`, `Rename`, `Delete`, `Set`), `MODEL`, `PSPACE`, `MSPACE`; `ZOOM Extents` on a layout is the sheet; the committed scene of a layout is the model's chunks then the layout's, each region flagged, the model drawn through each viewport by the adapter under a scissor (`scene_viewports`); the tabs and the `MODEL`/`PAPER` chip.
- **Plotting a layout** (#194, ADR-0027 extended): the page is the sheet, no margin of its own; the paper at one to one, the model through each viewport that is on at the viewport's scale inside a nested clip, the frame not plotted; a paper named at the prompt becomes the layout's sheet, kept on the row as one undoable change and written into the `LAYOUT` object's paper size; the file named after the drawing and the layout.

## Decisions made in M9

- [ADR-0029](../adr/0029-spaces.md): every entity is in a space; the model is row 0 of the layout table; one spatial index, filtered.
- [ADR-0030](../adr/0030-viewports.md): a viewport is an entity of the paper; its contents are the model's committed scene drawn again under its own transform — never a second scene per viewport.
- [ADR-0027](../adr/0027-pdf-plot.md) §5: a layout's page is its sheet; the viewport is a window, not a thing on the paper.
- The current space is session state, like the view (AGENTS.md): `U` undoes drawing changes only, and changing tabs changes nothing in the history.
- `PLOT` plots the space the commands act in: worked through a viewport, that is the model.

## What testing found

- The repack after a paper edit re-packed the model's chunk and the paper's both; the dirty-range test for a layout caught it, and the chunk is now chosen by the entity's space.
- A wrapped line in the WASM facade escaped a plain-text patch, so the browser was served the model's scene on a layout: the first recording of the two-viewport baseline showed one viewport and no paper line. The engine test on region counts failed first; the baseline was re-recorded after the fix and looked at.
- A grip and a `MOVE` on a viewport came back a few ulps off the typed coordinates; the tests compare with a tolerance, as the rest do.
- A layout's row carrying a paper size made `Layout` no longer `Eq`; nothing needed it.
- Nothing in the DXF corpus regressed; `every_valid_fixture_round_trips_stably` and the golden file held through spaces, viewports, and the sheet written into the `LAYOUT` object.

## Measurements

- The suite at the exit: 1,522 Rust tests, 142 web unit tests, 429 end-to-end runs across Chromium and Firefox, the M9 scenario in both.
- The frame on a layout makes six steady engine calls, as on the model (`render-boundary.test.ts`); pointer movement on a layout does not touch the committed scene.
- Not measured: a layout's queries beside a large model share one spatial index filtered by space (ADR-0029, "Revisit when"); if a layout feels slow on a 100k model, an index per space is the next step, measured first.

## What M9 taught us

- **Make the model a row.** Treating the model as row 0 of the layout table meant every space question had one answer and no special case: `shown_in(MODEL)` is `shown`, and `remove_layout` refusing the model is one line.
- **Draw the scene twice, build it once.** The viewport as the committed scene under another transform cost the adapter a scissor and a loop; a scene per viewport would have cost a cache per viewport and a second lifetime. The region flag was the whole of the render change.
- **Space is session state.** Putting the current space beside the view — never in the history — kept `U` meaning what it meant and made the tabs a request, like a host request, applied after the command's step.
- **Patch by regex when rustfmt has been through.** Two plain-text patches missed wrapped lines this milestone and one truncated a file; a helper that prints `MISSING` instead of asserting, and re-reading before patching, cost less than the hour each miss took.

## Open items

Needs a person (carried to M10 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF of a layout in a desktop viewer; an interactive CAD program reading our files with their paper blocks and viewports.

Known limitations, accepted for now:

- A layout's margins, scale, and plot area are read from the file for `MVIEW Fit` and the sheet only; the page takes the paper size alone (ADR-0027, "Revisit when").
- A viewport has no twist and no clipping boundary (ADR-0030, "Revisit when"); a file's is read as untwisted.
- Viewport frames are drawn on screen and never plotted; there is no per-layer "plot" flag.
- `QSELECT` tests one property at a time; `SELECT` is the only way to use the selection methods at the idle prompt (carried from M8).

## Is the product complete for its purpose?

Every milestone of DESIGN Part IV has shipped: the line, input and entities, snaps and grips, robust modify, organization, DXF, annotation and hatch, performance, compatibility, and now paper space. A drafter can open a file from another program, draft on the model and on sheets, and print. What the same drafter still cannot do is ask the drawing a question (`DIST`, `AREA`, `LIST`), put a point down, array a thing, match properties, purge a file, or set a length — the classic commands the §5 menus assume and the plan never listed. They are not a subsystem; they are commands on the machinery that exists. That is M10.

## M10, broken down

Epic #196, in order, each blocked by the one before:

1. **Inquiry** (#197): `DIST`, `ID`, `AREA`, `LIST`, committing nothing.
2. **Points** (#198): `PointEntity` with the drawing's point style, `NODe`, `POINT`, `DIVIDE`, `MEASURE`, DXF `POINT`.
3. **ARRAY** (#199): rectangular and polar copies, previewed, one transaction.
4. **MATCHPROP, PURGE, RENAME** (#200): properties copied, unused rows removed, any row renamed, through the one history.
5. **LENGTHEN, ALIGN, POLYGON** (#201): in the modify matrix.
6. **Exit** (#202).

Not in M10: `XLINE` and `RAY` (an entity without bounds needs an ADR against ADR-0003), `PEDIT` and polyline width, leaders, tables, fields, and the long-standing ideas — SHX fonts, xrefs, scripting, plugins, packaging, DWG, a native project format, WebGPU, associative dimensions, advanced hatch, plot style tables, searchable PDF text — on the epic.
