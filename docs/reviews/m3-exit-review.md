# M3 exit review — Robust Modify Geometry

**Date:** 2026-10-04  
**Scope:** DESIGN §31, as rewritten at the M2 exit ([roadmap](../roadmap.md))  
**Outcome:** Every item of the rewritten M3 scope is implemented and tested, an automated scenario exercises them together, and the milestone is closed. The stress tests the plan asked for found real defects in geometry that had already shipped — in M2 as well as M3; they are fixed, each with a regression test. Several choices this milestone made about how commands behave should be looked at by someone who drafts for a living; they are listed under *Open items*.

§31 has no exit sentence of its own. The epic's "done when" list stood in for one, and is what the criteria below are checked against.

## The scenario

`web/e2e/m3-scenario.spec.ts` drafts a slotted link in the production bundle, in Chromium and Firefox:

1. `PLINE` with its `Arc` and `Line` options draws a slot as **one polyline with two half-circle ends**.
2. `OFFSET` makes a second slot 10 outside it: the arcs come back as arcs, the result is one closed polyline.
3. Two construction lines are drawn across both slots.
4. `TRIM` with every object as an edge: each line is cut by **both polylines at once** — its two outer ends and its middle go, leaving four webs. Then the outer slot itself is trimmed between two webs: **a closed polyline as the object trimmed**, which opens.
5. `JOIN` makes the opened slot and the two webs at its ends into one polyline of eight vertices.
6. `STRETCH` with a crossing window pulls the right-hand end 20 further out; both polylines follow, their arcs still half circles. The preview uploads no committed geometry.
7. Seven undos empty the drawing — one per command — and seven redos restore it byte for byte. Export, refresh, reopen: the same file.

Geometry is asserted after every step, through the exported DXF.

## Done when (epic #102)

| Criterion | Where | Evidence |
| --- | --- | --- |
| Polylines can contain arc segments, and every operation that reads geometry handles them through `Entity::primitives` | `cad-geometry/src/polyline.rs`, `cad-document/src/entity.rs` | Bulge ↔ arc property tests; `cad-commands/src/tests/bulged.rs` runs selection, snapping, trim edges, extend boundaries, and grips on bulged polylines; `web/e2e/curves.spec.ts` |
| DXF keeps bulges | `cad-dxf` | Fixtures written by an independent library, `test-data/dxf/valid/ezdxf-*-bulged-*.dxf`; export → import → export byte-identical |
| TRIM, EXTEND, OFFSET, FILLET, and CHAMFER cover the combinations in the matrix, without the M2 refusals for polylines and arcs | `cad-document/src/editing.rs`, `cad-geometry/src/{path,path_offset,path_corner,curve_fillet}.rs` | One test per supported combination; [`docs/modify-matrix.md`](../modify-matrix.md) lists them and what is still refused |
| BREAK, JOIN, and STRETCH exist | `commands/{break_,join,stretch}.rs` | `cad-commands/src/tests/{break_,join,stretch}.rs`, `web/e2e/{break,join,stretch}.spec.ts` |
| Perpendicular and tangent snaps work | `cad-commands/src/snap.rs`, `cad-geometry/src/construct.rs` | `cad-commands/src/tests/snapping.rs`; marker baselines `test-data/screenshots/snap-marker-{perpendicular,tangent}.png` |
| Every modify operation has a stated invariant under property-based stress, and a regression corpus | `crates/cad-geometry/tests/{stress,regression}.rs`, `test-data/geometry/` | The table in [`test-data/geometry/README.md`](../../test-data/geometry/README.md); six corpus files, one runner |

Constraints, as the epic set them:

| Constraint | How it was kept |
| --- | --- |
| Every fixed geometry bug adds a permanent regression test | Seven in this milestone, listed below |
| Tolerance classes stay centralized | No literal epsilons in the new code; one new function on the linear class, by ADR |
| An operation returns its result or a typed reason | `OffsetError`, `TrimError`, `ExtendError`, `CornerError`, `BreakError`, `JoinError`; STRETCH returns `None` for an object that would stop being valid, and the command counts them |
| Polyline offset starts with an experiment and an ADR | [ADR-0011](../adr/0011-polyline-offset.md) |

Test counts at this review: 998 Rust tests, 94 Vitest tests, 155 Playwright tests in Chromium (146 in Firefox; the nine visual baselines are compared in Chromium only). At the M2 review they were 716, 94, and 125.

## What was built

