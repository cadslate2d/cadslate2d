# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed. All 18 acceptance criteria are covered by automated tests ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | In progress. |
| M2 — OSNAP, Grips, and Basic Modify | Planned in detail. |
| M3–M7 | PROVISIONAL placeholders, to be broken down at the preceding exit review. |
| M8 | FUTURE. Not planned against. |

## M1, revised after M0

M0 was built so that M1 is mostly *adding instances* of things that already exist once. The revisions below reflect that.

### Already in place from M0

- A command registry with aliases and one `invoke` entry point for typed input, shortcuts, and UI actions. The M1 registry issue shrinks to adding the remaining aliases and any registration ergonomics that prove necessary.
- Input history with Up/Down recall, and incremental command log.
- Parser diagnostics shown in the command line, with the command staying active.
- A semantic overlay description that commands fill in. The M1 transient-preview framework generalises this to edits of existing entities; it does not start from zero.
- `View::fit`, used when opening a file.

### Order of work

1. **Curve-rendering spike** → ADR. Nothing curved is drawn before this is decided. It must state how its choice fits ADR-0003 (pan and zoom never re-upload committed geometry) and what crosses the render boundary (ADR-0008 covers straight segments only).
2. **Keyword options and scalar input** (`[Close/Undo]`, distances, angles, direct distance). These extend `ExpectedInput`, which is matched exhaustively on purpose.
3. **Undo granularity.** Decide whether one UNDO reverts one segment or one command invocation, and implement it as a general history mechanism before more multi-step commands are written.
4. **Circle and arc entities**, then their commands. Adding a variant to `Entity` will fail compilation in rendering, picking, the test harness, and DXF; that is the checklist.
5. **Polyline entity and joins**, then `PLINE` and `RECTANG`.
6. **Selection sets** and the generalised transient-edit framework, then `ERASE`, `MOVE`, `COPY`.
7. **DXF for the new entities.** R12 has no `LWPOLYLINE`; polylines are written as `POLYLINE`/`VERTEX` (ADR-0005).
8. **Status toggles framework and toolbar**, which are independent of the above and can be done at any point.

### Carried over from M0

- Run the end-to-end suite in Firefox and WebKit.
- Measure GPU upload cost on real hardware.
- A safe bulk loader for the spatial index, to speed up opening large files.

## Standing rules

- A PROVISIONAL or EXPERIMENT item is resolved by an ADR with evidence, not by default.
- Every milestone ends with a review like M0's: criteria linked to tests, lessons recorded, the next milestone's issues adjusted.
- Performance targets are checked with the generated drawings in `benches/` at 10k, 50k, and 100k entities.
