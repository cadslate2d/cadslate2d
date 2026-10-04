# Architecture Decision Records

ADRs record architecture decisions made against the [design baseline](../DESIGN.md) (§46).

There are two kinds:

- **Bootstrap ADRs** fill gaps the baseline leaves unspecified but that must be settled before the first line of M0 code. They do not change any LOCKED decision.
- **Evidence ADRs** resolve PROVISIONAL / EXPERIMENT items, or change the baseline, and must cite implementation evidence, measurements, or a concrete failed assumption (§46).

## Index

| ADR | Title | Status | Kind |
| --- | --- | --- | --- |
| [0001](0001-crate-layering-and-session.md) | Crate layering and the interactive session | Proposed | Bootstrap |
| [0002](0002-wasm-boundary-api.md) | WASM boundary: events in, dirty flags out, pulled frames | Proposed | Bootstrap |
| [0003](0003-view-ownership-and-viewport-independent-scene.md) | View ownership and viewport-independent committed geometry | Proposed | Bootstrap |
| [0004](0004-wasm-toolchain-and-build.md) | WASM toolchain and build pipeline | Proposed | Bootstrap |

## Expected evidence ADRs

These are deliberately **not** written yet. Each is the deliverable of a tracked spike or benchmark:

| Topic | Design ref | Resolved by |
| --- | --- | --- |
| Render buffer transport (views vs copies, packing, buffer reuse) | §12.6 | M0 transport measurement |
| Entity store implementation | §10.3 | M0 store benchmark |
| Spatial-index choice | §20 | M0 index benchmark (revisit with M2 editing workloads) |
| DXF baseline (target release, parser strategy) | §24.1 | M0 DXF writer work |
| Curve rendering strategy | §12.5, §23.3 | M1 curve-rendering spike |
| Local persistence (IndexedDB / OPFS / hybrid) | §25 | M5 |
| Worker architecture | §37 | M7, profiling-driven |

DESIGN.md §46 lists example ADR numbers; those were illustrative. Numbers are assigned in the order ADRs are written.

## Conventions

- File name: `NNNN-short-kebab-title.md`, numbered sequentially, never renumbered.
- Status: `Proposed` → `Accepted` → optionally `Superseded by NNNN`.
- An accepted ADR is not edited to change its decision; write a new ADR that supersedes it.
- Sections: Context, Decision, Alternatives considered, Consequences, Revisit when.
