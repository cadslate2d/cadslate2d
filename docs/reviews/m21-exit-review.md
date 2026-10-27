# M21 exit review — Constraints and Constrained Generation

**Date:** 2026-10-25  
**Scope:** persistent geometric and dimensional constraints with a 2D solver; part generators and placements as commands, constrained; and the AI's second level, generation within constraints. Asked for by the user on 2026-10-19 ([roadmap](../roadmap.md), epic #264, [ADR-0038](../adr/0038-constraints.md)).  
**Outcome:** Every item is implemented, measured, and tested. Nothing in M21 is waiting on a person. Production is still unbuilt, as at M20's exit.

What M21 delivered:
- **Constraints are rows of the drawing**, changed in the same transactions and history as everything else:
  - geometric: coincident, collinear, concentric, fix, parallel, perpendicular, horizontal, vertical, tangent, symmetric, equal;
  - dimensional: distance, angle, radius, diameter — named parameters (`d1`, `ang1`, `rad1`, `dia1`) that drive the geometry.
- **Every edit is settled by the solver** (`cad_geometry::constraint`, Levenberg–Marquardt over sparse Jacobians, deterministic). The commit holds what the edit set, and the rest of each constrained part follows in the same undo. An edit that cannot be settled is refused, and the session says which constraints it would break.
- **The commands are AutoCAD's:**
  - `GEOMCONSTRAINT` and the `GC…` family, `DIMCONSTRAINT` and the `DC…` family, `-PARAMETERS`, and `DELCONSTRAINT`;
  - constraint bars and constraint dimensions on the canvas, hidden from View › Constraints or by `CONSTRAINTBAR` and `DCDISPLAY`.
- **DXF round trip** as CadSlate XRECORDs. AutoCAD's own associative network is read and reported, not lost silently.
- **Parts, made whole and constrained:** `PLATE`, `SLOT`, `HOLEPATTERN`, `BOLTCIRCLE`, `FLANGE`.
- **Placements that keep their relation:**
  - CIRCLE's Tan, Tan, Radius and Tan, Tan, Tan, `TANARC`, `CONCENTRIC`;
  - `ARRAYPATH`, which keeps circles on their path and at one size;
  - `ARRAYGRID`, a grid every so far inside a boundary.
- **The AI's second level.** The intent IR (version 4) reaches all of the above. The context the user agrees to send lists the parameters of the selected objects. "Make this plate 20 mm wider without moving the holes relative to the edges" is answered by setting its width parameter, and the holes follow.

## The scenario

`web/e2e/m21-scenario.spec.ts` runs in Chromium and Firefox against the tests' stand-in service, which answers from the corpus's recordings of the real model:

1. **Constrain a sketch:** an L of two lines, their corner coincident, one level, one plumb, its start fixed. Moving the plumb line 10 to the right, the level line's end follows and its fixed start stays.
2. **A plate by a request:** "Make a 120 × 80 mm plate … four Ø8 holes 10 mm in from the corners" is proposed as 8 objects and accepted.
3. **A flange by command:** `FLANGE`, round, 150 outside, a 50 bore, eight Ø12 holes on 110.
4. **Both edited:**
   - The plate by a request, "Make this plate 20 mm wider…". The consent lists `d1 = 120: horizontal distance`; once accepted, the plate is 140 wide, and every hole is still 10 from its nearest side.
   - The flange's outside diameter at the command line: its parameter, found by `-PARAMETERS ?`, is set to 180.
5. **Saved and reopened:**
   - `DXFOUT`, then `OPEN` on the file: the same parameters are listed.
   - Setting `d1` to 160 drives the plate again, and its holes follow.
   - Moving the sketch's fixed line is refused: "The edit would break the fix constraint."

**The plate comes before the flange,** which reorders the issue's steps 2 and 3. A parameter's name belongs to the drawing. The stand-in answers "wider" with the recorded model's `d1`, which names the plate's width only when the plate is the first part made. The live model reads the name from the context the user sends.

