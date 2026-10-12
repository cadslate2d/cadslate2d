# Performance

What the §42 targets — 10k entities effortless, 50k smooth for normal editing, 100k usable — measure as, on the mixed benchmark drawings, and how the figures move as M7 goes. Every figure is reproducible: the drawings come from a seed (`benches/generated-drawings/README.md`), the native numbers from `cargo bench -p cad-benchmarks --bench mixed`, the browser numbers from `pnpm --dir web bench:mixed`, which runs both browsers (`MIXED_FRAMES` sets the frames per run, `MIXED_ALL=1` includes the sizes Chromium skips). Machine: Intel i7-12650H, Linux under WSL2; headless Firefox renders with its software WebRender, headless Chromium with SwiftShader, a software rasteriser — **no figure here is from a GPU** (#110).

## The mixed drawing

Per hundred entities: 44 lines, 8 circles, 8 arcs, 10 polylines (one in three with an arc segment), 8 texts, 2 paragraphs, 8 block references, 8 dimensions, 4 hatches (`cad_benchmarks::MIXED_PER_HUNDRED`). What a frame draws, after the scene is built:

| Entities | Segments | Arcs | Triangles | DXF |
| --- | --- | --- | --- | --- |
| 10k | 142,784 | 4,170 | 13,600 | 3.2 MB |
| 50k | 755,136 | 20,989 | 68,000 | 16.2 MB |
| 100k | 1,567,688 | 42,079 | 136,000 | 32.5 MB |

Sixteen segments per entity: text is most of them — a text of six characters is some sixty strokes, and a dimension carries a text — which is what #175 measures and decides on.

## Native (2026-10-12, #171)

`cargo bench -p cad-benchmarks --bench mixed`, release.

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Open: read the DXF, build the drawing and its index | 17 ms | 88 ms | 178 ms |
| Build the scene: every entity to instances | 6.6 ms | 38 ms | 77 ms |
| One edit, then the scene rebuilt whole (ADR-0003; #171) | 6.5 ms | 35 ms | 69 ms |

The index is no longer most of an open (ADR-0007, amended): at 100k the packed tree takes 11 ms of the 178. The scene build was the cost #172 was for: every edit paid it whole.

### After #172 (2026-10-12)

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| A line in the middle moved: its chunk re-packed (ADR-0022) | 0.32 ms | 0.32 ms | 0.33 ms |
| A line drawn: the last chunk re-packed | 0.026 ms | 0.026 ms | 0.030 ms |
| The same edit, the scene rebuilt whole | 6.8 ms | 37 ms | 76 ms |

An edit now costs a chunk of 512 entities, whatever the drawing's size; the whole rebuild, which a layer switched on or off still pays, is a tenth slower than before for walking the ID ranges.

## In the browser (2026-10-12, #171)

`pnpm --dir web bench:mixed`: the drawing opened in the application's own engine and drawn by its own renderer, each frame timed from the input to the GPU finishing (`probeFrames`). *Pan* is a middle-button drag, *zoom* the wheel, *pointer* a move with nothing pressed (hover and snapping), *edit* the frame after one line is drawn, which rebuilds the scene. Mean and worst of sixty frames (ten for edits).

### Firefox, software WebRender

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Open (read the DXF into the engine) | 41 ms | 186 ms | 358 ms |
| First frame (pack, upload, draw) | 117 ms | 460 ms | 273 ms |
| Pan, mean / worst | 9.6 / 74 ms | 32 / 91 ms | 62 / 125 ms |
| Zoom, mean / worst | 5.0 / 7 ms | 20 / 32 ms | 39 / 59 ms |
| Pointer, mean / worst | 7.7 / 9 ms | 30 / 33 ms | 61 / 64 ms |
| Edit, mean / worst (#171, the scene rebuilt whole) | 24 / 33 ms | 123 / 145 ms | 218 / 286 ms |
| Edit, mean / worst (#172, one chunk re-packed and its ranges uploaded) | 11 / 24 ms | 42 / 86 ms | 79 / 152 ms |

### Chromium, SwiftShader

Sixty frames at 10k; at 50k and 100k the run is not attempted by default, a frame there taking tens of seconds (`MIXED_ALL=1` runs it).

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Open | 57 ms | — | — |
| First frame | 3.1 s | — | — |
| Pan, mean / worst | 3.1 / 3.3 s | — | — |
| Zoom, mean / worst | 1.3 / 3.0 s | — | — |
| Pointer, mean / worst | 3.1 / 4.0 s | — | — |
| Edit, mean / worst (#171) | 3.3 / 4.4 s | — | — |
| Edit, mean / worst (#172) | 3.2 / 3.2 s | — | — |

Three seconds a frame is SwiftShader rasterising 143,000 segments, 4,000 arcs, and 13,600 triangles: about 20 µs an instance, where Firefox's software renderer takes 60 ns and ADR-0009 measured 10k plain arcs at 175 ms in the same Chromium. The open, which is the engine alone, is the same as Firefox's.

## Against the targets

| Target | 10k effortless | 50k smooth | 100k usable |
| --- | --- | --- | --- |
| Open | Met everywhere: under 60 ms | Met: under 200 ms | Met: 358 ms in the browser |
| Pan, zoom, hover | Met in Firefox: frames under 10 ms | Frames of 20–32 ms in Firefox: smooth at 30 fps, not 60 | Frames of 40–60 ms in Firefox: usable; the software rasteriser's cost, 40 ns a segment, which only #110 can separate from the renderer's |
| Edit | Met: 11 ms | 42 ms: the pointer frame, which is the draw — met as far as editing is concerned (#172) | 79 ms: the same; what is left is the draw, the rasteriser's (#110) |
| On SwiftShader | A frame is seconds at every size: the rasteriser, not the renderer (10k plain lines take 260 ms there, ADR-0009 measured 10k arcs at 175 ms). Not a target this environment can judge | | |

What the figures asked for, in order: an edit that re-packs and re-uploads what changed (#172, done: the edit frame is now the pointer frame); then the share of text in the segment count (#175); culling only if a GPU's frames say so (#173, #110).