| Issue | What |
| --- | --- |
| #114–#116 | The bulge model; DXF bulges; `PLINE` `Arc` and `Line` |
| #117 | Perpendicular and Tangent snaps, and ranking tiers for snap candidates |
| #118 | Hover feedback: the object a pick would take, and for TRIM and EXTEND what the click would do |
| #119–#120 | TRIM and EXTEND with several edges at once, chosen as a selection set or all; polylines as targets |
| #121–#123 | OFFSET for arcs and the `Through` option; the polyline offset experiment; OFFSET for polylines |
| #124–#125 | FILLET between lines, arcs, and circles, with the picks choosing among up to eight solutions; FILLET and CHAMFER inside a polyline and on all its corners |
| #126–#128 | BREAK, JOIN, STRETCH |
| #129 | The corpus for every operation, the stress tests, `scripts/stress.sh` |

Beyond the list: a bare `@` at a point prompt means "the previous point again"; the toolbar no longer changes the size of the canvas as buttons are added.

## Decisions made in M3

| Item | Decision |
| --- | --- |
| Polyline offset (§22, experiment) | Slice the raw offset at the boundaries of the source's reach and keep what is at the distance — [ADR-0011](../adr/0011-polyline-offset.md). The outside of a corner is an arc: the true offset, not a mitre |
| The linear tolerance far from the origin (§21.1) | Applied at the magnitude of what it compares; intersections place a hit by where it is along the curve — [ADR-0012](../adr/0012-linear-tolerance-far-from-the-origin.md) |
| Arc segments | A bulge per vertex, the DXF convention; `Entity::primitives` stays the single way geometry is read |
| Snap ranking | Tiers: a snap point under the cursor, then a hovered construction (perpendicular, tangent), then a hovered centre, then Nearest |
| Choosing edges for TRIM and EXTEND | An ordinary selection set, or Enter for every object |
| FILLET with curves | Of the candidate arcs, the one that touches each object nearest its pick. A circle is left whole |
| CHAMFER | Lines only: a chamfer is a cut across a corner of two straight lines. Arcs are refused by name (recorded on #124) |
| JOIN | Decided by the whole selection: collinear lines become a line, bridging gaps; arcs of one circle that touch become an arc or the circle; otherwise end-to-end chains become polylines. A gap between arcs is not bridged |
| BREAK | The pick is the first point; a circle loses the part counter-clockwise from the first point to the second |
| STRETCH | The command asks for its window, corner by corner, because the window decides which ends move. Its corners are not snapped: `Command::takes_object_snap` |

## What testing found

Seven defects in geometry, all in code that already had unit tests and, in most cases, property tests, and one in the page layout. Each is fixed and has a regression test; the first two also have corpus lines.

| Found by | Defect | Fix |
| --- | --- | --- |
| The offset property test, by chance in a normal run | A polyline offset kept a stretch running across its source: `circle_circle` was measured from the larger circle, so against a nearly straight arc (radius 44,000) a hit landed micrometres off and was found from one side and not the other | Measure from the smaller circle |
| A million-case run | The same symptom at radius 8.8 million: good hits were rejected as "not on the arc" by re-measuring their distance from a centre millions of units away | A hit on the circle is checked against the sweep only |
| Stress, frames at 10⁷ | Intersections intermittently missing far from the origin: one line–circle crossing in ten at 10⁷, one in three at 10¹⁰ | ADR-0012 |
| Stress, frames at 10⁷ | A line broken at x ≈ 8.4 × 10⁶ would not JOIN again | ADR-0012 |
| Stress, near-tangent circles | Two circles touching within tolerance touched at a point several times the gap off the larger one | The touching point is taken on the smaller circle |
| Stress, near-parallel lines | A fillet between lines 10⁻⁵ rad apart sat 3 × 10⁻⁶ off tangent | The half angle is computed without subtracting nearly equal numbers |
| Unit tests of the bulge model, while writing the offset | A closed polyline of two vertices — a circle drawn as two arcs — lost its closing arc | `polyline_closes` |
| The first end-to-end run with eighteen toolbar buttons | The toolbar grew wider than the window and resized the canvas, moving every visual baseline | The shell is one column as wide as the window |

After the fixes: a million random polyline offsets, and 300,000 cases of every property test in `cad-geometry` and `cad-document`, pass.

## Measurements

Native x86-64, `opt-level = 3`.

| What | Result |
| --- | --- |
| Offset of a closed polyline, 10 / 102 / 1002 vertices | 22 µs / 0.31 ms / 9.8 ms (0.40 s at 1002 before pruning by bounding box) |
| Crossings found at 10⁷ / 10¹⁰ units from the origin, before → after ADR-0012 | Segments 99.9 % → 100 %, 94.8 % → 100 %; line through circle 89.3 % → 100 %, 64.4 % → 100 % |
| `scripts/stress.sh`, after compiling | 100,000 cases per property: 14 s. 300,000: about a minute |

**Not established**, still: frame time on a real GPU. Nothing in M3 changed the render path except polyline arc segments, which use the arc instances ADR-0009 measured.

## What M3 taught us

- **A property test with the default 256 cases is a smoke test.** The first offset bug appeared in a normal run only by luck; the second needed a million cases; the rest needed input *aimed* at the weak spots. Generators that spread a quantity evenly over ten orders of magnitude found in seconds what uniform random numbers never reach. `scripts/stress.sh` exists so that the long run is one command.
- **Re-measuring is where precision goes to die.** Four of the eight defects had the same shape: a point was computed on a curve, and then asked how far it was from that curve. The answer is noise, and near a tolerance noise decides. The rule that came out of it — place a computed point by where it is along the curve — is in `AGENTS.md`.
- **"Absolute tolerance" has a range.** 10⁻⁹ means something within about a million units of the origin. Nothing in the design said so, and georeferenced drawings live beyond it.
- **An independent implementation is worth writing once.** The winding-number offset in `benches/` disagreed with my expected values for two corpus cases and agreed with the implementation; the expected values were wrong. Without it the "fix" would have been to the code.
- **The corpus form pays for itself when a case is one line.** Thirty-odd trim, extend, and offset cases were written by hand and all but one matched first time; the one that did not was my arithmetic, and the property check on every case is what made that obvious.
- **Reading geometry through primitives held.** Bulged polylines reached snapping, intersection, trim edges, extend boundaries, selection, and rendering without a line of polyline-specific code in any of them — the thing the M2 review said M3 must preserve.
- **A selection is not always the right input.** STRETCH needs the window, not the objects in it; TRIM needs to know where an object was picked. Making the command own the question, rather than bending the selection set, kept both simple.
- **Layout is part of the test surface.** One more toolbar button moved the canvas and failed fifteen unrelated tests. The fix was to stop the shell depending on its content, not to re-record the baselines.

## Open items

Needs a person:

1. **How the new commands feel against classic CAD.** In particular: polyline offset rounds the outside of a corner where AutoCAD mitres by default; JOIN does not bridge a gap between arcs; STRETCH asks for a window instead of taking a selection, and ignores objects selected beforehand; BREAK on a circle removes counter-clockwise from the first point. Each is defensible and each is a guess about what a drafter expects.
2. Still open from M2: the hands-on assessment of its exit condition (#101); the arc grip behaviour against AutoCAD (ADR-0010); render measurements on real GPU hardware (#110); whether to report the R-tree library's bulk-load crash upstream (#111).

Carried forward as issues: drafting far from the origin end to end, including the renderer's single origin (#151); Firefox in CI and any WebKit run (#109); unit suffixes and drawing units (#113, now in the M4 order).

Known limitations, accepted for now:

- TRIM and EXTEND use real crossings only, not implied (extended) edges; no fence selection of objects to trim.
- FILLET and CHAMFER do not join a polyline to another object, and skip polyline corners next to an arc segment.
- Polyline arcs flatter than a radius of about 10⁷ are evaluated through a centre that far away and are known only to the spacing of numbers there.
- The modify operations other than intersection and JOIN still take the absolute tolerance; they hold their invariants to 10⁷ and move to the magnitude-aware one as a need is shown (ADR-0012).
- No grip menu (Lengthen, Radius, add or remove vertex); no dynamic input near the cursor.

## M4, rewritten

§32 lists candidate work for drawing organization. With M3 done, the order is ([`docs/roadmap.md`](../roadmap.md) has the reasoning):

1. **Entity properties and the generalized change model** (#131): entities carry a layer, colour, linetype, and lineweight; tables change through the one history. Everything else stands on it.
2. **Layers** (#132), the **`LAYER` command** with text input for names (#133), and **drawing units** (#113).
3. **Colours across the render boundary** (#134), with an ADR, then **object colour** (#135) and **lineweights** (#136).
4. **Linetypes**: an experiment and ADR on how dashes are drawn (#137), then the table and commands (#138).
5. **DXF** for all of the above (#139); the **Layers panel** (#140) and the **Properties panel** (#141).
6. **Text**: an experiment and ADR on rendering and metrics (#142), then `TEXT` (#143), `MTEXT` (#144), and their DXF (#145).
7. **Blocks**: the model and ADR, with redefinition as one change (#146); rendering, picking, and snapping (#147); `BLOCK`, `INSERT`, `EXPLODE` (#148); DXF (#149).
8. **Exit** (#150).
