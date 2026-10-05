# ADR-0012: The linear tolerance far from the origin

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (refines DESIGN §21.1; found by the stress tests of the M3 plan)  
**Design refs:** §21, §23.4, §31, §41

## Context

DESIGN §21.1 asks for tolerance classes with centralized values. The geometric linear tolerance is `LinearTolerance::DEFAULT`, 10⁻⁹ drawing units, and it is absolute: two points are the same point when they are within 10⁻⁹ of each other, wherever they are.

Document coordinates are `f64` (§23.4). An `f64` has about sixteen significant digits, so the distance between one representable number and the next grows with the number:

| Coordinate | Spacing of neighbouring numbers |
| --- | --- |
| 1 | 2 × 10⁻¹⁶ |
| 10⁶ | 1 × 10⁻¹⁰ |
| 10⁷ | 2 × 10⁻⁹ |
| 10¹⁰ | 2 × 10⁻⁶ |

Beyond a few million units the numbers themselves are coarser than the tolerance. A point computed to be on a line there is rounded to the nearest representable point, which is generally *not* within 10⁻⁹ of the line. Any test of the form "is this computed point within tolerance of that curve?" then fails for reasons that have nothing to do with the geometry.

Such coordinates are ordinary. A site plan in metres on a national grid has northings of several million; the same plan in millimetres has coordinates of 10⁹ to 10¹⁰.

## Evidence

The stress tests added for the M3 plan (`crates/cad-geometry/tests/stress.rs`) place their cases up to 10⁷ units from the origin. Two failed:

- a line broken in two at x ≈ 8.4 × 10⁶ would not JOIN again: the shared end, as computed, was 2 × 10⁻⁹ off the line, so the halves were "not collinear";
- intersections were intermittently missing.

The second was then measured directly: 1744 pairs of clearly crossing segments, and the same segments through a circle of radius 30 about their crossing, placed at increasing distances from the origin. Before the change, every hit was confirmed by measuring its distance from the segment against the absolute tolerance:

| Distance from the origin | Crossing segments found | Line through circle: both hits found |
| --- | --- | --- |
| 0 – 10⁶ | 100 % | 100 % |
| 10⁷ | 99.9 % | 89.3 % |
| 10⁸ | 99.0 % | 73.9 % |
| 10¹⁰ | 94.8 % | 64.4 % |
| 10¹³ | 95.2 % | 65.7 % |

A missed intersection is not a small error: TRIM reports that the objects do not meet, EXTEND finds no boundary, the Intersection snap is not offered.

After the change, all 1744 of both kinds are found at every distance out to 10¹³.

## Decision

### 1. A tolerance is applied at the magnitude of what it compares

`cad-core` gains two functions on `LinearTolerance`:

```rust
pub fn at_magnitude(self, magnitude: f64) -> Self
pub fn around(self, points: &[Point2]) -> Self      // at the largest coordinate among them
```

`at_magnitude` returns the tolerance itself near the origin, and eight spacings of the floating-point numbers at `magnitude` where that is wider. It is never narrower than the tolerance it is called on. With the default tolerance the widening begins at about 5.6 × 10⁵ units; at 10⁷ the tolerance is 1.8 × 10⁻⁸, at 10¹⁰ it is 1.8 × 10⁻⁵.

This is still one centralized value with one meaning — "the same, as nearly as can be told" — rather than a second epsilon. Near the origin nothing changes.

### 2. Intersections place a hit by where it is along the curve

A hit is computed on the line or circle it belongs to. Whether it is on the bounded segment or arc is then a question of *where along*: a parameter between 0 and 1, or an angle within the sweep, with the tolerance allowed at each end. Its distance from the curve is not measured again. `segment_segment` solves for the parameter along both segments at once; hits on a circle are tested against an arc's sweep.

This is what removes the missed intersections; the wider tolerance only governs the ends.

### 3. Where it is applied now

- `cad-geometry::intersect`: every intersection, as above.
- `cad-document::join`: collinearity, co-circularity, and matching of ends use the tolerance around the entities being joined.

The other modify operations — offset, trim, extend, fillet, chamfer, break, stretch — take their tolerance as a parameter and are still handed the default. The stress tests hold each of them to its invariant in frames up to 10⁷ units out, to the precision of the numbers there, and they pass. They move to `around` as a need is shown, not in one sweep.

## Alternatives considered

- **Leave the tolerance absolute and document a coordinate limit of about 10⁶.** Rejected: it excludes ordinary georeferenced drawings, and the failure is silent — a trim that does not happen.
- **A relative tolerance everywhere** (a fraction of the coordinates). Rejected: near the origin it shrinks towards nothing, and tolerances stop meaning a distance a drafter could name. The decision keeps the absolute value wherever the numbers can honour it.
- **Translate every computation to a local origin.** Sound, and what the renderer does (ADR-0003). For geometry it would touch every function for a gain the two changes above already deliver; it also does nothing for results that must be stored back in document coordinates. Kept in reserve.
- **Raise the default tolerance.** Rejected: it would weaken every comparison near the origin to help the ones far from it.

## Consequences

- Drawings far from the origin trim, extend, snap, and join as they do near it. Precision still falls with distance — at 10¹⁰ units nothing is known more finely than a few micrometres — but results degrade in proportion instead of failing.
- Two more defects of the same family were fixed alongside, each with a regression test: the touching point of two circles that touch within tolerance is taken on the smaller circle (the foot of the radical line overshot by several times the gap), and a fillet between lines a hair from parallel takes its half angle without subtracting nearly equal numbers (the arc sat 3 × 10⁻⁶ off tangent at 10⁻⁵ rad).
- Polyline arcs that are nearly straight have centres far from the drawing even when the drawing is near the origin. Intersections with them are placed by sweep, not by distance from that far centre, for the same reason. An arc whose radius exceeds about 10⁷ is still evaluated through its centre and is known only to the spacing of numbers at that radius; the stress tests cover radii to 10⁷.
- Code that compares lengths or points must choose: the default tolerance for quantities near the origin, `around` the points involved otherwise. `AGENTS.md` says so.

## Revisit when

- An operation other than intersection and JOIN is shown to fail far from the origin: move it to `around`, with the failing case as its regression test.
- Drawings need coordinates beyond 10¹⁰, or polyline arcs flatter than radius 10⁷: evaluate arcs from chord and sagitta, or compute in a local frame.
