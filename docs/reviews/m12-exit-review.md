# M12 exit review — Drafting Conveniences

**Date:** 2026-10-14  
**Scope:** the conveniences the M11 review found by sweeping the registry against the classic daily command set ([roadmap](../roadmap.md), epic #211)  
**Outcome:** Every item of the M12 scope is implemented and tested. An automated scenario drafts with all of it: a plan dimensioned with a continued chain and a baseline chain, the walls isolated by picking and the rest brought back, a door grouped and moved as one, a revision cloud round the change, and a word replaced in every note; the file is then saved and reopened with it all. The milestone is closed. The sweep run again finds the product one milestone short of complete: M13 is broken down below.

## The scenario

`web/e2e/m12-scenario.spec.ts`, in Chromium and Firefox, from an empty drawing:

1. **A plan** on four layers: the walls as a rectangle, a door as a leaf and its swing, a text, and a leader with a note.
2. **Dimensioned** end to end along the bottom (`DIMLINEAR`, then `DIMCONTINUE` twice) and from one baseline up the side (`DIMLINEAR`, then `DIMBASELINE`): 40, 40, 40, 40, and 80.
3. **The walls isolated by picking one** (`LAYISO`): only the rectangle is left to select. `LAYUNISO` restores the four layers it switched off.
4. **The door grouped** from a selection (`GROUP`, unnamed `*A1`); a pick on the leaf takes the swing too, the Properties panel says "in group *A1", and the group is moved along the wall as one.
5. **A revision cloud round the change** (`REVCLOUD`, two corners): sixteen arcs.
6. **A word replaced in every note** (`FIND door gate`): the text and the leader's note.
7. **Saved**: five `DIMENSION`s on `DIMS` with their measurements, the door moved, one `GROUP` named `*A1`, the rectangle and the cloud as polylines, and the notes reworded.
8. **Reopened**: the five dimensions, the leader, the polylines, and the circle, with "1 group." in the report; eleven objects to select, and a pick on the leaf takes the door whole.

## Done when (epic #211)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `DIMBASELINE` and `DIMCONTINUE` chain linear, aligned, and angular dimensions, spaced by the style's baseline spacing, in DXF both ways | #212 | `a_continued_linear_dimension_shares_the_line_and_a_baseline_one_steps_out`, `an_aligned_chain_keeps_its_offset_on_the_same_side`, `an_angular_chain_turns_on_from_the_origin_the_way_the_angle_does`, `a_chain_starts_from_the_nearer_extension_line_and_never_from_a_radius` (`cad-document/src/dimension.rs`); `tests/dimchain.rs`; `a_baseline_chain_from_another_writer_is_spaced_by_its_style`, `ezdxf-r2010-dimension-chains.dxf`; `dimension-chains.spec.ts` |
| Layers switched by picking; a frozen layer frozen, not off, through DXF | #213 | `a_frozen_layer_is_not_shown_and_the_current_layer_cannot_be_frozen`, the history property test's freeze; `tests/laytools.rs`, `freeze_and_thaw_hide_and_show_a_layer_apart_from_on_and_off`; `frozen_and_off_layers_from_another_writer_round_trip_as_they_were`, `ezdxf-r2010-frozen-layers.dxf`; `layer-tools.spec.ts` |
| A group is a named set of objects, selected whole, kept through the history, DXF `GROUP` objects both ways | #214 | `groups_hold_existing_entities_and_lose_them_as_they_are_removed`, the history property test's create, add, remove, and rename; `tests/groups.rs`; `groups_from_another_writer_keep_their_members_and_round_trip`, `ezdxf-r2010-groups.dxf`; the key routing's chord; `groups.spec.ts` |
| `REVCLOUD` from a rectangle, a polygon, or a closed object | #215 | `a_cloud_keeps_its_chords_within_the_lengths_and_its_vertices_on_the_outline` and the unit tests (`cad-geometry/src/cloud.rs`); `tests/revcloud.rs`; the baseline `revision-cloud.png`; `revcloud.spec.ts` |
| `FIND` finds and replaces words in every kind of text, in one transaction | #216 | `words_are_replaced_by_case_and_by_whole_word_as_asked` (`commands/find.rs`); `tests/find.rs`; `find.spec.ts` |
| A scenario drafts with them | #217 | `m12-scenario.spec.ts` |

## What was built

- **Dimension chains** (#212):
  - `DimensionEntity::chained` makes the next dimension of a chain.
    - Linear: at the same rotation.
    - Aligned: with its offset kept on the same side in the world.
    - Angular: turning on past the origin (continue) or across the angle (baseline), in the angle's own sense.
  - `nearer_origin` picks the extension line nearer a pick.
  - `DimStyle::baseline_spacing` is DXF `DIMDLI`.
- **Layer tools and freeze** (#213):
  - `Layer::frozen` beside `on`, and `Layer::is_shown`, which everything that shows or picks follows.
  - `LAYISO` and `LAYUNISO`, with what `LAYISO` changed kept in the session.
  - `LAYOFF`, `LAYFRZ`, `LAYLCK`, `LAYULK`, `LAYMCUR`, `LAYON`, `LAYTHW`.
  - `Command::picks_locked_objects`, so `LAYULK` can pick what a locked layer holds.
  - A Freeze column in the Layers panel.
- **Groups** (#214):
  - A group table with no permanent row, changed in transactions.
  - A removed entity leaves its groups in the same transaction.
  - `GROUP` with its options, `UNGROUP`, and `PICKSTYLE` with Ctrl+Shift+A.
  - The selection grows to whole groups in `Session::change_selection` alone.
  - DXF `GROUP` objects, named by their `ACAD_GROUP` entries.
- **Revision clouds** (#215): `Path::revision_cloud` cuts each side of an outline into the fewest equal arcs whose chords are within the longest, so corners are vertices. `REVCLOUD` makes one round a rectangle, a polygon, or a closed object, in its place.
- **Find and replace** (#216): `FIND` looks in texts, paragraphs, attribute defaults and values, leader notes, and dimension texts. Replacing is one transaction, and a locked layer's text is found but kept.

## Decisions made in M12

- A chain starts from the newest dimension in the space, by entity ID: the one last made, with no session state for undo to keep in step (noted on #212).
- Frozen is its own switch, not a kind of off. Thawing does not switch on what was off, and both read back from a file as they were written.
- What `LAYISO` changed is session state, like the view. `LAYUNISO` puts back only the layers still as `LAYISO` left them, so a change made since is kept.
- Nothing acts on a locked layer's objects, except a command that changes the layer itself, which says so (`picks_locked_objects`).
- A group's row may change more than once in one transaction, each change from the state the one before left. Erasing two members of one group in one gesture needs it, and every other table keeps its one change per row.
- A file's groups are imported one by one, so one bad group is reported and skipped rather than losing the file.
- A revision cloud is a polyline, not an entity kind: `PEDIT`, the modify commands, and DXF take it unchanged.
- `FIND`'s options are keywords at its first prompt, like every prompt's. Searching for a bare prefix of `Match case` or `Whole words` — "m", "match", "w", "whole" — switches the option instead.

## What testing found

- **A narrow filter hid a compile error.** The style strategy of a property test built `DimStyle` field by field, and a test run filtered for failures showed nothing because the test target did not compile. The check caught it. Test output is now filtered for `^error` too.
- **A wording change broke a spec's formatting.** Changing the current-layer refusal across specs pushed one line past Prettier's width; the whole web tree is now linted before a ship.
- **Erasing a whole group touched one row twice.** The rule that a transaction changes each thing once does not fit a group losing two members; the group's changes stack instead (above).
- **A refused group left its selection.** After a refused `GROUP Create`, the next `GROUP` grouped that selection at once, unnamed: noun-verb working as designed, which the test first misread.
- **A generator, not the cloud, was wrong.** The cloud's property test drew thin outlines bowed far inwards, whose inward clouds crossed and fooled the area comparison; the outlines are now kept wide enough, and the seed is kept.
- **Counting arcs by hand.** A circle of radius 20 takes five arcs per half turn under 15, not nine for the whole; a 100 by 80 rectangle takes 26. The tests corrected the expectations.
- **A float compared exactly.** `PICKSTYLE` first read its 0 or 1 as a number, which the lints refuse to compare exactly; it reads text.
- **A misplaced comment from M11.** The `editor` method's doc comment had been left above `take_opening_keyword` by #207; it is back.

## Measurements

- The suite at the exit: 1,652 Rust tests, 143 web unit tests, 464 end-to-end runs across Chromium and Firefox, the M12 scenario in both.
- Nothing in M12 touched the frame or the scene's cost. Group selection reads the groups only when there are any; a drawing without groups pays nothing.

## What M12 taught us

- **A table rule is a default, not a law.** One change per row per transaction keeps most tables simple, and groups showed where it stops fitting; the exception is one function, `group_now`, reading the newest change.
- **Put the expansion where every path meets.** Group selection lives in `change_selection`, which picks, windows, fences, and `ALL` already shared, and so it needed no second implementation.
- **Session state is the honest home for interaction memory.** `LAYISO`'s record and the cloud's arc lengths are the session's, like the view, and the history stays the drawing's.
- **Cut per side.** Placing a cloud's vertices per side of the outline, not along its whole length, kept corners exact and made the chord bounds provable.

## Open items

Needs a person (carried to M13 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF of a layout in a desktop viewer; an interactive CAD program reading our files with their groups, frozen layers, and chained dimensions.

Known limitations, accepted for now:

- A layer is frozen everywhere, not in one viewport; there are no layer states.
- A group has no order of its members to change (`GROUP Order`).
- `REVCLOUD` has no freehand mode and no calligraphy style.
- `FIND` searches the current space, not block definitions, nor attribute tags and prompts.
- A radius or diameter dimension cannot be chained, as classically.

## Is the product complete for its purpose?

The M11 review promised to sweep the registry again, and this review does. Most of the classic daily set is there:

- drawing, modifying, and organizing;
- annotating with chains and leaders;
- groups, clouds, and search;
- inquiry, paper space, and plotting.

What the sweep finds missing is of two kinds:

- **Drafting aids the design itself names.** The status bar §5 draws reads `SNAP GRID ORTHO POLAR OSNAP OTRACK LWT MODEL`, and two of those, grid snap and object snap tracking, were never built.
- **Reuse across drawings.** A title block or a symbol cannot be brought from one drawing into another: there is no `INSERT` of a file, no `WBLOCK`, and no clipboard. Text styles, likewise, come only from files.

Every one of these is classic and daily, and together they fit one milestone. Draw order, wipeouts, and ordinate dimensions remain smaller and are kept on the epic. So the product is not yet complete for its purpose: it is short by the aids its own design lists and by reuse.

## M13, broken down

Epic #218, in order, each blocked by the one before:

1. **Grid snap** (#219): `SNAP` and F9, in `Session::aim` after object snap.
2. **Object snap tracking** (#220): `OTRACK` and F11, alignment paths from acquired snap points.
3. **Text styles** (#221): `STYLE` with height, width factor, and oblique angle.
4. **A drawing as a block, a block as a drawing** (#222): `INSERT` a DXF file, `WBLOCK`.
5. **The clipboard** (#223): `COPYCLIP`, `CUTCLIP`, `COPYBASE`, `PASTECLIP`, kept across `OPEN`.
6. **Exit** (#224).

Not in M13: `DRAWORDER` and `WIPEOUT` (an ADR against ADR-0021 first), `DIMORDINATE`, `QDIM`, `BOUNDARY`, layer states, and the long-standing ideas on the epic.
