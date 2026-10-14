# M13 exit review — Drafting Aids and Reuse

**Date:** 2026-10-14  
**Scope:** the aids the design's status bar names and the reuse across drawings the M12 sweep found missing ([roadmap](../roadmap.md), epic #218)  
**Outcome:** Every item of the M13 scope is implemented and tested.

An automated scenario covers all of it:
- a plan laid out with grid snap and object snap tracking;
- a label in a text style of its own;
- a title block written as a file and inserted into a second drawing;
- the label carried across on the clipboard.

Both drawings are saved, and the second is reopened. The scenario also found one defect, fixed here.

The milestone is closed. The registry swept again finds four gaps for production sheets, which M14 takes up.

## The scenario

`web/e2e/m13-scenario.spec.ts`, in Chromium and Firefox:

1. **Grid snap** (F9, `SNAP 10`): two walls drawn with the pointer, its clicks a little off the grid, landing on (0, 0), (100, 0), and (100, 60).
2. **Object snap tracking** (F11): the pointer rests on one wall's end and on the other's. Brought near where the lines through them cross, it reads "Tracking: intersection", and a click there starts the third wall at (0, 60).
3. **A text style of its own** (`STYLE`): `Labels`, 5 high, 0.8 wide, leaning 10°. `TEXT` does not ask for the height and draws "Kitchen" in it.
4. **A title block written as a file** (`WBLOCK`): a rectangle and its text, selected by a crossing polygon, written about their corner as `title.dxf`.
5. **The label copied** (Ctrl+C), and the plan saved: three walls, exactly on the grid.
6. **A second drawing** (`OPEN`): the title block inserted (`INSERT ~`), and the label pasted (Ctrl+V). Saved: one `INSERT` of `title`, and "Kitchen" in `Labels`, 0.8 wide, 5 high, with the style's row.
7. **Reopened**: the block reference is there, and `Labels` can be set current.

## Done when (epic #218)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `SNAP` (F9) snaps the free cursor to a grid after object snap, before ORTHO and POLAR | #219 | `grid_snap_takes_the_nearest_grid_point_on_each_axis` (`aids.rs`); `tests/grid_snap.rs` (the command, F9, the pointer at the idle prompt and in LINE, object snap first, ORTHO from the snapped point, typed points untouched, `GRID Snap`); key routing for F9; `grid-snap.spec.ts`, `m13-scenario.spec.ts` |
| `OTRACK` (F11) tracks from rested-on snap points: paths, their crossings, a distance along one | #220 | `object_tracking_follows_a_path_and_takes_a_crossing` (`aids.rs`); `tests/tracking.rs`; `object_tracking_marks_its_points_and_draws_its_paths_without_touching_the_scene` (`cad-render`); "a pointer at rest acquires a snap point for tracking" (`cad-app.test.ts`); `shell.spec.ts`, `m13-scenario.spec.ts` |
| `STYLE` makes and changes text styles; `TEXT` and `MTEXT` take the current one | #221 | `a_style_refuses_values_no_text_can_be_drawn_by`; the text property tests with any width factor and slant; the history property test's style values and current style; `tests/style.rs`; `text_styles_and_their_texts_from_another_writer_round_trip`, `ezdxf-r2010-text-styles.dxf`; `text-styles.spec.ts` with the baseline `text-style-slanted.png` |
| `INSERT` places a DXF file as a block; `WBLOCK` writes a block or a selection as a file | #222 | `rows_are_matched_by_name_and_what_is_missing_comes_along`, `a_layer_the_target_lacks_comes_with_its_linetype` (`drawing/merge.rs`); `tests/wblock.rs`; `wblock.spec.ts`, `m13-scenario.spec.ts` |
| `COPYCLIP`, `CUTCLIP`, `COPYBASE`, `PASTECLIP` carry objects within a drawing and to the next | #223 | `tests/clipboard.rs`; the key routing's chords with and without typed text; `clipboard.spec.ts`, `m13-scenario.spec.ts` |
| A scenario drafts with them | #224 | `m13-scenario.spec.ts` |

## What was built

