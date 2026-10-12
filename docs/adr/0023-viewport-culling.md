# ADR-0023: Viewport culling — not taken up; the measurements that would reopen it

**Status:** Accepted (2026-10-12)  
**Date:** 2026-10-12  
**Kind:** Evidence (resolves §35 "viewport culling, if the measurements ask for it"; #173)  
**Design refs:** §12, §23, §35, §42; ADR-0003, ADR-0022

## Context

Every committed instance is drawn every frame, on screen or not. #171 measured the frames of the mixed drawings at the default view, where everything is on screen and culling could buy nothing. #173 asks what a frame costs zoomed in on a detail, where almost everything drawn is off screen, and whether that cost asks for drawing only the chunks (ADR-0022) whose bounds meet the view.

## The measurement

`pnpm --dir web bench:mixed`, Firefox with its software WebRender (no GPU in this environment: #110), 2026-10-12. *Zoomed in* is three full wheel steps about the middle of the view — e³, twenty times — so a four-hundredth of the drawing's area is on screen. Mean / worst of sixty frames.

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Instances drawn | 161k | 844k | 1.75M |
| Pan, default view | 9.2 / 87 ms | 31 / 96 ms | 62 / 128 ms |
| Pan, zoomed in | 4.6 / 7 ms | 20 / 21 ms | 38 / 40 ms |
| Pointer, default view | 6.7 / 8 ms | 30 / 31 ms | 60 / 62 ms |
| Pointer, zoomed in | 4.9 / 7 ms | 20 / 21 ms | 38 / 40 ms |

Zoomed in, the frame is a third cheaper and no cheaper than that: 38 ms at 100k with a four-hundredth of the drawing on screen, 20 ms at 50k, 4.6 ms at 10k — 23 ns an instance, linear in the instances drawn and independent of what is visible. That is the software rasteriser's per-instance work — the vertex stage of 1.75 million instances — and the other 22 ms at the default view is the rasterising of what is on screen, which no culling removes.

## Decision

Culling does not ship in M7.

1. **The numbers do not ask for it.** §42 asks for 100k "usable" and 50k "smooth for normal editing". Zoomed in, where culling would apply, the frames are the cheaper ones: 38 ms at 100k is usable, 20 ms at 50k is a frame short of smooth, and both are on a software rasteriser that a GPU's vertex stage outruns by orders of magnitude. On hardware the floor culling would attack is a few milliseconds at most; the hardware figures are #110's, and until they exist the software ones say the frame is not where the time goes.
2. **Chunk culling would cut nothing from the drawings measured.** A chunk is 512 entity IDs (ADR-0022), and the mixed drawings place entities at random, so every chunk's bounds are the whole drawing and no chunk is ever off screen. Culling by chunk pays off only for a drawing whose ID order follows its layout — one drawn region by region — and that is a property of real drawings nobody has measured yet.
3. **What it would cost is known and small, so deferring loses nothing.** The scene would report a bound per chunk and each chunk's first instance per kind — two coarse calls, a few kilobytes — and the adapter would test two hundred boxes against the view each frame and draw the runs that meet it. Nothing committed would be regenerated or re-uploaded on pan or zoom (ADR-0003), since the choice is made in TypeScript from bounds the scene already has. It fits in a day when the numbers ask for it.

The probe keeps its zoomed-in runs (`FramesReport.zoomedIn`), so the figure that would reopen this is measured on every bench run.

## Alternatives considered

- **Culling by chunk now, as insurance.** Measured as doing nothing for the benchmark drawings, and the frame cost it would attack is the software rasteriser's. Code that cannot be shown to help is not kept.
- **Spatial tiles instead of ID chunks.** Would cull on any drawing, but needs a second grouping of the instances beside the one an edit re-packs (ADR-0022), and a moved entity changes tiles. If culling is wanted, the first thing to try is ordering the IDs — a chunk's bounds shrink when its entities are near one another — not a second structure.
- **Culling in Rust, in the scene.** Would make the scene depend on the view, against ADR-0003. Rejected before measurement.

## Revisit when

- A GPU's frames (#110) show the zoomed-in frame bound by the instance count rather than the pixels, at a size the targets cover.
- Real drawings turn out to be drawn region by region, so chunk bounds are small: measure a drawing of that shape zoomed in, before and after culling, and ship if the frame halves.
