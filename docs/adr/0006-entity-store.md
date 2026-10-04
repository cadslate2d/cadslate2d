# ADR-0006: Entity store — dense vector with an ID-to-slot map

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves a PROVISIONAL item)  
**Design refs:** §10.3, §19.4, §42

## Context

DESIGN §10.3 locks the *concept* — stable `EntityId`, mutable physical slot — and leaves the concrete store "open until benchmarked". M0 shipped the simplest implementation that satisfies the concept: entities in a dense `Vec`, a `HashMap<EntityId, usize>` from ID to slot, and swap-remove on deletion. This ADR records whether to keep it.

## Decision

Keep the dense store as implemented in `cad_document::EntityStore`:

- entities live contiguously in a `Vec` and are iterated in storage order;
- `id_to_slot` maps each live ID to its current index;
- removal swap-removes and patches the one moved entry, so storage stays dense and freed slots are reused immediately;
- payloads are `Arc<Entity>`, shared with undo history rather than copied;
- no slot index appears in any public signature.

## Evidence

`cargo bench -p cad-benchmarks --bench document`, native x86-64 (Intel i7-12650H, `opt-level = 3`), generated drawings at constant density. The alternative is `HashMapStore`: a `HashMap<EntityId, Arc<Entity>>` and nothing else.

| Operation | Size | Dense | HashMap only |
| --- | --- | --- | --- |
| Iterate every entity (what scene preparation does) | 10k | 8.0 µs | 14.1 µs |
| | 50k | 71 µs | 102 µs |
| | 100k | 149 µs | 250 µs |
| 1000 lookups by ID | 100k | 9.7 µs | 8.3 µs |
| 1000 × remove then restore (erase + undo) | 10k | 48 µs | 24 µs |
| | 100k | 82 µs | 49 µs |

Through the full write path (`Drawing`: store, index, history, revision):

| Operation | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Commit 1000 single-line transactions | 0.69 ms | 0.85 ms | 0.87 ms |
| Undo then redo 1000 steps | 1.1 ms | 1.5 ms | 1.5 ms |

Reading the numbers:

- **Iteration is 1.4–1.8× faster dense.** That is the hot path: every committed edit rebuilds the scene by iterating all entities.
- **Lookup is a wash.** Both go through one hash probe.
- **Remove-and-restore is about 2× slower dense**, because it maintains two structures. In absolute terms that is 50–80 ns per entity, and it happens once per user action, not per frame.
- **The write path barely depends on drawing size**: 0.7–0.9 µs per committed entity from 10k to 100k, dominated by the spatial index, not the store.

## Alternatives considered

- **`HashMap` only.** Simpler by a few lines, but slower exactly where it matters.
- **Generational slot map (`slotmap` and similar).** Solves a problem the design already solved differently: IDs here are never reused, so generations buy nothing, and the handle would leak a storage concern into `EntityId`.
- **`BTreeMap<EntityId, _>`.** Ordered iteration for free, but pointer-chasing iteration and no density.
- **Structure-of-arrays per entity type.** Could speed up scene preparation further. Premature with one entity type; revisit if profiling at M7 points here.

## Consequences

- Iteration order is storage order, which changes when entities are removed. Anything that needs a stable order sorts by `EntityId` itself, as DXF export does.
- Benchmarks are native. WASM will be slower by some factor; the comparison between implementations is expected to hold, and the browser-side measurements in ADR-0008 show the end-to-end cost.
- Only `Line` exists. Entity size and enum layout will change as §11 fills in; re-run before relying on these absolute numbers.

## Revisit when

- Profiling shows scene preparation dominating edit latency at the 100k target.
- Entities gain large inline payloads (polylines, hatches) and `Arc` indirection or enum size becomes measurable.
