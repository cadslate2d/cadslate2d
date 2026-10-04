# ADR-0007: Spatial index — rstar R*-tree, grown by insertion only

**Status:** Accepted (2026-10-04); confirmed with editing workloads at the M2 exit — see *Re-measured at the M2 exit* below  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves a PROVISIONAL item, in part)  
**Design refs:** §19.5, §20, §42

## Context

DESIGN §20 fixes the `SpatialIndex` interface and calls an R-tree PROVISIONAL: "real editing workloads must be benchmarked". M0 has one real workload — picking — plus the mechanical ones (open, commit, undo). Snapping, hover, window selection, and trim candidates arrive in M1–M2.

Running the first benchmark produced a finding that outranks every timing.

## Finding: `rstar` 0.13.0 can panic after `bulk_load`

Building an `RTree` with `RTree::bulk_load` from the 100,000-line benchmark drawing and then inserting one more entry panics inside rstar:

```text
thread 'main' panicked at rstar-0.13.0/src/algorithm/rstar.rs:137:9:
This is a bug in rstar.
```

It reproduces deterministically at 100,000 entries and not at 90,000, 120,000, 150,000, 200,000, or 300,000. A tree of the same 100,000 entries grown by `insert` does not panic. The failing branch is reached when insertion descends into a child it expects to be an inner node and finds a leaf, which points at bulk loading producing a tree whose shape violates what the insertion algorithm assumes.

In the application this is: open a large file, draw one line, and the WASM module aborts. Nothing in M0's test suite would have caught it; the benchmark at a scale target named in §42 did.

## Decision

1. Keep `rstar`'s R*-tree as the default `SpatialIndex`, behind the trait.
2. **Never call `RTree::bulk_load`.** `RTreeIndex::rebuild` grows the tree by insertion.
3. Keep `LinearIndex` (brute force) as the reference implementation that property tests hold the R-tree to, and as the benchmark baseline.
4. Two regression tests reproduce the original failure with the exact drawing: one on the index, one through `Drawing` (open, then commit). They live in `benches/tests/index_regression.rs` because they need the generator.

## Evidence

`cargo bench -p cad-benchmarks --bench document`, native x86-64 (Intel i7-12650H, `opt-level = 3`).

| Operation | 10k | 50k | 100k |
| --- | --- | --- | --- |
| 1000 pick-sized queries, R-tree | 0.34 ms | 0.45 ms | 0.62 ms |
| 1000 pick-sized queries, brute force | 26 ms | 129 ms | 276 ms |
| Insert 1000 entries into an existing tree | 0.64 ms | 0.75 ms | 0.91 ms |
| Update 1000 entries (remove + insert), R-tree | 1.4 ms | 1.7 ms | 2.1 ms |
| Update 1000 entries, brute force | 1.3 ms | 11 ms | 30 ms |
| Rebuild by insertion, R-tree | 6.4 ms | 36 ms | 94 ms |
| Rebuild, brute force | 0.03 ms | 0.16 ms | 0.39 ms |

The unsafe `bulk_load` was measured only in the first run, which used `opt-level = "s"`: 34 ms for 100k entries, against 262 ms for insertion under the same setting — roughly 8× faster.

Reading the numbers:

- **A pick costs about 0.6 µs at 100k entities**, and grows by less than 2× from 10k to 100k. Brute force is 75–450× slower and scales linearly — 0.28 ms *per query* at 100k, which would be felt once hover and snapping query on every pointer move.
- **Incremental maintenance is 0.6–2 µs per change**, nearly independent of size. The rebuild-on-idle fallback in §20 is not needed at these sizes.
- **Opening is where insertion-only costs something.** Rebuilding the index accounts for essentially all of the 89 ms that `Drawing::from_entities` takes at 100k natively. In the browser the whole import of the 100k drawing — parse, build, index — measures 235 ms (ADR-0008). That is acceptable for "100k usable", and it is the first thing to improve.

## Alternatives considered

- **Keep `bulk_load` and catch the panic.** Not possible: release WASM is built with `panic = "abort"`, and a tree that has panicked mid-insert is not trustworthy anyway.
- **Use `bulk_load` only for trees that are never mutated.** Every opened drawing is mutated by the next edit.
- **Write our own index now** (uniform grid, or an STR-packed R-tree with our own bulk loader). A grid handles the mix of very short and very long lines poorly; an R-tree of our own is real work that the numbers do not yet justify. The trait keeps this open.
- **Rebuild from scratch instead of inserting.** With brute force that is cheap but queries are not; with the R-tree it is the slow path.

## Consequences

- Opening a 100k-entity file spends most of its import time building the index. A safe bulk loader should cut that several-fold; this is tracked as a follow-up.
- We depend on a library with a known crash in one of its entry points. The entry point is not used, the regression tests pin the behaviour, and the dependency sits behind a trait with a second implementation already written.
- The upstream bug has not been reported by this project yet. Reporting it needs a decision by the maintainer, since it means posting to a third-party tracker.

## Revisit when

- M2 adds snapping, hover, and window selection: re-run with those query shapes (this is what §20 means by "real editing workloads").
- `rstar` fixes bulk loading, or a safe bulk loader is written.
- Profiling shows index maintenance in transaction commit becoming visible.

## Re-measured at the M2 exit (2026-10-04)

The decision is unchanged. This section adds the evidence §20 asked for: the query shapes that M2 introduced.

`cargo bench -p cad-benchmarks --bench document -- index/` and `--bench snap`, same machine and settings as above. The machine was busy; differences under about 30% are noise, and a two-second run of the window benchmark was misleading enough that these figures come from a six-second run.

| Operation | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Window query over 1% of the drawing, R-tree | 1.4 µs | 3.5 µs | 9.3 µs |
| Window query over 1% of the drawing, brute force | 36 µs | 239 µs | 599 µs |
| Window query over 25% of the drawing, R-tree | 14 µs | 121 µs | 327 µs |
| Window query over 25% of the drawing, brute force | 26 µs | 342 µs | 631 µs |
| 1000 snap queries next to endpoints (index query + exact snap points + ranking) | 0.93 ms | 1.33 ms | 1.21 ms |
| Update 1000 entries, R-tree | 6.5 ms | 2.4 ms | 3.6 ms |
| Update 1000 entries, brute force | 4.5 ms | 19 ms | 57 ms |

Reading the numbers:

- **Snapping is index-bound.** About a microsecond per query, the same at 10k and 100k entities. Snapping queries on every pointer move, so this is the workload that mattered most.
- **Small windows are 25–65× faster with the R-tree.**
- **Large windows are where an index helps least**, because most of the cost is producing the result. Even so, at a quarter of the drawing the R-tree is about twice as fast as scanning everything. There is no window size in these measurements at which brute force wins.
- **Updates** — what MOVE, ROTATE, SCALE, and grip edits do — stay within a few microseconds per entry.

Nothing here argues for a different structure. The two follow-ups stand: a safe bulk loader for opening large files, and the upstream report.
