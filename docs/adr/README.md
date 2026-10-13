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
| [0005](0005-dxf-baseline.md) | DXF baseline: hand-written ASCII reader, R12 writer | Accepted; the release superseded by 0018 | Evidence |
| [0006](0006-entity-store.md) | Entity store: dense vector with an ID-to-slot map | Accepted | Evidence |
| [0007](0007-spatial-index.md) | Spatial index: rstar R*-tree, grown by insertion only | Accepted for M0–M1 | Evidence |
| [0008](0008-render-buffer-transport.md) | Render buffer transport: views, instanced segments, reused buffers | Accepted | Evidence |
| [0009](0009-curve-rendering.md) | Curve rendering: analytic arc instances under a covering strip | Accepted | Evidence |
| [0010](0010-grip-editing-and-arc-grip-semantics.md) | Grip editing as a command, and what arc grips hold fixed | Accepted | Evidence |
| [0011](0011-polyline-offset.md) | Polyline offset: slice the raw offset and keep what is at the distance | Accepted | Evidence |
| [0012](0012-linear-tolerance-far-from-the-origin.md) | The linear tolerance far from the origin | Accepted | Evidence |
| [0013](0013-entity-properties-and-tables.md) | Entity properties, tables, and one change model for both | Accepted | Bootstrap |
| [0014](0014-colours-across-the-render-boundary.md) | Colours across the render boundary: a style index per instance and a style table | Accepted | Evidence |
| [0015](0015-linetype-rendering.md) | Linetype rendering: dashes drawn by the shader from a distance along the path | Accepted | Evidence |
| [0016](0016-text-rendering.md) | Text rendering: a stroke font drawn through the line instances | Accepted | Evidence |
| [0017](0017-blocks.md) | Blocks: definitions as table rows, references resolved through the table | Accepted | Bootstrap |
| [0018](0018-dxf-release-r2000.md) | The DXF release is R2000: lineweights, MTEXT, LWPOLYLINE, and the structure they need | Accepted | Evidence |
| [0019](0019-local-persistence.md) | Local persistence: the autosave is one IndexedDB record, written when the revision changes | Accepted | Evidence |
| [0020](0020-dimensions.md) | Dimensions: definition points and a style, laid out by pure functions whenever asked | Accepted | Bootstrap |
| [0021](0021-filled-regions.md) | Filled regions: triangles as a third committed instance kind, cut from the boundary when asked | Accepted | Evidence |
| [0022](0022-scene-chunks.md) | Scene chunks: an edit re-packs the chunk that holds what changed, and the frame uploads the ranges that moved | Accepted | Evidence |
| [0023](0023-viewport-culling.md) | Viewport culling: not taken up — zoomed-in frames are the cheaper ones, and the floor is the rasteriser's per-instance work | Accepted | Evidence |
| [0024](0024-autosave-in-slices.md) | No worker; the autosave is written in slices on the main thread, a change under it abandoning the job | Accepted | Evidence |
| [0025](0025-kept-layouts.md) | Layouts and paper space are kept as the file's own groups, their handles remapped, written back and not drawn | Accepted | Design |
| [0026](0026-user-aliases-and-settings.md) | User aliases under the built-in table, settings in `localStorage`, restored in one call; the theme is UI state | Accepted | Design |
| [0027](0027-pdf-plot.md) | A PDF plot of model space is the renderer's entity walk written as a page by `cad-plot`; text as strokes, no fonts | Accepted | Design |
| [0028](0028-chunk-origins.md) | Each scene chunk is packed against an origin within reach of its geometry; regions of chunks drawn under their own transforms | Accepted | Design |

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
