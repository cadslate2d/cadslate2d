# M2 exit review — OSNAP, Grips, and Basic Modify

**Date:** 2026-10-04  
**Scope:** DESIGN §30  
**Outcome:** Every deliverable in §30 is implemented and tested, and an automated drafting scenario exercises them together. **One thing is not done:** the exit condition is a judgement about how the program feels, and the M2 exit issue asks for it to be made by someone with classic CAD experience. That assessment has not happened; the milestone stays open for it alone.

> **Exit condition (§30):** Ordinary drafting begins to feel recognizably like classic CAD.

## The scenario

`web/e2e/m2-scenario.spec.ts` drafts a small plate in the production bundle, in Chromium and Firefox:

1. Four edges drawn with `LINE`, deliberately not meeting.
2. `FILLET` with radius 10 joins two of them with a tangent arc; `CHAMFER` with distance 15 bevels the next corner; `FILLET` with radius 0 makes the third corner sharp.
3. A **grip** stretches the left edge down to the last corner, placed by typed coordinates; `EXTEND` carries the bottom edge on to it.
4. `CIRCLE`, with its centre taken by **object snap** from the midpoint of an edge; the status bar names the snap and shows the exact point before the click.
5. `TRIM` cuts away the half of the circle outside the plate; `MIRROR` copies the resulting notch to the other side.
6. `OFFSET` the bottom edge; `SCALE` and `ROTATE` the new line about its start.
7. With **ORTHO** on, a line straight down from an endpoint found by snap.
8. Sixteen undos empty the drawing — one per command — and sixteen redos restore it byte for byte. Export, refresh, reopen: the same file.

Geometry is asserted exactly after every step, through the exported DXF.

## Deliverables (§30)

| Deliverable | Where | Notes |
| --- | --- | --- |
| Endpoint, midpoint, center, quadrant, intersection, nearest snaps | `cad-commands/src/snap.rs` | Candidates come from the spatial index; snap points from an entity's primitives. Closest within 10 px wins; Nearest only as a fallback |
| Transient-edit snap exclusion (§18.2, LOCKED) | `Command::transient_sources` | MOVE, ROTATE, SCALE, and grip edits never snap to what they are moving |
| ORTHO, basic POLAR | `cad-commands/src/aids.rs`, `Session::aim` | One pipeline: typed > snap > ORTHO or POLAR > free. Mutually exclusive. `POLARANG` sets the increment |
| GRID | `OverlayItem::Grid`, `cad-render` | Adaptive 1-2-5 spacing; overlay content drawn beneath the scene; `GRID` sets the spacing |
| Line, circle, arc, polyline grips | `cad-document/src/grips.rs` | `Entity::grip_moved` is the pure edit function |
| Transient grip edits, typed and OSNAP-assisted placement | `commands/grip.rs` | A grip edit is a command — [ADR-0010](../adr/0010-grip-editing-and-arc-grip-semantics.md) |
| `ROTATE`, `SCALE`, `MIRROR` | `cad-geometry/src/transform.rs`, `commands/transform.rs`, `commands/mirror.rs` | One `Transform` type; MOVE, COPY, ROTATE, SCALE share a state machine |
| Line and circle offset | `cad-geometry/src/offset.rs`, `commands/offset.rs` | |
| Simple trim, simple extend | `cad-geometry/src/{trim,extend}.rs`, `commands/edge.rs` | One edge; lines, arcs, circles |
| Basic line–line fillet, basic chamfer | `cad-geometry/src/corner.rs`, `commands/corner.rs` | |

Beyond the list: the `OSNAP`, `GRID`, and `POLARANG` setting commands; snap and tracking readouts in the status bar; press–drag–release on grips; toolbar buttons for every modify command.

Test counts at this review: 716 Rust tests, 94 Vitest tests, 125 Playwright tests in Chromium (118 in Firefox; the seven visual baselines are compared in Chromium only).

## Supported inputs

The narrow modify commands refuse what they do not support, by name, and change nothing. The supported combinations are published in [`docs/modify-matrix.md`](../modify-matrix.md), each backed by tests: OFFSET (lines, circles), TRIM (lines, arcs, circles against any edge), EXTEND (lines, arcs to any boundary), FILLET and CHAMFER (two lines).

## Visual regression

`test-data/screenshots/` holds a baseline for each of the six snap markers and for a grip, idle and hot. They are clipped to 28 × 28 pixels, compared in Chromium with software WebGL — the configuration CI runs — and have passed in CI since they were added.

## Decisions made in M2

