# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | Complete ([review](reviews/m1-exit-review.md)). Two carried-over items need a person. |
| M2 — OSNAP, Grips, and Basic Modify | Next. Planned in detail. |
| M3–M7 | PROVISIONAL placeholders, to be broken down at the preceding exit review. |
| M8 | FUTURE. Not planned against. |

## M2, as adjusted after M1

M1 left M2 several pieces already in place: toggle state with status-bar buttons and function keys, a transient-edit framework that names its source entities, box-crossing tests and segment–circle root finding in `cad-geometry`, and an overlay that any new marker can join.

### Order of work

1. **Intersection primitives and robust predicates.** Everything else in M2 — intersection snap, trim, extend, fillet, chamfer — stands on these. Extend `cad-geometry` from boolean tests to intersection points, with a regression corpus under `test-data/geometry/`.
2. **Snap engine**, then the single-entity snaps, intersection snap, and the transient-edit exclusion rule (§18.2), which uses `Command::transient_sources`.
3. **Snap markers and the drafting aids** behind the existing toggles: OSNAP, ORTHO, GRID, POLAR.
4. **A general entity transform** (rotation, uniform scale, mirror) alongside `Entity::translated`, then ROTATE, SCALE, MIRROR.
5. **Grips:** the model and display, the grip-edit session on the transient framework, then per-entity grips.
6. **Narrow modify geometry:** OFFSET for lines and circles, simple TRIM and EXTEND, line–line FILLET and CHAMFER, each with a published matrix of supported inputs.
7. **Exit:** visual regression for markers and grips, a drafting scenario, a second look at the spatial index with snapping workloads (ADR-0007), and the M3 breakdown.

### Open scope question

§18 lists perpendicular and tangent snaps among the initial modes; §30 does not include them in M2. The M1 review recommends leaving M2 as §30 has it and taking them into M3.

### Carried over

- Measure rendering on real GPU hardware (needs a person).
- Decide whether to report the R-tree bulk-load crash upstream (maintainer's call).

## Standing rules

- A PROVISIONAL or EXPERIMENT item is resolved by an ADR with evidence, not by default.
- Every milestone ends with a review like M0's: criteria linked to tests, lessons recorded, the next milestone's issues adjusted.
- Performance targets are checked with the generated drawings in `benches/` at 10k, 50k, and 100k entities.