## Done when (epic #264)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| Constraints are kept in the drawing, edited by GEOMCONSTRAINT and DIMCONSTRAINT, shown by constraint bars, and re-solved on every edit | #265–#270 | [ADR-0038](../adr/0038-constraints.md), accepted 2026-10-25 after the user's review. The solver's tests in `crates/cad-geometry/src/constraint.rs` (proptests: determinism, satisfied after success, original values on failure). `drawing/settle.rs` tests and the history property test with constraint operations. `crates/cad-commands/src/tests/geomconstraint.rs`, `dimconstraint.rs`. `constrained_edits.rs`: MOVE, ROTATE, SCALE, STRETCH, grips, FILLET, OFFSET, and ERASE on a constrained plate, and the property test that every constraint holds after any accepted edit. The render-boundary constraint test, and the baselines `constraint-bars.png` and `constraint-bars-hidden.png` |
| They round-trip through DXF as CadSlate XRECORDs | #271 | `crates/cad-dxf/tests/fixtures.rs`; the golden `test-data/dxf/valid/cadslate-constraints.dxf`; AutoCAD 2010's network read and reported from `libdxf-autocad2010-constraints.dxf`; rows in `docs/dxf-matrix.md` and `docs/dwg-matrix.md`; this scenario's step 5 |
| Plates and hole patterns, bolt circles and flanges, arrays and grids, tangent and concentric placement exist as commands, constrained, and are reachable from the AI | #272, #273, #274 | `cad_document::generate`, with its tests. `crates/cad-commands/src/tests/generators.rs` and `placement.rs`: each part and placement, its constraints holding, one undo, and the same drawing by command and by intent. The tangency corpus `test-data/geometry/tangents.txt`, 17 cases. The intent schema `test-data/intent/schema-v4.json`, and `crates/cad-commands/src/tests/intent.rs`: a plate widened by its parameter, refusals with the solver's reason |
| The AI resolves intent to generator and constraint operations | #274 | `test-data/ai/l2/`: 48 cases in English and Portuguese, replayed in `make check`. `web/e2e/ai-panel.spec.ts` › *a plate made by prompt, then widened by prompt with its holes following* |
| A scenario and a review | #275 | `web/e2e/m21-scenario.spec.ts`; this review |

## Measurements

**The solver** (`benches/benches/constraints.rs`, native, release, this machine, 2026-10-25). The benchmark asserts each outcome, not only its time: a local drag settles locally, a long-range edit propagates down the whole chain, and an over-constrained drag is refused.

| Constraints | Long-range, solved | Local drag | Over-constrained, refused | Separate parts, one changed | Assess one | Degrees of freedom |
| --- | --- | --- | --- | --- | --- | --- |
| 11 | 2.7 µs | 1.4 µs | 10.8 µs | 2.7 µs | 16 µs | 7.6 µs |
| 99 | 84 µs | 6.2 µs | 272 µs | 9.2 µs | 297 µs | 175 µs |
| 1,001 | 6.4 ms | 47 µs | 20.2 ms | 127 µs | 20.1 ms | 15.2 ms |

These match ADR-0038's measurements from the start of the milestone. Settling a whole drawing's worth of parts stays well within a frame, because only the parts an edit touches are solved.

**The AI's second level.** The L2 corpus was asked of the model through the staging service at intent version 4, and scored by `ai-eval`:

| Level | Language | Recorded (committed) | Live, 2026-10-25 (run 40244124789) |
| --- | --- | --- | --- |
| l2 | en | 24/24 | 24/24 |
| l2 | pt | 24/24 | 23/24 |
| l1 | en | 35/35 | 34/35 |
| l1 | pt | 35/35 | 35/35 |

The live L2 miss was the scorer's, not the model's. A correct Portuguese explanation was judged Spanish ("que" twice outweighed "as" and "e"). The Portuguese word list is fixed, with that sentence as a test (a0844a7). The L1 miss (`grid-en`) is one stray answer in 35, within the 5-point tolerance.

The nightly job runs every level under `test-data/ai/`, so L2 is asked every night from now on. The run above was started by hand to have a first night before this review.

**Cost and latency of L2:**
- **Cost:** 48 requests spent 270,207 input tokens, 255,915 of them cached, and 8,719 output: **$0.0084**, about **$0.00017 per request**. That is more than L1's $0.00010, because L2's contexts carry parameters and its instructions are longer.
- **Latency:** p50 2.23 s and p90 3.76 s, from 1.44 s to 7.24 s, measured by the recorder from staging.

**The suite:**
- 1,956 Rust tests;
- 192 web unit tests;
- 554 end-to-end runs passing in Chromium and Firefox;
- the service: 28 tests.

**Size:** the engine is 1.23 MB gzipped (M20: 1.07 MB) and the main script 105.7 KB gzipped (M20: 103.6 KB). The engine's growth is the solver, the constraint table and its settling, the generators, the placements, and the DXF records.

## Commands added (registry sweep)

