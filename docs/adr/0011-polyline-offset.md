# ADR-0011: Polyline offset — slice the raw offset and keep what is at the distance

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves the experiment DESIGN §22 and the M3 plan call for)  
**Design refs:** §21, §22, §31, §42

## Context

DESIGN §22 names offset among the commands that need "dedicated robust-geometry work". For a line, a circle, or an arc the offset is one more of the same. For a polyline it is not:

- segments shorter than the corner they sit in **disappear**;
- at the outside of a corner the two neighbours' offsets **do not meet**, and at the inside they **overlap**;
- arcs offset towards their centre **collapse** when the distance reaches their radius;
- a shape with a narrow part **falls apart** into several;
- the naive result **crosses itself**.

Getting any one of these wrong produces geometry that looks plausible and is silently off by the offset distance. The M3 plan therefore started with an experiment.

## The two candidates

Both start from the same **raw offset**: every segment offset on its own (a line to a parallel line, an arc to a concentric arc), and at every vertex an arc about that vertex joining the two neighbours.

**A. Slice and filter.** Cut every raw piece wherever it crosses the boundary of the region within the offset distance of a source segment (the segment's two parallels and the circles about its ends). Between cuts a piece is right throughout or wrong throughout. Keep it if its middle is at the offset distance from the source as a whole. Stitch what is left.

**B. Winding number.** Cut the raw offset, as a closed curve, at its self-crossings. Keep a piece if the raw curve winds once round the region on its inner side and not at all round the region on its outer side.

A is in `cad-geometry/src/path_offset.rs`. B is experiment code in `benches/src/winding_offset.rs`, kept so the comparison can be re-run.

## Evidence

### The corpus

`test-data/geometry/polyline-offsets.txt` holds 22 cases: corners inside and out, a reflex corner, a notch narrower than the offset, a shape with a waist, a slot, a fillet smaller than the offset, tangent arcs, a circle drawn as two arcs, shapes that vanish, a self-crossing path, a spike, and one regression. For 18 of them the expected length follows from the geometry and is written out in the file; the other four are recorded.

`cargo test -p cad-benchmarks --test offset_comparison -- --nocapture` prints the comparison:

| | Slice and filter (A) | Winding number (B) |
| --- | --- | --- |
| Closed cases (12) | all 12 match the expected lengths | all 12 match A, to 10⁻¹⁴ |
| Open cases (10) | all 10 match | not applicable: an open path has no inside to wind round |
| Self-crossing input | handled (the offset breaks where the path crosses) | undefined |

Two independent implementations agreeing on every closed case — including the less obvious ones, such as the dimple a vanished corridor leaves in a room's offset — is the main evidence that A is right where B applies.

### The property

A's construction gives a property that can be tested without knowing the answer:

> Every point of the result is the offset distance from the source.

`every_point_of_an_offset_is_at_the_offset_distance` checks it on random paths — three to seven vertices, straight and arc segments mixed, open and closed, self-crossing allowed, either side. It holds over 12,000 generated paths. The corpus runner checks it on every case.

A second property guards against the opposite failure, throwing away something valid: a convex polygon offset outwards must come back as one closed path, longer by exactly one full turn of the offset radius.

### What the experiment found

1. **A crossing that intersection missed.** On an arc so flat that its radius is 100,000, a crossing was not found, and a stretch that ran 22 units off was kept. Exact reasoning said the middle of a stretch decides for the whole stretch; rounding said otherwise. A now probes each stretch at seven points and, where the answer changes between two probes, bisects to find where. The failing case is a named regression test and a line in the corpus.
2. **A bug in the polyline model itself.** A circle drawn as two semicircles is a closed polyline of two vertices. The rule "two vertices closed is one segment, not one doubled" — written for straight polylines — dropped its second arc. Half of every such circle was missing: in the offset, and equally on screen and on DXF import. Fixed in `polyline_closes`, with regression tests at three levels.
3. **Two of my own expectations were wrong, and A was right.** Over a notch narrower than the offset, the offset is not a straight line: it dips, because the nearest source points are the notch's two corners. The same happens where a corridor vanishes. B agrees.

### Cost

`cargo bench -p cad-benchmarks --bench offset`, a closed comb shape, native, `opt-level = 3`:

| Vertices | A, every piece against every segment | A, with bounding-box pruning | B (experiment code) |
| --- | --- | --- | --- |
| 10 | 49 µs | 22 µs | 3.3 ms |
| 102 | 4.5 ms | 0.31 ms | 0.42 s |
| 1002 | 0.40 s | 9.8 ms | 40 s |

Unpruned, A is quadratic. Pruning — a raw piece is only compared with the source segments whose reach it enters — changes nothing in the result and takes a thousand vertices from 0.4 s to 10 ms. B's numbers are for unoptimised code and say little beyond "not obviously cheaper".

## Decision

1. **Use A**, with probing and bounding-box pruning. It handles open and closed paths alike, tolerates self-crossing input, and its correctness rests on a property the tests check directly.
2. **The result is the true offset: the outside of a corner is an arc.** Every point of the result is at the offset distance; a mitred corner would be farther away than that.
3. **An offset can be several polylines, or none.** The command creates them all; when nothing is left it says so.
4. **Offsetting is done on `cad_geometry::Path`**, like trimming and extending, and reached through `Entity`.

## What it does not handle

- **Mitred corners.** AutoCAD by default extends the neighbours of an outside corner to a sharp point (`OFFSETGAPTYPE` 0); it offers arcs as an option. CadSlate2D does the arcs only. A drafter used to the default will notice. This is a product choice more than a geometry one, and is worth revisiting with user feedback.
- **The exact moment of collapse.** Offsetting a 60-wide rectangle inwards by exactly 30 leaves, in principle, a line. The result there is unspecified: nothing, or a degenerate sliver. Just short of it and just past it are right.
- **A wrong stretch narrower than the gap between two probes**, on a piece whose crossing intersection also missed. Probing reduces the exposure; it does not remove it.
- **Segments that lie exactly on top of each other** (a path that doubles back along itself). The property still holds for what is returned, but pieces may be duplicated.
- **Very large polylines.** Pruning tests every raw piece against every segment's box, and stitching compares ends pairwise. Both are quadratic with a small constant: fine at a thousand vertices, not measured beyond.

## Alternatives considered

- **B, winding number.** Rejected as the method: closed, non-self-crossing shapes only, so open polylines would need a second algorithm anyway. Kept as an independent check.
- **Joining neighbours locally and stopping there** (trim where they overlap, arc where they do not, no global step). Rejected without building it: it is step 1 of A, and every "disappearing segment" and "falls apart" case in the corpus needs the global step.
- **An external library.** None was evaluated. The method is about 300 lines on top of intersection primitives that already exist and are tested, and a dependency in the geometry core would need its own justification (§8).

## Consequences

- `Path` gains `offset`; `PathSpan`, `raw_offset`, and `stitch_spans` are public so the experiment can share them.
- OFFSET for polylines (the next issue) is a thin layer: `Entity::offset` may return several entities.
- The probing step costs seven distance evaluations per stretch. It is the price of not trusting intersection on badly conditioned arcs.
- The corpus is the place for every future offset bug: one line each.

## Revisit when

- Users ask for mitred outside corners.
- A polyline large enough to make the quadratic steps visible turns up in practice.
- A wrong stretch gets past probing: add it to the corpus first, then decide whether probing needs to be adaptive.
