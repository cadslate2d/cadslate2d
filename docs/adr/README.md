# Architecture Decision Records

ADRs record architecture decisions made against the [design baseline](../DESIGN.md) (§46).

There are two kinds:

- **Bootstrap ADRs** fill gaps the baseline leaves unspecified but that must be settled before the first line of M0 code. They do not change any LOCKED decision.
- **Evidence ADRs** resolve PROVISIONAL / EXPERIMENT items, or change the baseline, and must cite implementation evidence, measurements, or a concrete failed assumption (§46).

## Index

| ADR | Title | Status | Kind |
| --- | --- | --- | --- |
| [0001](0001-crate-layering-and-session.md) | Crate layering and the interactive session | Accepted | Bootstrap |
| [0002](0002-wasm-boundary-api.md) | WASM boundary: events in, dirty flags out, pulled frames | Accepted | Bootstrap |
| [0003](0003-view-ownership-and-viewport-independent-scene.md) | View ownership and viewport-independent committed geometry | Accepted | Bootstrap |
| [0004](0004-wasm-toolchain-and-build.md) | WASM toolchain and build pipeline | Accepted | Bootstrap |
| [0005](0005-dxf-baseline.md) | DXF baseline: hand-written ASCII reader, R12 writer | Accepted | Evidence |
| [0006](0006-entity-store.md) | Entity store: dense vector with an ID-to-slot map | Accepted | Evidence |
| [0007](0007-spatial-index.md) | Spatial index: rstar R*-tree, grown by insertion only | Accepted for M0–M1 | Evidence |
| [0008](0008-render-buffer-transport.md) | Render buffer transport: views, instanced segments, reused buffers | Accepted | Evidence |
| [0009](0009-curve-rendering.md) | Curve rendering: analytic arc instances under a covering strip | Accepted | Evidence |
| [0010](0010-grip-editing-and-arc-grip-semantics.md) | Grip editing as a command, and what arc grips hold fixed | Accepted | Evidence |
| [0011](0011-polyline-offset.md) | Polyline offset: slice the raw offset and keep what is at the distance | Accepted | Evidence |
| [0012](0012-linear-tolerance-far-from-the-origin.md) | The linear tolerance far from the origin | Accepted | Evidence |
| [0013](0013-entity-properties-and-tables.md) | Entity properties, tables, and one change model for both | Accepted | Bootstrap |
| [0014](0014-colours-across-the-render-boundary.md) | Colours across the render boundary: a style index per instance and a style table | Accepted | Evidence |

## Expected evidence ADRs

These are deliberately **not** written yet. Each is the deliverable of a tracked spike or benchmark:

| Topic | Design ref | Resolved by |
| --- | --- | --- |
| Local persistence (IndexedDB / OPFS / hybrid) | §25 | M5 |
| Worker architecture | §37 | M7, profiling-driven |

DESIGN.md §46 lists example ADR numbers; those were illustrative. Numbers are assigned in the order ADRs are written.

## Conventions

- File name: `NNNN-short-kebab-title.md`, numbered sequentially, never renumbered.
- Status: `Proposed` → `Accepted` → optionally `Superseded by NNNN`.
- An accepted ADR is not edited to change its decision; write a new ADR that supersedes it.
- Sections: Context, Decision, Alternatives considered, Consequences, Revisit when.
