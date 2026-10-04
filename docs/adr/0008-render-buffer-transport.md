# ADR-0008: Render buffer transport — views into WASM memory, instanced segments, reused buffers

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves a PROVISIONAL item)  
**Design refs:** §12.3, §12.4, §12.6, §28, §42; confirms ADR-0002 §4 and ADR-0003 §3

## Context

DESIGN §12.6 leaves the packed-buffer transport PROVISIONAL and lists what M0 should measure: allocation behaviour, copy overhead, typed-array views versus copied arrays, committed-buffer reuse, transient-buffer churn, and GPU upload cost. ADR-0002 and ADR-0003 chose a starting point — zero-copy views, one 16-byte instance per segment — to be confirmed or replaced by measurement.

## Decision

Confirm the M0 starting point as the transport:

1. **Geometry crosses as typed-array views into WASM linear memory.** The TypeScript wrapper lends them to a callback and re-creates them on every pull; nothing retains a view.
2. **One instance per stroke segment: four `f32`s (`x0, y0, x1, y1`),** relative to the frame's `f64` origin. Stroke width is applied in the vertex shader.
3. **Batches are stored in transfer layout** (`style, first, count` as `u32`), so a pull converts nothing and allocates nothing in Rust.
4. **GPU buffers are allocated once and reused;** they grow geometrically when a frame no longer fits.
5. **The committed scene is rebuilt whole on each document change.** No incremental scene updates.
6. **Release WASM is built with `opt-level = 3`,** not `"s"`.

## Evidence

`pnpm --dir web bench:transport`: the production bundle in headless Chromium, with an in-page probe (`web/src/renderer/webgl/transport-probe.ts`) operating on drawings generated at 1k, 10k, and 100k lines. Intel i7-12650H. Each figure is a mean over a batch large enough for the browser's clamped clock to resolve.

**The GPU is SwiftShader, a software rasteriser.** Upload figures therefore measure a memcpy-like path, not a real driver. Comparisons between layouts and between views and copies are meaningful; absolute upload times on real hardware are not established here.

| Operation | 1k | 10k | 100k |
| --- | --- | --- | --- |
| Create views over the scene | 0.1 µs | 0.1 µs | 0.2 µs |
| Create views and copy the data out | 10 µs | 62 µs | 705 µs |
| Upload from views into an existing buffer | 6 µs | 14 µs | 94 µs |
| Copy, then upload the copy | 19 µs | 92 µs | 908 µs |
| Upload an expanded-vertex buffer (5× the bytes) | 7 µs | 52 µs | 827 µs |
| Upload with reallocation every time | 7 µs | 15 µs | 121 µs |
| One pointer-move frame (engine call, overlay pull and upload, both transforms) | 3.0 µs | 2.6 µs | 2.7 µs |
| One edit: commit a line, rebuild the scene, re-upload it | 64 µs | 314 µs | 2.05 ms |

Committed data size: 16 KB, 160 KB, 1.6 MB instanced; 80 KB, 800 KB, 8 MB as expanded vertices.

Native scene preparation alone (`cargo bench -p cad-benchmarks --bench render`): 0.65 ms at 100k, cold or after an edit; 1.4 ns to look up an unchanged scene.

JavaScript heap, with `--enable-precise-memory-info`: no growth over 20,000 consecutive pointer frames at any size (the measured delta was slightly negative, i.e. within garbage-collector noise).

Reading the numbers:

- **Views are free; copies are not.** Creating views costs 0.1–0.2 µs whatever the drawing size. Copying first makes the upload path about 10× more expensive at 100k.
- **Instancing is the right layout.** The expanded-vertex equivalent is 5× the bytes and about 9× the upload time at 100k, before counting the Rust-side cost of generating it.
- **Pointer movement costs about 3 µs and does not depend on drawing size.** That is the §12.3 requirement, with more than three orders of magnitude of headroom inside a 16.7 ms frame.
- **A whole-scene rebuild per edit is affordable through 100k:** about 2 ms, an eighth of a frame. Incremental scene updates would add complexity for no visible gain at the stated scale targets.
- **Buffer reuse versus reallocation is within noise in software GL.** Reuse is kept because it is already written, cannot be worse, and real drivers are known to penalise orphaning large buffers.
- **Nothing accumulates per frame.** The probe cannot measure allocation *rate* — each pull creates a few small short-lived objects — only that the heap does not grow.

### Optimisation level

| | `opt-level = "s"` | `opt-level = 3` |
| --- | --- | --- |
| `.wasm` size (after `wasm-opt -Os`) | 202 KB | 220 KB |
| Import the 100k drawing (parse, build drawing and index) | 610 ms | 235 ms |
| Import the 10k drawing | 51 ms | 22 ms |
| One edit at 100k | 2.8 ms | 2.05 ms |

Eighteen kilobytes buys a 2.6× faster open. The table above uses `opt-level = 3`.

## Alternatives considered

- **Copy geometry into JS-owned arrays.** Removes the view-lifetime rule, at 10× the transport cost. The rule is enforced structurally instead: `readScene`/`readOverlay` take a callback, and a test grows memory to show both the hazard and that pulls stay correct.
- **Four expanded vertices per segment.** Simpler shader, 5× the data. Rejected on the numbers.
- **Incremental scene patches** (send only changed entities). Not needed at 2 ms per full rebuild; would complicate the revision rule that makes the boundary easy to reason about.
- **One combined "frame" call** returning scene and overlay together. Saves a few calls per frame out of four, and would marshal unchanged scene data on pointer moves.

## Consequences

- The view-lifetime rule stays a sharp edge, confined to `web/src/cad/engine.ts`.
- Linetypes (§23.2) need accumulated path distance per segment, and joins need neighbour information; the instance grows when those arrive. The layout is confirmed for plain strokes only.
- Curve data is undefined here (§12.5). The curve-rendering ADR must state what crosses the boundary for circles and arcs.
- Absolute GPU upload cost on real hardware is unmeasured. If uploading 1.6 MB proves slow on an integrated GPU, the first lever is uploading only when the revision changes — which is already the case.

## Revisit when

- Measurements on a real GPU contradict the software-GL comparisons.
- Edit latency at 100k becomes visible once more entity types make scene preparation heavier.
- Linetype or join data changes the instance layout.