| Area | Commands |
| --- | --- |
| Geometric constraints | `GEOMCONSTRAINT` (`GCON`), `GCHORIZONTAL`, `GCVERTICAL`, `GCPERPENDICULAR`, `GCPARALLEL`, `GCTANGENT`, `GCCOINCIDENT`, `GCCONCENTRIC`, `GCCOLLINEAR`, `GCSYMMETRIC`, `GCEQUAL`, `GCFIX`, `DELCONSTRAINT` (`DELCON`) |
| Dimensional constraints | `DIMCONSTRAINT` (`DCON`), `DCLINEAR`, `DCHORIZONTAL`, `DCVERTICAL`, `DCALIGNED`, `DCANGULAR`, `DCRADIUS`, `DCDIAMETER`, `-PARAMETERS` (`PARAMETERS`, `PAR`, `-PAR`) |
| Display | `CONSTRAINTBAR` (`CBAR`), `DCDISPLAY` |
| Parts | `PLATE`, `SLOT`, `HOLEPATTERN`, `BOLTCIRCLE`, `FLANGE` |
| Placement | CIRCLE `Ttr` and `TTT`, `TANARC`, `CONCENTRIC`, `ARRAYPATH`, `ARRAYGRID` |

Every one is reachable from the menus: Parametric › Geometric Constraints, Dimensional Constraints, Parts, Placement, Delete Constraints, Parameters; Draw › Circle › Tan, Tan, Radius and Tan, Tan, Tan; View › Constraints. `menus.test.ts` runs every macro without a pause.

## Decisions

- **The solver is our own** (ADR-0038). The candidate libraries were weighed and rejected: their licences, their sizes, or their maturity. The solver is Levenberg–Marquardt with Jacobi-preconditioned conjugate gradients over each connected component.
  - It is deterministic: there are no hash maps on the solving path.
  - A failure returns every value as it was.
  - Redundancy is judged on the whole constraint, at a generic configuration.
- **The edit is held, and the rest follows.** What a transaction set does not move. Moving it would break the constraints that tie it, so such an edit is refused rather than quietly reshaped.
- **What placement makes is constrained.** This differs from AutoCAD:
  - CIRCLE's Tan, Tan, Radius adds tangent constraints, where AutoCAD (with `CONSTRAINTINFER` off) adds none. A tangent circle follows its lines.
  - The picked objects are held when it is made. A free object it touches may still give a little on a later edit, when that is the least movement.
- **Parameters go to the AI only with consent.** They are part of the context the user is shown and agrees to, listed by name, value, what they measure, and the IDs of what they measure.
- **The IR grows by versions.** Version 2 added the parts, 3 the placements, and 4 the constraints and parameters. Every earlier version is still read (`cad_intent::READS`), so a stored proposal of version 1 still runs.
- **L1 was recorded again** under the new instructions. The model now answers L1's plate and bolt circle with the parts, and those two cases accept the parts as well as their old answers.

## What testing found

- **A tangent circle's touching point was taken on the wrong side** for a circle enclosing the curve it touches, so Tan, Tan, Tan chose the wrong one of the eight Apollonius circles. Found by the tangency corpus and fixed in `Curve::touching_point` (#273).
- **A circle between two parallel lines** its diameter apart has a whole family of answers, and Tan, Tan, Radius found none. It now places the circle level with the picks (`between_parallels`) (#273).
- **A typed pick could not name a point.** A coincident constraint or a fix asks for an end or a centre; the IR's pick clicked the object's nearest point, which for a circle's centre is on the circle. Where a command asks for a point, a pick now types the point it is near (#274).
- **The scorer judged every reply with a `Ø` to be in no language** (#274), and once judged Portuguese to be Spanish. Both are fixed, with tests.
- **The model resized a part's holes one by one** after setting their diameter parameter, unaware the others were held equal to it; the edit was refused. The service's instructions now say a part's holes share one parameter (#274).
- **Two end-to-end specs still expected CIRCLE's old prompt,** `[3P/2P]`, after #273 added `Ttr` and `TTT`: #273 was pushed with the check passing but without the e2e suite. They were updated here. From now on, a command whose prompt changes runs its e2e specs before it is pushed.

## What needs a person

Nothing new. M20's list stands:
- production (its public address, and the Free allowance);
- the provider's zero-data-retention application.

The nightly key has been set since 2026-10-25.

## What follows

**M22 — Floor-Plan Generator** (epic #276) is next. It is a deterministic layout solver from a terrain and room constraints to walls, doors, and labels: `FLOORPLAN` as a command with alternatives, and briefs and revisions through the AI. Its level-3 corpus goes in `test-data/ai/l3/`, which the nightly job picks up.
