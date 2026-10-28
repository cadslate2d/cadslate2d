# M25 exit review — Composed Sheets

**Date:** 2026-10-28  
**Scope:** sheets of several views — a part's, a structure's, a specification sheet — composed by operations that place and tie their parts, while the model draws inside each view. Asked for after the maintainer's Iron Man and spillway sheets of 2026-10-27 ([roadmap](../roadmap.md), epic #360, [ADR-0041](../adr/0041-composed-sheets.md)).  
**Outcome:** Every item is implemented and tested, by command and by request, in English and Portuguese. The corpus's new level, L5, passes all eight of its sheets in one recording. Three things need a person:
- ADR-0041 was reviewed twice by the maintainer and accepted;
- the geometry inside a view is still the model's own, so a sheet can be well composed and mechanically wrong (below);
- the corpus's recordings are of one run per instruction change.

What M25 delivered:
- **Views** (#351): `VIEWFRAME` places drawn objects as views on the sheet a title block drew.
  - A view is the principal view, projected from another looking from a side, the rear, above, or below, or free.
  - Placement is two-pass and measures what will be drawn: dimensions, markers, and titles included.
  - A projected view is aligned to its parent on the side first- or third-angle projection puts it. Views already placed never move.
- **Datums** (#361): named positions declared once and used in several views. They are read into signed numbers when the document is parsed, by the orientation table, so the views that use one agree.
- **The scale contract** (#352): a free view at a scale of its own is drawn true size and enlarged. Its dimensions read true through a style made for it (DIMLFAC 1/k, DIMSCALE N), and DIMLFAC is read and written in DXF.
- **Sections** (#353): a cut given across its parent view, marked there with the cutting line, arrows, and its letter, and titled to match.
- **Parts** (#354): `BOMTABLE` draws the bill of materials in paper millimetres. `BALLOON` points at a part it lists and refuses one it does not.
- **Chains and notes** (#355): `dim_chain`, placed beyond what its view has drawn on that side; `NOTES` draws a wrapped block in paper millimetres.
- **Layers through the AI** (#356): a colour and a linetype, built-in linetypes loaded on the way.
- **The review** (#357, #358): the AI service runs the engine's review on the model's sheet and asks once more with the findings. It finds:
  - a view's objects outside its frame, views overlapping, a projected view out of line or not as tall or as wide as its parent;
  - a section without its marker, a balloon for a part no table lists;
  - a datum declared for a view that does not use it;
  - a view whose own drawing is refused;
  - a sheet that cannot be composed, with the smallest paper it composes on.
- **Through the AI:** intent versions 15 to 21 add `view`, `datum`, `bill_of_materials`, `balloon`, `dim_chain`, `notes`, and `layer`'s colour and linetype. The corpus gains a level, **L5**, judged on four scores apart.
- **Sheets drawn by the AI before M25** (#348): colours by common name or hex, a title block's values set smaller to stay in their boxes, and instructions that draw true size and dimension.

## The scenario

`web/e2e/m25-scenario.spec.ts` runs in Chromium and Firefox. Its request is answered by the tests' stand-in service from the L5 corpus's recording of the real model:
1. **A bracket's sheet by a request**: "Draw a drawing sheet for an L-bracket on an A4 sheet at 1:1 in third-angle projection, …". It is accepted as one proposal. The front, left side, and top views come back placed and titled, with at least three dimensions, the bill of materials, and its balloon.
2. **Edited by command**: `NOTES` adds a block of the user's own beside the model's, placed where there is room. It is undone in one step and redone.
3. **Exported**: saved as DXF and plotted to PDF.

## Done when (epic #360)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An ADR: what the program composes and what the model draws | #350 | [ADR-0041](../adr/0041-composed-sheets.md), accepted after two reviews; its "As built" section records each issue's choices |
| Views placed and aligned by projection, in their own coordinates | #351 | `cad_geometry::composition`: slots, and property tests that nothing overlaps and placed views never move. `tests/viewframe.rs` |
| Shared references | #361 | `cad_intent::datums`: the orientation table's signs, refusals, unused datums as findings. `tests/intent.rs` |
| A detail at its own scale reading true | #352 | `a_two_to_one_detail_on_a_one_to_three_sheet_is_drawn_six_times_and_reads_true`; the `ezdxf-r2010-dimlfac.dxf` fixture |
| Sections marked across their parents | #353 | `a_section_is_marked_across_its_parent_and_titled_to_match` |
| Parts, a table, balloons | #354 | `bom.rs` and `tests/bom.rs` |
| Chains and notes | #355 | `tests/bom.rs`, `tests/intent.rs` |
| Layers with colour and linetype through the AI | #356 | `tests/intent.rs`; LAYER's tests |
| The review checks the structure | #357, #358 | `review.rs`: a test per finding and per sheet without it. `the_review_finds_what_breaks_a_composed_sheet` |
| L5 scored four ways, in English and Portuguese | #358 | `cad_commands::evaluation`: each score failed alone in `a_sheet_is_judged_on_structure_agreement_dimensions_and_print_scale`. `test-data/ai/l5`: bracket, spillway, armour, pump, 8/8 |
| A scenario and a review | #359 | `web/e2e/m25-scenario.spec.ts`; this review |

## What it does not do, and why

- **The geometry inside a view is the model's.** The program composes the sheet, but it does not project one view from another. Views agree as far as the model uses datums and draws whole outlines. The review catches a projected view shorter or narrower than its parent, but not a feature at a different place in each. ADR-0041 §8 says what would cross that line: a 3D description, which DESIGN §4 keeps out.
- **A sheet can be structurally right and mechanically wrong.** Nothing checks that an assembly works, that a section shows what a real cut would, or that a bill of materials is complete. The replies say so where the request leaves shapes unspecified.
- **Views are orthographic, sections are straight cuts, and details are free views at a scale.** Auxiliary views at an angle, broken views, offset or aligned sections, and detail circles on the parent are not drawn.
- **A datum is one position on one side.** It has no expressions: a symmetric object's two sides are two datums.
- **The stand-in service serves a conversation's last attempt.** The real service serves the last valid one, as L5's judge does. Every recording the e2e flows replay ends in a valid answer.

## Measurements

**The AI corpus** at intent version 21 (`test-data/ai/README.md`):

| Level | en | pt |
| --- | --- | --- |
| L1 | 42/42 | 42/42 |
| L2 | 29/29 | 29/29 |
| L3 | 26/26 | 26/26 |
| L4 | 9/9 | 9/9 |
| L5 | 4/4 | 4/4 |

L5 took six rounds of recording, from 0/6 to 6/6, then two more for the pump. Each round found something the engine or the instructions now say; the corpus README lists them. One L2 answer slipped (an unasked colour) and was right three times in three when asked again.

**Latency.** A sheet takes up to three model calls: the answer, a correction or a review, and one more. Over the last full L5 recording the median sheet took 100 s, the slowest 114 s, under the service's 240 s limit for asking again.

## The whole suite

`make e2e`: 616 flows pass in Chromium and Firefox, the M25 scenario among them. 34 are skipped, as before M25: the visual baselines compared in Chromium only, and the few flows that need one browser's behaviour.

## Registry sweep

New commands, each registered, in the README's table, on a menu, and reachable by intent:

| Command | Menu | Intent operation |
| --- | --- | --- |
| `VIEWFRAME` | Insert › View Frame... | `view` |
| `BOMTABLE` | Insert › Bill of Materials... | `bill_of_materials` |
| `BALLOON` | Insert › Balloon | `balloon` |
| `NOTES` | Insert › Notes... | `notes` |

`dim_chain` compiles to `DIMLINEAR` and `DIMCONTINUE`, `datum` to nothing (it is read when the document is parsed), and `layer`'s colour and linetype to `LAYER`.

## For the maintainer

- Ask the AI panel for a sheet of your own, the Iron Man sheet among them, and look at what the review made of it. The reply says what is schematic.
- Try `VIEWFRAME` on views you draw by hand: Insert › View Frame....