| Item | Decision |
| --- | --- |
| Arc grip semantics (§16.2, "validated during M2") | Start, midpoint, and end re-fit the arc through the dragged point and the other two; the centre moves it — [ADR-0010](../adr/0010-grip-editing-and-arc-grip-semantics.md) |
| How a grip edit runs (§16.3) | As an ordinary command started by clicking a grip — ADR-0010 |
| Spatial index (§20, PROVISIONAL) | R-tree confirmed with snapping and window-selection workloads — [ADR-0007](../adr/0007-spatial-index.md), re-measured |
| Point acquisition | `Session::aim`, derived on demand, so the marker, the readout, the preview, and the click cannot disagree |
| Where the grid lives (§12.2) | In the overlay frame, drawn beneath the scene; not a third render lifetime |
| Defaults | OSNAP and GRID on in a new session; ORTHO and POLAR off; POLAR increment 90° |
| Picking one object | A new input kind, `Object`: the command receives the entity and the pick point |
| Transforms | Move, rotate, uniform scale, mirror as one `Transform`, each computed directly so common cases stay exact |
| Perpendicular and tangent snaps | Left out of M2 as §30 has it; taken into M3 |

## Measurements

Native x86-64, `opt-level = 3`, generated drawings of constant density. The machine was busy during these runs; treat differences under about 30% as noise.

| What | Result |
| --- | --- |
| Snap query next to an endpoint | 0.9 µs at 10k entities, 1.3 µs at 50k, 1.2 µs at 100k |
| Snap query in a crowded spot | 24 lines through one point 41 µs (276 pairs intersected); 2000 lines 0.46 ms (pair search skipped above 24 candidates) |
| Grip-drag frame, 10,000-vertex polyline, all grips shown | 173 µs; commit plus undo 124 µs |
| Window selection over 1% of a 100k drawing | R-tree 9 µs, brute force 0.6 ms |
| Window selection over 25% of a 100k drawing | R-tree 0.33 ms, brute force 0.63 ms |
| Update 1000 entries at 100k | R-tree 3.6 ms, brute force 57 ms |

Every per-frame cost is far inside a 16 ms frame.

**Not established**, still: frame time on a real GPU. That needs a person with hardware and remains tracked.

## What M2 taught us

- **Making the snap a derived value removed a class of bugs.** There is no stored "current snap" to go stale after a zoom, a toggle, an undo, or a command step. The cost is computing it up to three times per pointer frame, at about a microsecond each.
- **Commands were the right home for grip edits.** Typed coordinates, snaps, ORTHO, POLAR, previews, cancel, and undo grouping all came for free. The one addition to the `Command` trait was `grip()`.
- **Decomposing entities into primitives keeps paying.** Snapping, intersections, trim edges, and extend boundaries all work on polylines without any polyline-specific code. The same decomposition is what the polyline bulge model in M3 must preserve.
- **Exactness is worth special cases.** Quarter turns, axis-parallel mirrors, and translations are exact because they are computed directly. Extending a line along its own exact line, rather than along a long probe segment, removed a one-ulp error the first test caught.
- **Typed errors made the narrow cases honest.** Each modify operation returns its result or a reason as a value, so "not supported yet" is a tested outcome, and the published matrix is the list of non-error cases.
- **A long measurement window matters on a shared machine.** A two-second benchmark run showed the R-tree losing to brute force on large windows; a six-second run showed it winning by 2×. The first number was noise.
- **Object picks are blind.** TRIM, EXTEND, OFFSET, and FILLET give no feedback about what a click will pick until after the click. Hover feedback is the most obvious usability gap and is in the M3 plan.
- **A grip sits where people click to select.** Clicking a line at its midpoint selects it; a second click there takes the midpoint grip and moves the line. This is classic behaviour, and Escape puts it back, but it will surprise newcomers.

## Open items

Needs a person:

1. **The exit condition itself**: someone with classic CAD experience drafts with the build and says whether it "begins to feel recognizably like classic CAD". The scenario above is a script for it.
2. **Arc grip behaviour against AutoCAD** (ADR-0010): the comparison is from documented behaviour, not a hands-on check.
3. Render measurements on real GPU hardware.
4. Whether to report the R-tree library's bulk-load crash upstream.

Carried forward as issues: Firefox in CI and any WebKit run; a fallback for curves at extreme zoom; unit suffixes and drawing units; a safe bulk loader for the spatial index.

Known limitations, accepted for now: no grip menu (Lengthen, Radius, add or remove vertex); snap candidates on a very large polyline are computed from all of its segments; the snap aperture and polar increment are not persisted.

## M3, rewritten

§31 says "the exact scope should be rewritten after real M2 geometry exists". It now reads, in order ([`docs/roadmap.md`](../roadmap.md) has the reasoning):

1. **The polyline bulge model**, then DXF bulges and `PLINE` arc segments. Most of the rest depends on it.
2. **Perpendicular and tangent snaps.**
3. **Hover feedback for object picks**, then **TRIM and EXTEND** with several edges and with polylines as targets.
4. **OFFSET** for arcs, then — after an experiment and an ADR — for polylines, with self-intersection cleanup.
5. **FILLET and CHAMFER** with arcs and circles, and on polylines.
6. **BREAK, JOIN, STRETCH.**
7. **The regression corpus and property-based stress tests**, grown alongside each item rather than left to the end.
