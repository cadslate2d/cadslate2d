# Geometry regression corpus

Plain-text cases for the geometry underneath the modify commands, one case per line. The format of each file is described at its top.

| File | Cases | Property checked on every case |
| --- | --- | --- |
| `intersections.txt` | Two primitives and the points where they meet | The answer is the same whichever is named first |
| `offsets.txt` | A line, circle, or arc, a distance, and a side | The result is the same kind of thing at that distance: a parallel line of the same length, a concentric circle or arc |
| `polyline-offsets.txt` | A polyline and a signed distance; the number and total length of the paths that result | Every point of the result is the offset distance from the source ([ADR-0011](../../docs/adr/0011-polyline-offset.md)) |
| `trims.txt` | An object, its cutters, and a pick; the pieces kept | Kept and removed add up to the object; the removed part is the one nearest the pick |
| `extends.txt` | An object, its boundaries, and a pick; the extended object | The other end has not moved; the result passes through the old end; the new end is on a boundary |
| `fillets.txt` | A fillet radius or two chamfer distances, two picked objects; both objects as they become and the joint | The joint ends on both objects; a fillet arc has the radius asked for and is tangent to both |

**Rule (DESIGN §21.3):** every fixed geometry bug adds a permanent case here, in the same commit as the fix — or, when the bug is too fine for values typed to six places, a regression test beside the code.

## Running it

All six files go through one runner, `crates/cad-geometry/tests/regression.rs`:

```bash
cargo test -p cad-geometry --test regression
```

A failing case is reported with its file and line, with the reason and the case itself:

```text
test-data/geometry/trims.txt:21: gave segment 0 0 40 0
    segment 0 0 100 0 / segment 40 -50 40 50 @ 80 0 => segment 0 0 50 0
```

Every failing case in a file is listed, not just the first.

To record a new case, write its inputs with any outcome and print what the implementation produces, in corpus syntax:

```bash
REPORT_CORPUS=1 cargo test -p cad-geometry --test regression -- --nocapture
```

Check the printed outcome by hand before recording it. The property each kind of case is held to is independent of the recorded outcome, so a wrong recording of a right result still fails, and a right recording of a wrong result usually does too.

## Invariants of the modify operations

Each modify operation has at least one invariant that is stated in a test and held over generated input, not just examples. The stress tests (`crates/cad-geometry/tests/stress.rs`) hold them on input chosen to be nearly degenerate: lines a hair from parallel, curves a hair from tangent, cuts a hair apart, and frames up to ten million units from the origin and a thousand times larger or smaller than usual.

| Operation | Invariant | Test |
| --- | --- | --- |
| Intersection (under TRIM, EXTEND, FILLET, snaps) | Every hit is on both objects; the order of the two does not matter; plain crossings are found at any distance from the origin | `intersect.rs`: `hits_lie_on_both_primitives`, `intersection_is_symmetric_in_count`; `stress.rs`: `nearly_parallel_lines_meet_on_both_or_not_at_all`, `a_nearly_tangent_line_hits_the_circle_on_the_circle`, `nearly_tangent_circles_meet_on_both_whichever_is_first`, `a_small_circle_on_a_huge_one_crosses_it_twice`, `crossings_are_found_however_far_from_the_origin` |
| OFFSET (line, circle, arc) | Offsetting and offsetting back is the identity; the result is at the distance | `stress.rs`: `offsetting_there_and_back_is_the_identity` |
| OFFSET (polyline) | Every point of the result is the offset distance from the source | `path_offset.rs`: `every_point_of_an_offset_is_at_the_offset_distance`, `a_convex_polygon_offset_outwards_grows_by_one_full_turn`; `stress.rs`: `a_polyline_of_nearly_straight_arcs_offsets_true` |
| TRIM | Kept and removed add up to the object; no piece is empty; the pick is in what was removed | `stress.rs`: `a_trimmed_line_is_the_line_less_the_stretch_at_the_pick`, `a_trimmed_arc_or_circle_is_itself_less_the_stretch_at_the_pick` |
| EXTEND | The other end is untouched; the result passes through the old end, is never shorter, and ends on the boundary | `stress.rs`: `an_extended_line_ends_on_the_boundary_and_keeps_its_other_end` |
| FILLET | The arc is tangent to both objects and has the radius asked for | `corner.rs`: `a_fillet_arc_is_tangent_to_both_lines`; `curve_fillet.rs`: `a_fillet_arc_is_tangent_to_a_line_and_a_circle`, `a_fillet_arc_is_tangent_to_two_arcs`; `stress.rs`: `nearly_parallel_lines_are_refused_or_properly_filleted`, `a_fillet_at_a_near_tangency_is_tangent_or_refused` |
| CHAMFER | Each line is set back by its own distance from the corner, and the bevel joins those two points | `stress.rs`: `a_chamfer_sets_each_line_back_by_its_own_distance` |
| BREAK | Exactly the stretch between the two points is removed | `stress.rs`: `breaking_removes_the_stretch_between_and_joining_undoes_a_split` |
| JOIN | Joining the pieces of a break at one point gives the object back; joined arcs cover every arc, whatever the order; a path cut into its segments chains back into one | `stress.rs`: the same, and `joined_arcs_cover_every_arc_whatever_the_order`; `join.rs`: `a_path_cut_into_its_segments_chains_back_into_one`; `cad-document/src/editing.rs`: `breaking_at_a_point_and_joining_gives_the_entity_back` |
| STRETCH | A point moves by the offset if the window holds it and stays otherwise; holding everything is a plain move | `cad-document/src/editing.rs`: `stretching_moves_exactly_the_vertices_the_window_holds`, `stretching_a_line_circle_or_arc_moves_only_what_is_held` |
| MOVE, COPY, ROTATE, SCALE, MIRROR | Rotation there and back, and mirroring twice, are the identity; rotation keeps distances; scaling multiplies them | `transform.rs`: `rotating_there_and_back_is_the_identity`, `mirroring_twice_is_the_identity`, `rotation_preserves_distances`, `scaling_multiplies_every_distance` |
| Everything | Nothing panics on any finite input, however extreme | `stress.rs`: `no_operation_panics_on_any_finite_input` |

## The long run

`scripts/check.sh` runs every property with the default number of cases. For many more:

```bash
scripts/stress.sh
```

runs every property test of `cad-geometry` and `cad-document` with 200,000 cases each (give another number as its argument). It takes a minute or so and is not part of the normal check. A failure is shrunk to its smallest input and saved in a `proptest-regressions` file next to the test, which the normal check replays from then on; fix the bug and commit that file with the fix.
