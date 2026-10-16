# M17 exit review — Multileaders

**Date:** 2026-10-16  
**Scope:** multileaders, the first long-term idea, taken up on the user's choice after M16 ([roadmap](../roadmap.md), epic #239, [ADR-0033](../adr/0033-multileaders.md))  
**Outcome:** Every item is implemented and tested. An automated scenario annotates a plate: one note with two leaders, and a balloon that is a block. It changes the style, then saves and reopens. ezdxf, an independent reader, reads what we write with no audit errors and places notes and balloons where we do. The milestone is closed. Nothing else is planned: what remains is M15's verification, which needs a person, and the other long-term ideas.

## The scenario

`web/e2e/m17-scenario.spec.ts`, in Chromium and Firefox, from an empty drawing:

1. **A plate** with two holes.
2. **A balloon block** made from a circle and its number. The reference `BLOCK` leaves is erased.
3. **A note with two leaders.** `MLEADER` makes it with one leader to the first hole. `MLEADEREDIT` adds the second, to the other hole.
4. **The balloon** on a corner: `MLEADER`, `Content`, `Block`.
5. **The style changed** with `MLEADERSTYLE`: arrowheads 4, dogleg 8. Both multileaders are redrawn by that one row.
6. **Saved.** Two `MULTILEADER`s:
   - the note, with two leader lines;
   - the balloon, by its block (content type 1).

   The `MLEADERSTYLE` has the new values.
7. **Reopened:** "2 multileaders" in the report, and nothing reported lost.

## Done when (epic #239)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An ADR decides what a multileader stores and what its style lays out; nothing laid out is stored | #240 | [ADR-0033](../adr/0033-multileaders.md); `the_layout_follows_the_points_and_the_style` (every leader ends at the landing, the dogleg reads away from the leaders, a moved multileader lays out moved) |
| `MLEADER` draws one, with text or a block as its note | #240, #241 | `tests/mleader.rs`: `a_multileader_is_drawn_from_its_arrowhead_to_its_note_as_one_undo`, `a_block_note_is_placed_at_the_dogleg_and_an_unknown_block_is_refused`; the baseline `mleader-note.png` |
| `MLEADEREDIT` adds and removes leaders | #241 | `mleaderedit_adds_and_removes_leaders_keeps_the_last_and_is_one_undo` |
| `MLEADERSTYLE` makes and changes styles; a change redraws every multileader in the style as one row | #241 | `mleaderstyle_makes_sets_lists_renames_and_deletes_and_says_why_not`; `a_style_naming_a_block_makes_block_notes_and_a_change_of_style_redraws_them`; `a_block_note_follows_its_block_and_its_style` (the index follows, undo puts it back); the render-boundary test `a_multileader_is_drawn_by_its_style_and_redrawn_when_the_style_changes` (only its chunk re-packed) |
| The style table changes in transactions, in the history property test | #240 | `derived_state_and_identity_survive_any_edit_sequence` |
| DXF `MULTILEADER` and `MLEADERSTYLE` read and written; what is not held is reported | #240, #242 | `multileaders_from_another_writer_round_trip`, `ezdxf-r2010-multileaders.dxf` (two leaders, a style of its own, a balloon, a style whose notes are a block; byte-identical); `multileaders_and_their_styles_survive_a_round_trip` (any multileader and style, float for float); the fuzz property gains both records; rows in [`dxf-matrix.md`](../dxf-matrix.md) |
| A scenario annotates with them | #243 | `m17-scenario.spec.ts` |

## Decisions

- **One landing.** AutoCAD's multileader may have a leader root on each side of its note. A file's second root is brought to the first's landing and reported. No command here makes one.
- **A block note's nearer side meets the dogleg, without the gap.** The middle of the block's left side touches the dogleg's end when the note reads right, its right side when the note reads left. AutoCAD and ezdxf place balloons this way; the fixture's balloon comes back at ezdxf's own insertion point, (20, 60). The gap is for words.
- **A style's default block is kept by name.** So a style needs no rule for a block removed or brought from another drawing, and a name no block has means words. A multileader's own block note is kept by ID, like a reference: the block cannot be removed while a note shows it, and a block cannot hold a multileader noting itself.
- **A block note is drawn in the multileader's look.** Its parts' colours are not resolved one by one, as a reference's are. This matches AutoCAD's default, a block colour by block.
- **The file is release 2000.** The R2007 and R2010 groups are left out, as ezdxf leaves them out for R2000. `$CMLEADERSTYLE` names the current style by handle.

## What testing found

- **Inserts were not checked.** `c0c2135` checked a multileader's style only where a drawing is assembled from a file, though its message said inserts. The transaction path let an unknown style through. `febbe72` checks the style, the block, and the points there, found by the drawing test of block notes.
- **The current style was dropped.** `with_settings` left out the current multileader style, so a file's `$CMLEADERSTYLE` never took effect. Found by the fixture.
- **A multileader's own normals and Z groups read as an elevation.** They marked every multileader as having lost properties. The reader no longer takes the record's groups after `AcDbMLeader` as the entity's.
- **Words lost their end spaces.** Kept groups were trimmed. The round-trip property found it; a multileader's words now keep them, as a text's do (#177).
- **The predicted style IDs were one off.** A table's first row is 0, not 1. Found by the fixture.
- **`D` meant both `Delete` and `Dogleg`.** So it selected neither. `DOgleg` takes two letters.
- **The M13 scenario failed about once a full e2e run.** The traces show the corner was never acquired. The first guess, a repeated move at the same place, was wrong (`294d3b6`), though the change it made stands. Under the suite's load, Firefox ran frames about once a second. The rest timer fired after the fixed wait was over, and the next move cancelled it. The host now counts the rests it tells the engine and where each one was, and the scenario waits for a rest at its point (`3e533ce`). The full runs since are clean.

## Measurements

- The suite: 1,750 Rust tests, 146 web unit tests, and 488 end-to-end runs across Chromium and Firefox.
- A multileader is laid out whenever it is bounded, picked, or drawn, as a dimension is. A style change re-packs only the chunks holding the multileaders it draws (ADR-0022).

## What follows

Nothing is planned. **M15 (#231)** still holds what needs a person:
- real GPUs (#110) and the text atlas (#175);
- WebKit (#109);
- the upstream crash (#111);
- a check in AutoCAD and a desktop PDF viewer (#232). That check now also covers multileaders, written in a release 2000 file as ezdxf writes them.

**The long-term ideas stay on the M15 epic:**
- tables and fields;
- raster underlays, xrefs, and SHX fonts;
- DWG and scripting.
