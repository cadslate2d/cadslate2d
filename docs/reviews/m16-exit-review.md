# M16 exit review — Occasional Conveniences

**Date:** 2026-10-16  
**Scope:** the conveniences the M14 review set aside, taken up on the user's word ([roadmap](../roadmap.md), epic #233)  
**Outcome:** Every item is implemented and tested. An automated scenario cleans up a drawing as it might arrive — duplicates removed, layers merged and deleted — edits a spline, and saves and restores a named view, across a save and a reopen. The milestone is closed. Nothing else is planned: what remains is M15's verification, which needs a person, and the long-term ideas.

## The scenario

`web/e2e/m16-scenario.spec.ts`, in Chromium and Firefox, from an empty drawing:

1. **As it arrived.**
   - On `WALLS`: a wall drawn twice and once more in an overlapping piece, and a column drawn twice.
   - A wall on `WALL-OLD`, a note on `NOTES`.
2. **`OVERKILL ALL`.** Three objects go, and the oldest wall is lengthened to cover the pieces.
3. **`LAYMRG`** folds `WALL-OLD` into `WALLS`. **`LAYDEL`** removes `NOTES` and its note, after asking.
4. **A spline closed** with `SPLINEDIT`.
5. **A named view** of the column, saved after `ZOOM Window` and restored after `ZOOM Extents`.
6. **Saved:**
   - two lines and one circle, and no text;
   - the spline flagged closed;
   - neither old layer;
   - the view in the `VIEW` table.
7. **Reopened:** "1 named view" in the report, and the view restored.

## Done when (epic #233)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `OVERKILL` removes duplicates and overlapping collinear lines, as one undo | #234 | `collinear_runs` unit and property tests (`cad-geometry`); `overkill.rs` unit tests and the property test that a second pass finds nothing and every point drawn before still is; `tests/overkill.rs` |
| `VIEW` saves, restores, deletes, and lists named views, in the drawing and its DXF `VIEW` table | #235 | The history property test's views; `named_views_are_checked_and_go_with_their_layout`; `show_puts_back_what_the_canvas_showed` (`cad-core`); `tests/view.rs`; `named_views_from_another_writer_round_trip`, `ezdxf-r2010-named-views.dxf` |
| `LAYMRG` and `LAYDEL`, one undo each | #236 | The history property test's merge and delete; `tests/laymerge.rs` |
| `SPLINEDIT` closes, opens, reverses, edits fit points, and makes a polyline | #237 | `spline_edit.rs` unit tests and the property test that reversing twice is the spline and once the same curve backwards; `tests/splinedit.rs` |
| A scenario drafts with them | #238 | `m16-scenario.spec.ts` |

## Decisions

- **A restored view is shown whole, without a margin.** `View::show` puts back what the canvas showed. `fit`'s margin would have drifted the view a tenth each time it was saved and restored.
- **A named view moves the session's view, so `U` does not undo it.** The view is session state, never in the history, as with `ZOOM`; `ZOOM Previous` comes back from it. Saving and deleting are drawing changes and undo.
- **A paper-space view from a file goes to the first layout.** The `VIEW` table says only that a view is of paper, not of which layout.
- **Merging and deleting layers are one builder call each.** `remove_layer` refuses a layer with anything on it in the drawing as it was, so the layer cannot go in the same transaction that empties it. `merge_layers` and `delete_layers` move or erase everything, in blocks too, and then remove the layer.
- **Closing a spline drawn through points fits it through its first point again.** The curve meets itself with a joint where it began, not as a periodic curve. A spline of control points only closes its control polygon, with even knots.

## What testing found

- `fit`'s margin, above: a saved view came back a tenth smaller.
- A block's reference is on the layer that was current when the block was made. `LAYMRG` moved it with the line inside the block, so the scenario counted five objects, not four.
- "1 layer deleted, with 1 object on them" now reads "on it".

## Measurements

- The suite: 1,735 Rust tests, 146 web unit tests, and 484 end-to-end runs across Chromium and Firefox.
- `OVERKILL` sorts lines by the direction and position of their line, so it compares only neighbours. Other objects are compared within a sorted window.

## What follows

The product was complete for its purpose at M14, and M16 adds the conveniences that review named. No milestone of features follows.

**M15 (#231)** holds what needs a person:
- real GPUs (#110) and the text atlas (#175);
- WebKit (#109);
- the upstream crash (#111);
- a check in AutoCAD and a desktop PDF viewer (#232). That check now also covers named views and the layers merged and deleted.

**The long-term ideas stay on the M15 epic:**
- `MLEADER`, tables, and fields;
- raster underlays, xrefs, and SHX fonts;
- DWG and scripting.
