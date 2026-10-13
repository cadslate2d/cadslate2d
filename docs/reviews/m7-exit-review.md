# M7 exit review — Rendering and Performance Hardening

**Date:** 2026-10-12  
**Scope:** DESIGN §35, §42, as rewritten at the M6 exit ([roadmap](../roadmap.md))  
**Outcome:** Every agent-doable item of the M7 scope is implemented, measured, and tested; an automated scenario drives the 100k mixed drawing through the §42 targets; the milestone is closed. M7 set out to measure before building, and the numbers chose: a packed bulk loader made opening a large drawing eight times faster to index; a mixed 100k drawing was measured natively and in two browsers; an edit went from rebuilding the whole scene to re-packing one chunk of 512 entities; culling and a worker were measured and declined, each with the figure that would reopen it; strokes got their baselines and needed nothing; a panic in the engine now leaves the drawing recoverable; two coverage-guided fuzzers found five reader and writer defects in their first hours; curves past single precision and drafting ten million units out both work. What needs a person — WebKit, real GPUs, the text atlas that waits on them — moves to M8 with its name on it.

## The scenario

`web/e2e/m7-scenario.spec.ts`, in Firefox (Chromium's software rasteriser takes seconds a frame at this size; it is skipped there, and in CI, where the 32 MB generated drawing is absent):

1. The **100k mixed drawing** — 44,000 lines, 8,000 circles, 8,000 arcs, 10,000 polylines, 10,000 texts, 8,000 block references, 8,000 dimensions, 4,000 hatches; 1.57 million segments, 42,000 arcs, 136,000 triangles on screen — is **opened** in under a second and its first frame drawn in under a second and a half.
2. Sixty frames each of **pan**, **zoom**, and **pointer** at the default view stay under the bounds — half a second a frame, "usable" with room for a machine whose CPU the rest of the suite shares; the measured figures in [`docs/performance.md`](../performance.md) are a tenth of that — and ten **edits** under 600 ms each.
3. **Zoomed in** twenty times, pan frames stay under 400 ms.
4. A line drawn through the command line reaches the drawing; `DXFOUT` **saves** 32 MB with every polyline, hatch, and dimension.

## Done when (epic #106)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| The §42 targets are measured on a mixed drawing at 10k, 50k, and 100k, native and in two browsers, and recorded | #171 | [`docs/performance.md`](../performance.md): `cargo bench --bench mixed`, `pnpm bench:mixed`, `pnpm bench:main-thread` |
| One edit re-packs what changed, not the scene; culling, a text atlas, and a worker are each decided by measurement, with an ADR | #172, #173, #178; #175 waits on #110 | [ADR-0022](../adr/0022-scene-chunks.md) (an edit at 100k: 0.33 ms where it was 69), [ADR-0023](../adr/0023-viewport-culling.md) (declined: zoomed-in frames are the cheaper ones), [ADR-0024](../adr/0024-autosave-in-slices.md) (no worker; the autosave in slices holds the thread for 64 ms where it held 539) |
| Joins, caps, dots, and heavy lineweights have baselines and draw no darker than the stroke | #174 | `web/e2e/strokes.spec.ts`: six baselines under `test-data/screenshots/`, and no pixel brighter than the stroke's colour around the joins |
| A panic in the engine is reported, the autosave untouched, the drawing recoverable | #176 | `engine-failure.test.ts`, `cad-app.test.ts` "a panic in the engine", `web/e2e/failure.spec.ts`; [ADR-0019](../adr/0019-local-persistence.md) extended |
| The DXF reader and the input grammar are fuzzed coverage-guided, in CI | #177 | `fuzz/`, `scripts/fuzz.sh`, `.github/workflows/fuzz.yml` run on request and weekly; `fuzz/README.md` records the runs and the five findings |
| The carried hardening issues are done or recorded: the bulk loader, curves at extreme zoom, drafting far from the origin; Firefox in CI (WebKit and real GPUs need a person) | #108, #112, #151; #109, #110 | [ADR-0007](../adr/0007-spatial-index.md) amended (a packed tree of our own: 10.8 ms to index 100k, from 94); [ADR-0009](../adr/0009-curve-rendering.md) extended (`curves.spec.ts` sweeps to 10⁹ px of radius); [ADR-0012](../adr/0012-tolerance-far-from-origin.md) and [ADR-0003](../adr/0003-view-ownership-and-viewport-independent-scene.md) extended (`far_from_origin.rs`, `far-from-origin.spec.ts`). Firefox runs locally in every end-to-end run; CI is Chromium (the hosted runner's Firefox has no WebGL2, #109) |

## What was built

- **A packed spatial tree** (#108): STR bulk loading into a tree of our own, with the R-tree kept for growth and a repack when the removed outnumber the packed; opening the 100k drawing indexes in 10.8 ms.
- **The mixed benchmark drawing** (#171): `cad_benchmarks::mixed` in a drafter's proportions, generated from a seed; native benches for open, scene build, and edit; browser probes for frames (`probeFrames`) and, later, main-thread tasks (`probeMainThread`); `docs/performance.md`.
- **The scene in chunks** (#172, ADR-0022): the drawing journals what each change touched, `shown_changes_since` tells the cache which chunks of 512 IDs to re-pack in place, dirty ranges cross the boundary once (`scene_dirty_ranges`), and the adapter uploads those runs with `bufferSubData`; the style table is append-only so indices stay what they were.
- **Culling measured and declined** (#173, ADR-0023): the frame probe gained zoomed-in runs; at 100k a zoomed-in frame is 38 ms against 60 at the default view and linear in the instance count — the rasteriser's vertex stage, which chunk culling could not cut on a drawing whose ID order is random.
- **Stroke baselines** (#174): a polyline's sharp, shallow, and reversing joins at 0.25, 0.70, and 2.11 mm; a dashed polyline turning corners; a DOTS hatch; a selected polyline. Looked at, four times enlarged, and found right: an opaque capsule over an opaque capsule of the same colour is that colour once.
- **Crash recovery** (#176): the panic hook keeps the message, the TypeScript engine guards every call and declines all after the first trap, the app stops the frame loop and the autosave and shows what happened; a reload offers the kept drawing back. `panic_for_test` makes it testable end to end.
- **Fuzzing** (#177): libFuzzer targets for `cad_dxf::import` (seeded with every fixture; the writer's output must read back and re-export float-exact) and for `cad_input::resolve` over every kind of prompt; `scripts/fuzz.sh`; a weekly CI job that caches the corpus it grows.
- **No worker; the autosave in slices** (#178, ADR-0024): `cad_dxf::Export` is the writer as a job stepped by a budget of records and ended by a change; the app writes the autosave two thousand records per turn of the event loop. Open and DXFOUT, user actions within a second at 100k, stay as they are.
- **Curves past single precision** (#112): past 50,000 px of on-screen radius the overlay carries the visible span of every committed circle and arc, tessellated in `f64` from the view centre, and the shader collapses the instance; a circle keeps its stroke past 10⁹ px.
- **Drafting far from the origin** (#151): every command, snap, and modify operation at 10⁷ and 10¹⁰ units, headless and end to end, passing as they stood; the render origin answered.

## Decisions made in M7

| Decision | Where |
| --- | --- |
| Opening a drawing bulk-loads a packed tree of our own; the R-tree is for growth | ADR-0007, amended |
| The scene is packed in chunks of 512 entity IDs laid end to end; an edit re-packs the chunks the drawing's journal names, and the frame uploads the ranges that moved | ADR-0022 |
| Culling is not taken up: zoomed-in frames are the cheaper ones, the floor is per-instance in the software rasteriser, and ID chunks would cull nothing on a drawing not drawn region by region | ADR-0023 |
| No worker: the one recurring main-thread cost, the autosave, is an export in slices; open and DXFOUT are user actions within a second at the top of the range | ADR-0024 |
| A panic is declined in TypeScript before the call, never reported from Rust; the autosave is never written by a failed engine | ADR-0019, extended |
| A curve past 5×10⁴ px of radius is its visible span in the overlay; the span carries the theme's entity colour, the overlay having no style table | ADR-0009, extended |
| A drawing wholly far out is drawn exactly; one spanning 10⁷ is placed to a unit at its far end; chunk origins are the rebase when such a drawing is asked for | ADR-0003, extended |
| The fuzzer's bar for a re-export is float-exact — ulps of the file's largest magnitude, numbers inside text compared as numbers; the curated fixtures keep byte-for-byte | `fuzz/README.md` |

## What testing found

The fuzzers found five defects in their first hours, each now a fixture and a fix:

- **Every group value was trimmed**, so a text ending in a space lost it and an MTEXT written in pieces fused the words at a piece boundary. The reader keeps a value as written beside its trimmed form (`text-with-spaces.dxf`).
- **A pattern origin at 10²⁰ hung the import**: the pattern's line indices were where one and the next are the same number, and the walk never moved; a spacing small enough to make the counts infinite passed `>` as NaN. `pattern_lines` refuses both and the dash walk goes by index (`hatch-pattern-origin-far.dxf`).
- **A `%` written as itself** starts a `%%` code on reading: eleven came back as three, and a text ending in `%%` lost it. The writer writes `%%%` (`a_percent_sign_in_a_text_or_a_paragraph_survives`).
- **A block placing itself forty times hung the cycle check**, found by CI's first five minutes: reachability and nesting depth followed every reference afresh, forty to the power of the nesting limit; each block is now looked into once (`self-referencing-block-fanout.dxf`).
- **Derived values drift by an ulp** across a round trip — a radius dimension's far point by ulps of its radius, a dimension's text formatted from it. Not bugs; the fuzzer's tolerance says so.

Elsewhere:

- **The ear clipper bridged a hole from the wrong endpoint.** The leftward ray's candidate was the hit edge's right-hand endpoint, the mirror of the standard choice, so a bridge could cross the hole it was for; a workspace property-test run found it with a diamond and an eight-point star (`a_bridge_never_crosses_the_hole_it_is_for`).
- **A benchmark that reuses a drawing measures the drawing it grew.** The whole-rebuild figure came out at 137 ms at 10k until each group got a fresh drawing: 6.8 ms.
- **A dialog over the canvas swallows the wheel.** The import report sat over the far plot in the spanning-drawing scenario, and seventeen wheel steps zoomed nothing; a zoom aimed at a dot must be aimed afresh each e-fold, since a pixel there is thousands of units.
- **A 0.25 mm line straddling a pixel row draws dimmer** than the diagonals beside it: antialiasing of a width under a pixel over two rows, not a join. The baseline records it.

## Measurements

[`docs/performance.md`](../performance.md) has them all; the ones M7 turned on:

| What | Before | After |
| --- | --- | --- |
| Indexing the 100k mixed drawing on open | 94 ms | 10.8 ms |
| One edit, natively, at 100k | 69 ms (the scene rebuilt whole) | 0.33 ms (one chunk re-packed) |
| The edit frame in Firefox, 50k / 100k | 123 / 218 ms | 42 / 79 ms — the pointer frame plus one chunk's upload |
| The autosave holding the main thread at 100k | 539 ms | 64 ms (the last step: the buffer taken and put) |
| A frame zoomed in twenty times at 100k | — | 38 ms, against 60 at the default view: the rasteriser's per-instance floor |
| A circle's stroke at an on-screen radius of 10⁹ px | gone past 1.6×10⁷ | drawn, antialiased |
| Fuzzing | the structured fuzz of M5 | 149 million executions of the grammar in an hour, nothing found; the reader's hours listed in `fuzz/README.md` |

**Not established:** any of it on a GPU (#110) — every frame figure is a software rasteriser's, and the text atlas (#175) waits on that; WebKit (#109).

## What M7 taught us

- **Measure first, then the numbers choose.** Three of M7's items ended in "no": culling, a worker, a stroke fix. Each has a figure that would reopen it, which is cheaper than the code would have been.
- **The journal was the missing piece.** Re-packing a chunk needed only to know which entities a change touched; `apply` already knew, and the dependents of a style or a block came with it. The render boundary carried the same buffers plus one list of ranges.
- **A fuzzer earns its hour in minutes.** Five defects in the reader and writer — two of them hangs — in the first runs, where twenty thousand structured files had found none. Every finding was at an edge the authors had not imagined: a trailing space, a `%`, an origin at 10²⁰, a block placing itself forty times.
- **Slicing beats moving.** The autosave's cost was not where the work ran but that it ran at once. An export that stops every two thousand records fixed it without a second engine, a copy of the drawing, or a thread.
- **The overlay is where view-dependent geometry goes**, again: the grid in M1, and now the span of a curve the shader cannot place. Nothing committed changed in either case.

## Open items

Needs a person (carried to M8 as #109, #110, #111, #175):

1. **Real GPUs** (#110): every frame figure here is from a software rasteriser; the culling and text-atlas decisions rest on that caveat. **WebKit** (#109).
2. **The R-tree crash upstream** (#111).
3. **An interactive CAD program** reading our R2000 files, dimensions and hatches included; the File System Access path in a real browser; how M3 and M4 feel against classic CAD; the arc grip behaviour (ADR-0010); the M2 exit assessment (#101).

Known limitations, accepted for now:

- A curve past the shader's radius is drawn in the theme's entity colour at the ordinary width; its own colour, lineweight, and linetype are not carried (ADR-0009, extended).
- A drawing spanning 10⁷ units is placed to a unit at its far end (#188 in M8).
- The scenario's 100k drawing is generated, not committed: CI runs the suite without it.
- The first edit after an open uploads whole, when the instance buffers grow to fit (ADR-0022); the nine after it do not.

## M8, broken down

§36 lists possible work and says nothing in it should constrain M0. M8 takes what a drafter opening files from other programs meets first — the entities still reported, the layouts a save loses, the view commands classic drafting assumes — and leaves the rest where it was ([`docs/roadmap.md`](../roadmap.md) has the order):

1. **ZOOM and PAN commands** (#180): extents, window, previous, scale; a far plot reached by name.
2. **Ellipses** (#181) and **splines** (#182): the curves still reported on import, as entities with primitives, grips, commands, and DXF both ways.
3. **Block attributes** (#183): definitions, values on references, the prompts on insert, DXF both ways.
4. **Layouts and paper space kept through a round trip** (#184): not drawn, but not lost.
5. **Advanced selection** (#185): fence, polygons, previous, last, all, quick select.
6. **User aliases and settings** (#186), persisted in the browser.
7. **PDF export of model space** (#187): the first form of plotting.
8. **Chunk origins** (#188) for drawings that span ten million units, carried from #151.
9. **Exit** (#189).

Not in M8: SHX fonts, xrefs, scripting, plugins, native packaging, DWG, a native project format, a WebGPU adapter — they stay on the epic as ideas.
