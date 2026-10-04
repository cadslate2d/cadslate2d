# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | Complete and closed ([review](reviews/m1-exit-review.md)). |
| M2 — OSNAP, Grips, and Basic Modify | Every deliverable is implemented ([review](reviews/m2-exit-review.md)). Open only for the hands-on assessment of its exit condition, which needs a person. |
| M3 — Robust Modify Geometry | Next. Rewritten after M2 and broken into issues. |
| M4–M7 | PROVISIONAL placeholders, to be broken down at the preceding exit review. |
| M8 | FUTURE. Not planned against. |

## M3, as rewritten after M2

DESIGN §31 lists candidate work and says the exact scope should be rewritten once real M2 geometry exists. It exists; this is the rewrite.

M2 left M3 these pieces: intersection primitives with a regression corpus; entities that decompose into primitives, which snapping, trimming, and extending already work through; a `Transform` for every entity kind; modify operations that return typed errors; a published matrix of what each command supports; and object picking as a command input.

### Order of work

1. **The polyline bulge model.** Arc segments in polylines, as a bulge per vertex. Polyline offset, polyline fillets, polyline trim, JOIN, and faithful DXF polylines all stand on it, so it comes first. It must keep `Entity::primitives` as the single way geometry is read, so snapping, intersections, and trimming keep working unchanged. Then DXF bulges, and arc segments in `PLINE`.
2. **Perpendicular and tangent snaps.** §18 names them among the initial modes; §30 left them out of M2. They need a base point and exact tangent and foot-of-perpendicular constructions, which fit here.
3. **Hover feedback for object picks**, the usability gap M2 exposed, then **TRIM and EXTEND** with several edges at once and with polylines as targets.
4. **OFFSET.** Arcs are simple and come first. Polylines are the hard case in this milestone — pieces vanish, corners need joining, results self-intersect — so they start with an experiment and an ADR, as curve rendering did.
5. **FILLET and CHAMFER** with arcs and circles, where there are several solutions and the pick points choose; then on polylines.
6. **BREAK, JOIN, STRETCH.**
7. **Exit:** scenario, updated matrices, and the M4 breakdown.

The regression corpus and property-based stress tests are not a step of their own at the end: each item adds its cases as it lands, and one issue covers the stress-test harness.

### Not in M3

- A grip menu (Lengthen, Radius, add or remove vertex). Useful, but not robust-geometry work; a candidate for M4 alongside properties.
- Dynamic input near the cursor (§13.5).
- Splines and ellipses.

### Needs a person

- The hands-on assessment of the M2 exit condition.
- Confirming the arc grip behaviour against AutoCAD (ADR-0010).
- Measuring rendering on real GPU hardware.
- Deciding whether to report the R-tree bulk-load crash upstream.

## Standing rules

- A PROVISIONAL or EXPERIMENT item is resolved by an ADR with evidence, not by default.
- Every milestone ends with a review like M0's: criteria linked to tests, lessons recorded, the next milestone's issues adjusted.
- Performance targets are checked with the generated drawings in `benches/` at 10k, 50k, and 100k entities.