- **Grid snap** (#219):
  - `aids::grid_snap` with the snap spacing, across and up;
  - `Toggle::Snap` on F9;
  - `SNAP` with `Aspect`, `ON`, and `OFF`, and `GRID Snap`.
  - Grid snap applies in `Session::aim` to the free cursor.
- **Object snap tracking** (#220):
  - `aids::object_tracking` gives the nearest alignment path, or the crossing of two from different points.
  - The host times a rest of the pointer and calls `pointer_rest`, the one new coarse call, since Rust keeps no clock.
  - Up to seven points are acquired, marked in the overlay, and dropped when the command ends.
  - A distance typed on a path is measured along it.
- **Text styles** (#221):
  - A style holds a fixed height, a width factor, and an oblique angle.
  - A TEXT carries its own width and slant, as DXF does. Its width, box, and strokes follow them, and a mirrored text, redrawn readable, keeps its slant.
  - `STYLE` with its options; `TEXT` and `MTEXT` take the current style.
  - `$TEXTSTYLE` and the STYLE and TEXT groups are read and written.
- **A drawing as a block, a block as a drawing** (#222):
  - `TransactionBuilder::bring_in` turns another drawing's records into this one's, matching every row they refer to by name.
  - `INSERT ~` reads a file through the host and places it as a block named after it.
  - `WBLOCK` writes a block or a selection as a file.
- **The clipboard** (#223):
  - The clipboard is a small drawing of the session's own, kept across `OPEN`.
  - `COPYCLIP`, `CUTCLIP`, `COPYBASE`, and `PASTECLIP` use it, pasting through `bring_in`.
  - Ctrl+C, Ctrl+X, and Ctrl+V reach them only with nothing typed.

## Decisions made in M13

- **The pipeline's order** is object snap, then object snap tracking, then grid snap, then ORTHO or POLAR. Typed points bypass all of them (DESIGN §13.4, noted).
- **Rust keeps no clock.** A pointer at rest is the host's call, `pointer_rest`. The other choice was timestamps on every pointer event, which would have widened every call for one aid.
- **New toggles go last.** SNAP and OTRACK take the next bits, so the toggles a browser kept between sessions mean what they meant.
- **A text keeps its own width and slant**, as DXF's TEXT does and interactive CAD programs behave. A style's values are what a new text takes, and changing a style changes no text (noted on #221).
- **A file is inserted by `~` at the block name**, as classic `-INSERT` does, rather than by a keyword that would shadow block names.
- **Rows from another drawing are matched by name**, and the drawing's own row is kept when a name is taken. One function serves insertion, writing, and the clipboard.
- **The clipboard is the session's, not the system's** (§45). It is a drawing of its own, so it outlives the one it came from.

## What testing found

- **Negative zero in a file.** Grid snap rounded a point just below the origin to −0.0, which the writer wrote as `-0.0`. The M13 scenario's exact comparison caught it. `grid_snap` now adds zero, and its unit test checks the sign.
- **A fixed height changes the prompts.** With a style of fixed height current, `DT` skips the height. The scenario's first draft typed one anyway, and the command took the height as the rotation and the rotation as the text.
- **A slant through a mirror.** The first version negated the slant of a mirrored text. A mirrored text is redrawn readable, with its glyphs unmirrored, so the slant must stay. The property test now holds it.
- **Degrees and radians.** A slant read as 15° comes back 14.999999999999998° through radians, as rotations already do. Export, import, and export again is still byte-identical.
- **A name already taken.** The layouts fixture has a layer named TITLE, so a title block's TITLE layer took that drawing's colour on insertion. This is the rule working, and the spec now uses a name the fixture lacks.
- **Small test-writing errors:**
  - A click 5 px from a line's end took its grip, not the line.
  - The lower left of a copy included a circle's bottom below the line.
  - A status-bar locator for SNAP also matched OSNAP.
  - A render test divided a segment count by four.
  - The `Arc` around an inserted drawing was not `Send` and is now `Rc`.

## Measurements

- The suite at the exit: 1,677 Rust tests, 145 web unit tests, and 474 end-to-end runs across Chromium and Firefox, the M13 scenario in both.
- Tracking costs nothing until a point is acquired. After that, a pointer move tests at most seven points against their alignment lines: a handful of products, with no index query.
- Inserting, writing, and pasting are proportional to what is brought in. No measurement was needed at drafting sizes.

## What M13 taught us

- **The design's own sketch is a checklist.** SNAP and OTRACK sat in DESIGN §5's status bar, and in the UI as greyed-out labels, from the start; only a sweep against the design noticed nothing had filled them.
- **One rule for rows from elsewhere.** `bring_in` was written for `INSERT ~` and then carried `WBLOCK` and the clipboard unchanged. Matching by name in one place is why the three agree.
- **Keep the clock where the events are.** The host already owns time (frames, the autosave); asking it for one more timer kept the engine deterministic and its tests clock-free.
- **Exact comparisons in scenarios pay.** `-0.0` would have survived any tolerance.

## Open items

Needs a person (carried to M14 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF of a layout in a desktop viewer; an interactive CAD program reading our files with their text styles and inserted blocks.

Known limitations, accepted for now:
- The clipboard is not the system's, so it cannot carry objects to another program.
- A paragraph takes its style's height but not its width factor or slant, which DXF keeps as formatting codes.
- An inserted file's base point is its origin; `$INSBASE` is not read.
- Tracking acquires only by resting. There is no explicit acquire key, and no tracking from the last point (the classic "temporary track point").

## Is the product complete for its purpose?

The sweep again, against the design and the classic daily set. Drawing, modifying, organizing, annotating, paper space, plotting, inquiry, the drafting aids of the design's status bar, and reuse across drawings are now all there. What production sheets still reach for is:
- **Draw order and wipeouts.** A note over a hatch needs one or the other, and the committed scene's fixed fills-beneath-strokes order needs an ADR to change.
- **Ordinate dimensions and quick dimensioning**, for mechanical work.
- **Saved layer states**, for drawings that serve several sheets.
- **`BOUNDARY`**, the loop HATCH already finds, as a polyline.

They fit one milestone. Beyond them, what remains is product-level rather than a gap in the classic set: tables, multileaders, raster underlays, xrefs, SHX fonts, DWG, and scripting. The M14 review should weigh whether the product is then complete for its purpose, with those kept as the product's long-term ideas.

## M14, broken down

Epic #225, in order, each blocked by the one before:

1. **Draw order and wipeouts, by an ADR** (#226): `DRAWORDER`, `TEXTTOFRONT`, `WIPEOUT`, DXF `SORTENTSTABLE` and `WIPEOUT`.
2. **Ordinate dimensions and QDIM** (#227).
3. **Saved layer states** (#228): `LAYERSTATE`, kept in the drawing and its file.
4. **BOUNDARY** (#229).
5. **Exit** (#230).
