# ADR-0022: An edit re-packs the chunk that holds what changed, and the frame uploads the ranges that moved

**Status:** Accepted (2026-10-12)  
**Date:** 2026-10-12  
**Kind:** Evidence (resolves §35 "committed-buffer reuse"; the edit cost #171 measured)  
**Design refs:** §12.2, §12.3, §12.6, §35, §42; ADR-0002, ADR-0003, ADR-0014, ADR-0017, ADR-0020, ADR-0021

## Context

A committed edit changes the drawing's shown revision, and the whole `RenderScene` was rebuilt — every entity to instances, every dimension laid out, every hatch cut — and every committed buffer re-uploaded. #171 measured that at 69 ms of packing for the 100k mixed drawing natively, and an edit-to-frame of 218 ms in Firefox against a pointer frame of 61 ms: the rebuild and the upload of 1.6 million instances, after every edit, at every size. §42 asks for 50k "smooth for normal editing".

Two shapes of chunking were open: by **entity ID range** (a chunk is the entities whose IDs fall in a range of 512) or by **spatial tile** (a chunk is what lies in a cell of the plane). A spatial tile is what culling (#173) would want; an ID range is what an edit wants, because the drawing's `apply` reports what it inserted, removed, and replaced *by ID*, and because a transformed entity does not leave its chunk, while it would leave its tile and have to be removed from one chunk and added to another.

## Decision

### 1. The drawing journals what each change touched

`Drawing::apply` keeps a journal of its last 64 changes to what is shown: for each, the shown revision before and after, and the IDs it inserted, removed, or replaced — including the dependents of a table change: every dimension drawn by a restyled dimension style, every reference, direct or through other blocks, of a redefined block (§19.6). A layer switched on or off, which changes what is shown without naming an entity, journals *everything*. `Drawing::shown_changes_since(revision)` walks the journal back to that revision and returns the IDs, or `None` when it cannot say: a layer toggle on the way, a revision the journal has forgotten, a drawing it never knew. Undo and redo apply changes like any other and are journaled like any other.

### 2. The scene is packed in chunks of 512 entity IDs, laid end to end

`RenderScene` packs every shown entity in ID order, the entities of one chunk together, and remembers how many segments, arcs, and triangles each chunk produced. When the drawing can say what changed, `SceneCache::scene` re-packs only the chunks holding those IDs — each into a scratch frame from the scene's origin, spliced over the old run in the scene's buffers, with what follows moved when the chunk's size changed — and leaves the rest untouched. When it cannot, the scene is rebuilt whole as before. The buffers stay one run per kind, batched as one, so the render boundary carries the same buffers it did (ADR-0002); nothing per chunk crosses it.

The origin is fixed at the first full build, so a repacked chunk's offsets are from the same point as its neighbours'. (Since #188 a chunk has an origin of its own when its geometry is out of reach of the previous chunk's, and keeps it across repacks while its geometry stays within reach — [ADR-0028](0028-chunk-origins.md); a drawing of ordinary size is still one origin, one run per kind.) The style table is append-only: a style index means what it meant across repacks, and a row no entity uses any more stays until the next full build (ADR-0014).

### 3. The frame uploads the ranges that changed

The scene keeps, per kind, the runs of instances that changed since they were last taken: the chunk's own run when its size held, from the chunk's start to the end of the buffer when it did not. One new coarse call, `scene_dirty_ranges`, returns them — `kind, first, count` triples, taken once — and the adapter uploads those runs with `bufferSubData`, or the whole buffers when a buffer had to grow or the GPU holds no scene yet (a lost context). More than 64 ranges, or a whole rebuild, is reported as everything. The style-index and path-distance buffers are capacity-managed like the instance buffer, so they take the same ranges.

### 4. Chunk size

512 IDs. At 100k mixed entities that is 200 chunks of about 8,000 segments each: a repack of one costs a third of a millisecond, and the upload of one chunk is 130 KB against 25 MB for the scene. A smaller chunk saves little more packing and multiplies the ranges an edit of many entities touches; a larger one multiplies what one edit re-packs. Nothing in the boundary knows the size: `CHUNK_IDS` is the scene's.

## Measurements

Native, `cargo bench -p cad-benchmarks --bench mixed`, release, on the mixed drawings of #171 (Intel i7-12650H, WSL2):

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| A line in the middle moved, its chunk re-packed | 0.32 ms | 0.32 ms | 0.33 ms |
| A line drawn, the last chunk re-packed | 0.026 ms | 0.026 ms | 0.030 ms |
| The same edit, the scene rebuilt whole (before) | 6.8 ms | 37 ms | 76 ms |

In Firefox (software WebRender), `pnpm --dir web bench:mixed`, the frame after one line is drawn, mean / worst of ten:

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Edit, before (#171) | 24 / 33 ms | 123 / 145 ms | 218 / 286 ms |
| Edit, now | 11 / 24 ms | 42 / 86 ms | 79 / 152 ms |
| Pointer, for comparison | 6.7 / 8.0 ms | 30 / 31 ms | 60 / 62 ms |

The edit frame is now the pointer frame plus the upload of one chunk's run: 42 against 30 ms at 50k, 79 against 60 ms at 100k, on the mean of ten. The worst of the ten is the first edit after an open, when the instance buffers were allocated to fit and have to grow (§12.6), so that one upload is whole; the nine after it are within a frame of the pointer's. What is left is the draw, which is the rasteriser's (#110, #173).

## Alternatives considered

- **Spatial tiles.** Right for culling, wrong for editing: a moved entity changes two tiles, and a tile's contents must be found by a query rather than read off the ID. If #173 wants tiles, they can be a grouping *within* the ID order (the batches say where to draw from), not a second chunking.
- **One buffer per chunk across the boundary.** Two hundred calls a frame at 100k, against ADR-0002's "frames pulled in batches"; and the GPU would draw two hundred batches per kind. Splicing into one buffer costs a memmove of what follows only when a chunk's size changes — 25 MB at most, a few milliseconds, and only on an edit that changes an instance count in a chunk that is not the last.
- **Per-entity instance ranges in the scene.** The finest repack, and a map of a hundred thousand ranges to keep; a chunk of 512 is a third of a millisecond and needs a vector of two hundred counts.
- **Diffing the packed buffers to find the ranges.** Would read 25 MB to find what the journal already knows.

## Evidence

- `crates/cad-document/src/drawing.rs`: the journal; `shown_changes_since` for one change, for several, for a layer toggle (`None`), and for a revision too old.
- `crates/cad-render/src/scene.rs`: a change of properties re-packs one chunk and keeps the style rows; a layer off rebuilds whole; frames of identical drawings are equal; pointer-free requests build nothing.
- `crates/cad-render/tests/render_boundary.rs`: one edit re-packs one chunk, the untouched instances are bit-identical and from the same origin, the dirty range lies within the chunk, the repacked scene is as long as one built afresh; pointer movement reports nothing; a layer switched off reports everything; a dimension restyled re-packs the chunk holding its dimension.
- `web/src/cad/engine.test.ts`: the ranges cross the boundary once, and an edit names the chunk's run.
- `web/e2e/transform.spec.ts`: the commit of a rotation is one partial upload.
- `web/src/cad/render-boundary.test.ts`: `scene_dirty_ranges` is in the reviewed set.

## Consequences

- `Drawing::shown_changes_since`, `Drawing::id_bound`; `cad_render::{CHUNK_IDS, DirtyRange, InstanceKind}`, `SceneCache::take_dirty`, `partial_build_count`; `CadSession::scene_dirty_ranges`; `FrameBuffers.dirty`, `InstanceLayer.upload` with ranges, `RendererStats.scenePartialUploads`.
- The "Two render lifetimes" rule gains a clause: a change to the drawing re-packs and re-uploads the chunks holding what changed; pointer movement, pan, and zoom still touch nothing committed.
- The mixed benchmark's edit groups are `edit_and_repack`, `insert_and_repack`, and `edit_and_rebuild_whole`.

## Revisit when

- A GPU's frames (#110) show the draw dominating at 100k: culling (#173) groups within the ID order, drawing the batches whose chunks are in view.
- Edits of many entities at once (a STRETCH across the drawing) name more than 64 chunks: a hierarchy of ranges, or chunks of fewer IDs.
