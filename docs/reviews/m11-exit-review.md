# M11 exit review — Polylines, Leaders, and Loose Ends

**Date:** 2026-10-14  
**Scope:** the loose ends the M8 to M10 reviews accepted for now, as broken down at the M10 exit ([roadmap](../roadmap.md), epic #203)  
**Outcome:** Every item of the M11 scope is implemented and tested, and one performance issue found on the way (#210) is fixed. An automated scenario opens a file whose layout plots at one to two turned a quarter turn, draws a wide polyline edited with `PEDIT`, a leader to a note, and construction lines trimmed into rays, quick-selects on two properties, plots the layout at the file's settings, and saves and reopens it all. The milestone is closed. Swept against the classic command set, the product is still one milestone short of complete: M12 is broken down below.

## The scenario

`web/e2e/m11-scenario.spec.ts`, in Chromium and Firefox:

1. **A file whose Layout1 plots at 1:2, turned 90°**: the layouts fixture with its `LAYOUT` groups 142/143 and 73 changed, as another program would leave them.
2. **A polyline with an arc, widened and closed** (`PEDIT` `Width` 3, `Close`).
3. **A leader to a note** (`LEADER`, two points, "Wall to be kept"), red, on a layer of its own.
4. **Two red construction lines** (`XLINE Ver`) **trimmed at the site line into rays** (`TRIM`).
5. **A quick select on two properties**: red ("3 match so far"), then on layer 0 ("2 match so far"): the two rays, not the leader.
6. **Layout1 plotted at the file's settings**, every prompt's default its own: `<1:2>`, `<90>`; a portrait A3 page, "at 1:2, turned 90°", the band filled, and every coordinate of the page's paths on the page — the rays, a million units long in the drawing, clipped to what the viewport shows of them.
7. **Saved**: the polyline with width 3 and closed, the leader on `NOTES` with its `MTEXT` note, no `XLINE`, two red `RAY`s from the site line, Layout1's settings in its groups.
8. **Reopened**: three lines, the polyline, the viewport, the leader, two construction lines; the same quick select finds the same two.

## Done when (epic #203)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| `PEDIT` closes, opens, joins, and edits vertices; a polyline has a constant width, drawn and plotted as a filled band, in DXF; `DONUT` | #204 | `tests/pedit.rs` (`pedit_closes_opens_widens_decurves_and_undoes_within_the_command_as_one_undo`, `pedit_joins_what_meets_the_polylines_ends_and_keeps_its_width`, `pedit_edits_vertices_next_previous_insert_move_and_straighten`, `a_wide_polyline_is_picked_on_its_band_and_keeps_its_width_through_edits`, `donut_draws_rings_until_enter_and_a_disc_for_zero_inside`); `the_band_stays_within_half_the_width_of_the_path` (`cad-geometry/src/band.rs`); `ezdxf-r2010-wide-polylines.dxf`, `ezdxf-r12-wide-polylines.dxf`; the baseline `wide-polyline-and-donut.png`; `docs/modify-matrix.md` (PEDIT); `wide-polylines.spec.ts`, `m11-scenario.spec.ts` |
| A leader is an entity with `LEADER`, grips, and DXF both ways | #205 | `a_leader_is_laid_out_from_its_style_with_the_note_past_a_landing`, `the_layout_follows_the_vertices_and_the_style` (`cad-document/src/leader.rs`); `tests/leader.rs`; `ezdxf-r2010-leaders.dxf` and its round trip; `m11-scenario.spec.ts` |
| `XLINE` and `RAY` exist as entities without bounds, by an ADR | #206 | [ADR-0031](../adr/0031-construction-lines.md) with its measurements; `construction_lines_are_queried_beside_the_tree_and_left_out_of_the_extents`, `a_construction_line_is_clipped_to_a_box_and_measured_exactly`; `tests/construction.rs`; `a_construction_line_plots_clipped_to_the_area_and_leaves_the_fit_alone`; `ezdxf-r2010-construction-lines.dxf`; `benches/benches/construction.rs`; `construction.spec.ts`, `m11-scenario.spec.ts` |
| `QSELECT` filters on several properties; the selection methods work at the idle prompt | #207 | `qselect_narrows_by_several_properties_and_appends_or_removes`, `qselect_tests_text_styles_dimension_styles_and_blocks_by_name` (`tests/advanced_selection.rs`); `advanced-selection.spec.ts` (`FENCE at the idle prompt`), `m11-scenario.spec.ts` |
| A layout's margins, scale, and plot area from its file shape the page | #208 | `a_layouts_page_is_its_sheet_at_its_own_scale_within_its_margins`, `a_layouts_plot_settings_shape_the_page`, `a_layouts_kept_plot_settings_are_read_from_its_groups` (`cad-plot/src/tests.rs`); `a_layouts_plot_settings_are_read_changed_and_written_back` (`cad-dxf/tests/fixtures.rs`); `tests/plot.rs`; [ADR-0027](../adr/0027-pdf-plot.md) §6; `paper-space.spec.ts`, `m11-scenario.spec.ts` |
| The overlay no longer reads every entity in view on every pointer move | #210 | `the_curves_larger_than_a_radius_follow_every_edit_and_redefinition`, the history property test's check of the list against every entity's primitives; `far_segments`, `zooming_far_in_on_a_curve_uploads_nothing_committed`, `curves.spec.ts`; `benches/benches/overlay.rs`; [performance](../performance.md#far-curves-and-the-overlay-2026-10-14-210) |
| A scenario drafts with them | #209 | `m11-scenario.spec.ts` |

## What was built

- **PEDIT, width, DONUT** (#204): `Path::band` — a quad per straight segment, a strip along an arc, a disc at every vertex — drawn as triangles through `walk_entity`, so the screen and the page share it; the width on `PolylineEntity`, kept through every edit that makes a polyline from a polyline and scaled by a scaling; `PEDIT` with every classic option and `Undo` within the command; `DONUT`; DXF 43 and agreeing 40/41.
- **Leaders** (#205): `LeaderEntity` laid out from its dimension style whenever it is drawn, picked, or bounded, nothing derived stored; the `MTEXT` a file's `LEADER` names by handle becomes the leader's note and no entity of its own, and is written back under the leader's 340.
- **Construction lines** (#206): ADR-0031 — a side list beside the R-tree, left out of the extents, read along a million-unit reach for geometry and locally where precision matters, drawn by the overlay clipped to the view and never by the committed scene; `XLINE` with every option and `RAY`; `TRIM` and `BREAK` turn them into rays and lines.
- **Selection loose ends** (#207): `QSELECT` narrows by any number of tests, with `Append` and `Remove`; `FENCE`, `WPOLYGON`, `CPOLYGON`, `PREVIOUS`, `LAST`, `ALL`, and `REMOVE` at the idle prompt through `Command::take_opening_keyword`, one path with `SELECT`.
- **A layout's plot settings** (#208): `LayoutPlot` — an area, a scale or a fit, quarter turns — read from the file's groups or defaulted, offered by `PLOT`, kept on the row, written back; the page held the other way for an odd turn, printed within the margins.
- **Far curves by radius** (#210): the drawing keeps its entities with a circle or an arc by their largest radius, and the overlay reads only those large enough to need the far-curve pass.

## Decisions made in M11

- [ADR-0031](../adr/0031-construction-lines.md): an entity without bounds lives beside the index, not in it, and is drawn as overlay; the committed scene stays view-independent (ADR-0003).
- [ADR-0027](../adr/0027-pdf-plot.md) §6: a layout plots by its own settings; "a fit of the sheet is one to one" gives way to the layout's scale, one to one by default.
- A polyline's width is constant. A file's varying widths are dropped and counted in the import report; a band per segment of its own widths is not taken up.
- A leader's note is the leader's words, not a second entity, so the leader moves, erases, and undoes as one.
- At the idle prompt the short forms `F`, `CP`, `P`, and `L` stay `FILLET`, `COPY`, `PAN`, and `LINE`: the selection methods take their full names there.
- The drawing keeps a second derived structure beside the index — the curves by size — updated in the same places and checked by the same history property test.

## What testing found

- **The construction-line benchmark found a cost nobody had measured.** The overlay rebuild at the extents of the 100k drawing took 25 ms with no construction line at all: the far-curve pass read every entity in view on every pointer move. It was filed as #210 and fixed: the rebuild is now under 5 ns there, and the Firefox pointer frame at 100k went from 123 ms to 63 ms.
- **The fix had a price that only the open showed.** Noting radii one insert at a time made opening the 100k drawing 10% slower; building the list in one sort brought that to 5% (201 to 211 ms). The figures are in [performance](../performance.md).
- **Precision on a million-unit segment.** Snaps on a construction line came out a ten-billionth off where it crossed something; they are now computed with the line clipped to the other object's bounds, and its crossing with another construction line exactly.
- **The plot's property test had a fixed slack.** A quarter-turn Bézier stands off its arc by a fixed fraction, which on A0 exceeded the test's half point; the slack now scales with the page, and the case is kept as a regression.
- **Keyword abbreviations.** `eXit` could not be typed as `X`, and `DIMstyle` and `Done` were chosen so that no two of QSELECT's keywords share a shortest form.
- **Tests that used what was unsupported.** The unsupported-kind tests read `RAY` and `XLINE`, which are now supported; they use `SOLID` and `TRACE`.
- **A fixture without its style.** The ezdxf leader fixture was written without the dimension style its leaders named, until the style was passed to the call that makes them.
- **The shipping order.** One issue's boxes were ticked before its commit had landed (an ESLint rule failed after the Rust tests passed); the boxes are now ticked only after `git log` shows the commit.

## Measurements

- The suite at the exit: 1,610 Rust tests, 142 web unit tests, 452 end-to-end runs across Chromium and Firefox, the M11 scenario in both.
- #206: a hundred construction lines add 0.2 ms to an overlay rebuild that then cost 25 ms, and 3 µs to a pick query ([ADR-0031](../adr/0031-construction-lines.md)).
- #210: the overlay rebuild at extents, natively, and the frames in Firefox:

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Overlay rebuild at extents, before / after | 1.30 ms / 4.3 ns | 8.64 ms / 4.7 ns | 23.3 ms / 4.7 ns |
| Firefox pointer frame, mean, before / after | 13.5 / 7.3 ms | 50 / 31 ms | 123 / 63 ms |
| Open, natively | | | 201 / 211 ms |

## What M11 taught us

- **Benchmark the thing next to the thing.** #206's benchmark was written to price construction lines, and found the far-curve pass, which had shipped three hours after the browser figures were taken and had never been measured.
- **A side list is cheaper than a special case in the tree.** Keeping unbounded entities beside the R-tree changed one query and one extents function; nothing in the index had to learn about infinity.
- **Read the file's settings as the defaults, not as overrides.** A layout's plot settings became `PLOT`'s defaults and a choice became the row's, so a file plots as it was set up and a change is one undo.
- **Measure the cost of a fix, too.** The radius list made pointer moves free and the open dearer; only the open benchmark said so.

## Open items

Needs a person (carried to M12 as #109, #110, #111, #175):

1. **Real GPUs** (#110) and **WebKit** (#109); the text atlas (#175) waits on the first.
2. **The R-tree crash upstream** (#111).
3. The PDF of a layout in a desktop viewer; an interactive CAD program reading our files with their wide polylines, leaders, construction lines, and layout settings.

Known limitations, accepted for now:

- A polyline's width is constant; varying widths are reported, not held.
- A leader is straight; a file's spline leader is read straight. `MLEADER` is reported, not read.
- Construction lines and far curves are drawn in the theme's entity colour, continuous, at the ordinary width on screen, because the overlay has no style table; on the page they have their own look.
- A frozen layer is read as off and written back off (taken up in M12).
- The open of the 100k drawing is 5% dearer for the list of curves by size.

## Is the product complete for its purpose?

The M10 review expected M11 to finish the product, and M11 closed every gap a review had named. This review checked differently: it swept the registry against the commands a production drawing uses daily rather than against what earlier reviews had accepted. That sweep finds five gaps no review listed, each of them classic and small:

- dimensions chained from a baseline or end to end (`DIMBASELINE`, `DIMCONTINUE`);
- layers switched by picking what is on them, with freeze kept as freeze;
- groups;
- revision clouds;
- find and replace across the drawing's text.

A drafter meets every one of them in a working week. `UCS` is out of scope by design (§4.1, LOCKED). Draw order and wipeouts need an ADR against the fills-beneath-strokes order of ADR-0021. Ordinate dimensions and `QDIM` are less common than the chains. The remaining ideas — SHX fonts, xrefs, scripting, DWG, a native format, WebGPU — are products of their own or wait on a person with hardware.

So the product is not yet complete for its purpose: it is short by one milestone of drafting conveniences. Its exit review must run the same sweep again.

## M12, broken down

Epic #211, in order, each blocked by the one before:

1. **Dimension chains** (#212): `DIMBASELINE` and `DIMCONTINUE` for linear, aligned, and angular dimensions; the style's baseline spacing, DXF `DIMDLI`.
2. **Layer tools by picking, and freeze kept** (#213): `LAYISO`, `LAYUNISO`, `LAYOFF`, `LAYON`, `LAYLCK`, `LAYULK`, `LAYMCUR`, `LAYFRZ`, `LAYTHW`; a frozen layer frozen through a DXF round trip.
3. **Groups** (#214): a group table changed in transactions, `GROUP` and `UNGROUP`, a pick selecting the whole group, DXF `GROUP` objects.
4. **Revision clouds** (#215): `REVCLOUD` from a rectangle, a polygon, or an object, as a closed bulged polyline.
5. **Find and replace** (#216): `FIND` across every kind of text, in one transaction.
6. **Exit** (#217).

Not in M12: `DRAWORDER` and `WIPEOUT` (an ADR against ADR-0021 first), `DIMORDINATE`, `QDIM`, layer states, freezing in one viewport, `MLEADER`, tables, fields, and the long-standing ideas on the epic.
