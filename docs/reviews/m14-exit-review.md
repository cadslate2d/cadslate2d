# M14 exit review — Production Annotation

**Date:** 2026-10-16  
**Scope:** what the M13 sweep found production sheets still reach for ([roadmap](../roadmap.md), epic #225)  
**Outcome:** Every item of the M14 scope is implemented and tested.

An automated scenario covers all of it:
- a plate dimensioned by ordinates with `QDIM`;
- a note on a wipeout over a hatch, brought to the front;
- a boundary offset;
- layer states for two sheets, the drawing saved and reopened, and a state restored in it.

The milestone is closed. The registry swept again against the classic daily set finds nothing a production sheet needs that is not there. **The product is complete for its purpose**, as DESIGN §47 defines a successful v1. What remains is either verification that needs a person (M15, below) or the product's long-term ideas.

## The scenario

`web/e2e/m14-scenario.spec.ts`, in Chromium and Firefox, from an empty drawing:

1. **A plate with two holes, on layer `PLAN`, dimensioned by ordinates.**
   - `QDIM ALL`, `Ordinate`, `Datum point` 0,0, placed below: four X ordinates.
   - Placed beside: three Y ordinates.
2. **A note on layer `NOTES`.**
   - A rectangle filled solid (`HATCH`) and a line across it.
   - A `WIPEOUT` over the middle, and a `TEXT` on it, brought forward with `TEXTTOFRONT`.
   - On screen, inside the wipeout neither the hatch nor the line shows; outside it the hatch does.
3. **A boundary.** `BOUNDARY` with islands off, round the plate's outline, then `OFFSET` 5 outwards, which rounds its corners.
4. **Layer states for two sheets.** `Sheet A` with everything on; `Plate only` with `NOTES` off. `Sheet A` restored switches one layer.
5. **Saved.**
   - Seven `DIMENSION`s, four with the X bit (64).
   - One `WIPEOUT`.
   - Two layer-state `XRECORD`s.
   - The offset outline, closed, with four bulged corners.
6. **Reopened.** The report says "2 layer states". `Plate only` restored switches `NOTES` off, and the hatch disappears from the screen.

## Done when (epic #225)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An ADR decides how an object is drawn above or beneath another, fills staying triangles and strokes batched by style; `DRAWORDER` and `TEXTTOFRONT` change the order; `WIPEOUT` masks; DXF `SORTENTSTABLE` and `WIPEOUT` both ways | #226 | [ADR-0032](../adr/0032-draw-order.md) with its 100k measurements; `tiers_paint_any_sequence_as_it_was`, `front_and_back_go_past_everything_else_keeping_their_own_order`, `above_and_under_go_next_to_the_reference_and_make_room` (`draw_order.rs`); `tests/draworder.rs`; `the_drawing_order_crosses_in_the_style_words_and_repacks_only_what_moved` (render boundary); `the_page_is_painted_in_the_drawing_order_and_a_wipeout_masks_in_white` (`cad-plot`); `the_drawing_order_and_a_wipeout_from_another_writer_round_trip`, `ezdxf-r2010-draw-order.dxf`; `draw-order.spec.ts` with the baselines `text-over-hatch.png` and `wipeout-under-note.png`. `SORTENTSTABLE` is read; the order is written as the order of the records (see Decisions) |
| `DIMORDINATE` measures X or Y from the origin; `QDIM` dimensions a selection continuous, baseline, or ordinate | #227 | `an_ordinate_measures_one_coordinate_from_its_datum_with_a_leader_to_its_text`; the kind in the dimension property tests; `an_ordinate_leader_is_connected_from_its_offset_to_its_end` (`cad-geometry`); `tests/dimordinate.rs`; `ordinate_dimensions_from_another_writer_round_trip`, `ezdxf-r2010-ordinate-dimensions.dxf`; the corpus's `dimensions` cases, whose ordinate is read now |
| `LAYERSTATE` saves, restores, lists, and deletes layers' switches, colours, linetypes, and lineweights by name, in the drawing and its file | #228 | The history property test's save, restore, and delete; `a_layer_state_restores_the_layers_it_names_as_one_undo`, `a_restore_keeps_the_current_layer_shown_and_a_linetype_gone_leaves_its_own`, `layer_state_names_are_checked_and_unknown_states_refused`; `tests/layerstate.rs`; `layer_states_from_another_writer_restore_and_round_trip`, `ezdxf-r2010-layer-states.dxf` |
| `BOUNDARY` draws a closed polyline round the region a point is in, from HATCH's loops | #229 | `tests/boundary.rs`: lines and an arc with an island, whose polylines are the loops HATCH fills; islands off; a circle; a point outside refused |
| A scenario drafts with them | #230 | `m14-scenario.spec.ts` |

## What was built

- **The drawing order** (#226, ADR-0032).
  - **A tier on the record.** Every record carries an `i16` tier. A new object takes the top tier, an object made from another that one's, a wipeout the tier above the top.
  - **Fills beneath strokes, as before.** Within a tier, fills lie beneath strokes, as ADR-0021 always drew them, so a drawing nobody reorders looks as it did.
  - **The scene.** The tier rides in the high half of each instance's style word: no new buffer and no new boundary call. The adapter draws it as depth: fills write it, strokes only test it.
  - **Commands.** `DRAWORDER` (`Above objects`, `Under objects`, `Front`, `Back`, each keeping the selection's own order) and `TEXTTOFRONT`.
  - **`WIPEOUT`.** A polygon in the background colour, a new style-row kind, with frames switched by `Frames`.
  - **Paper and file.** The plot paints in the same order, a wipeout as white. The DXF writer writes each space's records in the order they are painted; the reader takes a `SORTENTSTABLE` too.
- **Ordinate dimensions and `QDIM`** (#227).
  - `Measure::Ordinate` is laid out as a leader square to the axis, jogging across in its middle, with the text past its end.
  - `DIMORDINATE` takes the axis from the drag or from `Xdatum` and `Ydatum`, with a session datum.
  - `QDIM` dimensions a selection's ends and centres `Continuous`, `Baseline`, or `Ordinate`.
  - DXF reads and writes the ordinate kind.
- **Saved layer states** (#228).
  - A table of states, each keeping every layer's switches, colour, linetype, and lineweight by name.
  - A restore leaves layers the state does not name, and keeps the current layer shown.
  - `LAYERSTATE` saves, restores, deletes, renames, and lists.
  - DXF keeps the states in `ACAD_LAYERSTATES` in the LAYER table's extension dictionary.
- **`BOUNDARY`** (#229): HATCH's region search, now shared, drawn as closed polylines with islands or without.

## Decisions made in M14

- **The drawing order is a tier drawn as depth** ([ADR-0032](../adr/0032-draw-order.md)). The alternatives ADR-0032 rejects:
  - a pass per tier, which costs batching;
  - a scene sorted by order, which costs chunked re-packing;
  - tiers in the style table, which costs `u16` rows;
  - a depth per object by ID, which costs depth precision.
- **The file keeps the order as the order of its records.**
  - Writing a `SORTENTSTABLE` beside ID-ordered records was built and measured first. On the 100k drawing the table held most entities: 2 MB more file, and an export and an open each 36 ms slower.
  - Records in paint order say the same thing for nothing, and readers that ignore the table paint them right. The issue's "both ways" was narrowed to reading the table (noted on #226).
- **The tier is one integer attribute.**
  - Read as a second float attribute it cost 5–7 ms a frame in Firefox at 100k, and the depth test itself cost nothing.
  - Read as one unsigned integer with the style index, frames are as quick as before or slightly quicker.
- **An imported area after a stroke is over it.** A solid hatch or a wipeout after a line in a file is painted over it, as its writer meant. That drawing is now shown as AutoCAD shows it.
- **QDIM's datum is `Datum point`**, abbreviated `D`. Our keywords abbreviate by their leading capitals, so the classic `datumPoint` could only be typed in full.
- **A layer state names its layers by name.** It survives a layer removed and made again. A restore keeps the current layer shown, as `LAYER` does.

## What testing found

- **A wipeout's corners can come back an ulp off.** Normalized clip coordinates do not invert exactly. The curated fixtures use exact numbers, and the fuzz target compares at the file's own precision. The matrix says so.
- **Opening got slower, then not.** The first import of the draw order sorted with two hash lookups per comparison and cost 110 ms at 100k. Keys read once, handles indexed by ID, and the sort skipped when already in order brought it to about 15 ms of a 220 ms open.
- **A flaky scenario fixed.** The M13 scenario's rest-to-acquire failed once in Firefox under the full run: the pointer was already near the point, so it made no move for the host to time a rest from. The rest now comes from a little way off.
- **HATCH's area follows chords.** The fill's area is that of its triangles, a thousandth under the exact region. `BOUNDARY`'s tests compare loops, not areas, and the exact area is the polylines'.
- **Smaller ones:**
  - ezdxf writes an ordinate's leader end an ulp off (4.999999999999999).
  - A rectangle offset outwards has rounded corners, so the scenario looked for a vertex that is not there.
  - "A ordinate dimension" now reads "An ordinate dimension".

## Measurements

- The suite at the exit:
  - 1,714 Rust tests;
  - 146 web unit tests;
  - 482 end-to-end runs across Chromium and Firefox, the M14 scenario in both.
- 100k mixed drawing, natively ([performance](../performance.md#the-drawing-order-2026-10-15-226)):
  - One object to the front re-packs its chunk in 0.37 ms.
  - Every hatch to the back costs about a whole rebuild (81 ms).
  - The open and the scene build are unchanged within the run-to-run spread.
- 100k mixed drawing, in Firefox:
  - pan: 63.9 ms before, 60.0 ms after;
  - pointer: 62.1 ms before, 58.8 ms after;
  - zoomed in: 40.4 ms before, 36.5 ms after.

## What M14 taught us

- **Write what every reader understands.** A record order is honoured by every DXF reader; a sort table only by some, and it cost more to write.
- **Measure the first version.** The two-attribute tier and the per-comparison hash lookups both looked harmless; the 100k benchmarks found them at once.
- **A shared search pays twice.** `BOUNDARY` is HATCH's region search drawn as polylines; making it two functions left one rule for what a region is.

## Open items

Needs a person (M15, below):
1. **Real GPUs** (#110), **WebKit** (#109), and the text atlas waiting on them (#175).
2. **The R-tree crash upstream** (#111).
3. **A check against AutoCAD of the records written since M12**: groups, wipeouts, ordinates, layer states. Their layouts follow the DXF reference and what other projects document, and our tests read them back, but no program of Autodesk's has opened them. The same goes for a PDF of a layout in a desktop viewer.

Known limitations, accepted:
- Two crossing strokes of different tiers meet in instance order: one pixel at the crossing may show the lower one.
- A block definition's own `SORTENTSTABLE` is reported, and its objects are drawn in the definition's order.
- A layer state keeps no plot style or transparency, which the drawing does not hold.

## Is the product complete for its purpose?

The sweep again, against the classic daily set and against DESIGN §47.

**The daily commands are all there:**

| Area | Commands |
| --- | --- |
| Drawing | `LINE`, `PLINE`, `CIRCLE`, `ARC`, `RECTANG`, `POLYGON`, `ELLIPSE`, `SPLINE`, `XLINE`, `RAY`, `POINT`, `DONUT`, `HATCH`, `BOUNDARY`, `REVCLOUD`, `WIPEOUT` |
| Text and dimensions | `TEXT`, `MTEXT`, every dimension kind with chains, ordinates, and `QDIM`; `LEADER` |
| Modifying | `MOVE`, `COPY`, `ROTATE`, `SCALE`, `MIRROR`, `STRETCH`, `TRIM`, `EXTEND`, `OFFSET`, `FILLET`, `CHAMFER`, `BREAK`, `JOIN`, `LENGTHEN`, `ALIGN`, `ARRAY`, `EXPLODE`, `PEDIT`, `MATCHPROP`, grips |
| Organizing | layers with their tools and states, blocks with attributes, groups, draw order |
| Inquiry | `DIST`, `ID`, `AREA`, `LIST` |
| Aids | object snaps, tracking, `ORTHO`, `POLAR`, `SNAP`, `GRID` |
| Paper and output | layouts, viewports, `PLOT` to PDF |
| Files | DXF in and out, with what is not held reported |
| Reuse | `INSERT` of a file, `WBLOCK`, the clipboard |
| Housekeeping | `PURGE`, `RENAME`, `UNITS` |

**Every item of §47 is met:** no account, precise entry, grips, snaps, modify, layers and blocks, text, dimensions, and hatch, undo of everything, large drawings, DXF in and out with warnings, local recovery, and no server.

**What the sweep finds missing:**
- **Product-level ideas, kept on the epic:** `MLEADER`, tables, raster underlays, xrefs, SHX fonts, DWG, and scripting.
- **Occasional conveniences:**
  - `OVERKILL`, to remove duplicate objects;
  - named views (`VIEW`);
  - `LAYMRG` and `LAYDEL`;
  - `SPLINEDIT`, beyond the grips splines have.

None of these stops a production sheet. They join the long-term ideas, to be taken up when drafters ask for them.

**The product is complete for its purpose.** No milestone of features follows. M15 is verification that needs a person: the GPUs, WebKit, the text atlas, the upstream crash, and the check against AutoCAD. Until a person takes those up, the backlog has no work an agent can do on its own.

## M15, broken down

Epic #231, **M15 — v1 Verification (needs a person)**:
- #109, #110, #111, #175, carried from M14;
- #232, a check against AutoCAD and a desktop PDF viewer of what the program writes.
