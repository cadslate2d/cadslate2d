# ADR-0032: The drawing order is a tier on each record, drawn as depth

**Status:** Accepted (2026-10-15)  
**Date:** 2026-10-15  
**Kind:** Design (#226)  
**Design refs:** §12, §16, §19, §24; ADR-0003, ADR-0014, ADR-0021, ADR-0022

## Context

Until now the committed scene has had one fixed order: every fill beneath every stroke (ADR-0021), and objects of a kind in the order of their IDs, chunk by chunk (ADR-0022). A drafter cannot bring a text above a hatch, make a solid fill hide the lines beneath it, or mask a note's background. Classic programs answer with `DRAWORDER`, `TEXTTOFRONT`, and `WIPEOUT`, and DXF keeps the order in a `SORTENTSTABLE`.

Two rules constrain any answer:
- **Batching by style (ADR-0014).** The committed scene is a few whole buffers, one style index per instance, drawn in a handful of calls. A draw order must not split it into a call per object, or per run of objects.
- **Chunks (ADR-0022).** An edit re-packs the chunks of 512 IDs that hold what it changed, and uploads only their ranges. A change of order must touch only the chunks of what it moves, and a pointer move must still upload nothing.

## Decision

1. **A tier on the record.** Every `EntityRecord` carries `order: i16`, its tier in the drawing order. Like the space (#191), it sits beside the geometry and the properties; a change of tier is a `Replace` of the record, in the one history. `Drawing` counts its entities per tier, so its top and bottom tier are known without a scan.
2. **What a tier means.** An object of a higher tier is painted over one of a lower. Within a tier, areas — a solid hatch, a wipeout, a wide polyline (`Entity::is_fill`) — are beneath strokes, as ADR-0021 has always drawn them, and of two areas, or two strokes, the later made is over the earlier. So a drawing nobody reorders is drawn exactly as before. `draw_order::paint_key` is that order, `(tier, area first, ID)`, and the page is painted in it.
3. **New objects go on top.** A new object takes the drawing's top tier, so it is drawn over everything below it and, being a stroke or a later area, over the top tier's own areas. An object made *from* another — a copy, an offset, a piece — takes that one's tier, as it takes its properties (`insert_like`). A wipeout is made a tier above the top (`insert_on_top`), so it hides what is there, and the note written next lies over it.
4. **`DRAWORDER`.** `Front` and `Back` put the selection past everything else; `Above objects` and `Under objects` just past the highest or lowest tier of the reference objects, moving what lay beyond by as many tiers as the selection spans when, and only when, it is in the way. The selection keeps its own order. A pure function, `draw_order::reordered`, decides every tier; when the `i16` range would run out, every object is renumbered from the middle in the same order — once in 32,767 moves to the front, and it re-packs everything. `TEXTTOFRONT` does `Front` for every text, dimension, or leader.
5. **The scene carries the tier beside the style.** The per-instance style index becomes a 32-bit style word: the index in its low half, the tier in its high half. No new buffer, no new boundary call: the three views that crossed as `Uint16Array` cross as `Uint32Array`, and a chunk's words are re-packed with its instances. A wipeout's triangles take a style row of a new kind, *background*, whose colour the adapter supplies from the theme, as it does the foreground's.
6. **The adapter draws the tier as depth.** The vertex shaders read the word as one unsigned integer attribute, take the index from its low half and the tier, sign and all, from its high half, and write the tier as the clip-space depth, nearer for a higher tier. The committed scene is drawn with the depth test `LEQUAL`: fills write depth, strokes only test it. Fills are drawn first, as before, then strokes. So a fill hides whatever of a lower tier lies beneath it whatever order the instances arrive in, a stroke is drawn unless a fill of a higher tier covers it, and a stroke's antialiased edge hides nothing. Each viewport's model and then the paper's own objects begin with the depth cleared, so the paper lies over the viewports. The overlay is drawn without a depth test, in its own order, as before.
7. **The file.** A reader paints a space's records in the order it reads them, unless a `SORTENTSTABLE` says otherwise. So the writer writes each space's records in the order they are painted, `paint_key` order, and needs no table: a drawing nobody reorders, with no area made after a stroke, is written in ID order as before. Reading, each space's entities are sorted by their sort handle — their own where no table names one — and `draw_order::tiers_of` puts them in the fewest tiers that paint them in that sequence: a solid hatch or a wipeout after a line in the file is over it, as its writer meant. Write, read, write: the same records in the same order.

## Evidence

Natively, `cargo bench -p cad-benchmarks --bench mixed`, the 100k mixed drawing, this machine (Intel i7-12650H, WSL2), 2026-10-15. The scene's numbers before are the commit before this one, run the same hour.

| 100k mixed | Before | After |
| --- | --- | --- |
| Open: read the DXF, build the drawing | 249 ms | 223 ms |
| Build the scene whole | 94.7 ms | 85.6 ms |
| A line moved: its chunk re-packed (ADR-0022) | — | 0.37 ms |
| A line brought to the front: its chunk re-packed | — | 0.37 ms |
| Every hatch, 4,000, to the back: nearly every chunk re-packed | — | 81 ms |

At 10k and 50k a line to the front is 0.33 ms, as an edit is, and every hatch to the back 7.4 and 39 ms. The open and the scene build are the same within this machine's run-to-run spread, which is a tenth.

In Firefox (`bench:mixed`), each build run one after the other, the same hour:

| 100k mixed, mean of 60 frames | Before | Tier as a second attribute | Tier in one integer attribute (decided) |
| --- | --- | --- | --- |
| Pan | 63.9 ms | 68.7 ms | 60.0 ms |
| Zoom | 41.0 ms | 46.0 ms | 38.2 ms |
| Pointer | 62.1 ms | 67.1 ms | 58.8 ms |
| Pan, zoomed in | 40.4 ms | 47.7 ms | 36.5 ms |
| Pointer, zoomed in | 40.4 ms | 47.7 ms | 36.4 ms |
| Edit (one chunk re-packed and uploaded) | 82.1 ms | 94.4 ms | 79.0 ms |

The bench's drawing was written before this change, its hatches after the lines around them, so it opens with some 2,000 tiers: the depth test is doing real work. The first version read the word's halves as two float attributes, and cost five to seven milliseconds a frame; with the depth test switched off it cost the same, so the cost was the second attribute, not the test. Read as one integer, the word costs nothing measurable — the frames are a little quicker than before, which had a float conversion of its own.

What the numbers say:
- **A pointer move uploads nothing**, and a pan or a zoom: the depth is in the instances already, not computed per frame (`the_drawing_order_crosses_in_the_style_words_and_repacks_only_what_moved` checks the scene's dirty ranges stay empty while the pointer moves, `web/e2e/canvas.spec.ts` that no frame uploads committed geometry).
- **An order change re-packs only what it touches.** One object to the front re-packs its one chunk, as an edit does, and uploads its chunk's words alone; the render-boundary test checks that the only words that change are its own. Every hatch of the 100k drawing to the back touches nearly every chunk, and costs what a whole rebuild does — the most an order change can cost.
- **The style word costs two bytes an instance**: 3.5 MB more on the 100k drawing's 1.75 million instances, in WASM memory and on the GPU, and nothing measurable a frame.
- **The file costs nothing**: written in paint order, the 100k drawing's DXF is the same size, byte for byte, and opens in the same time.

## Alternatives considered

- **A pass per tier.** Instances grouped by tier inside each chunk, one draw call per tier per region. Exact with no depth buffer, but the calls multiply with the tiers in use, and a file whose order is a long interleaving of fills and strokes — the import of `ezdxf`'s and AutoCAD's own files — has thousands of tiers. Batching by style (ADR-0014) is what this would give up.
- **The scene sorted by the order.** Instances packed in paint order, not ID order. One draw call per kind, exact — but a change of order moves an object across the buffer, and with it every instance between: the scene re-packed whole, which ADR-0022 exists to avoid.
- **The tier in the style table.** A row per (style, tier), the depth a column of the table. No new per-instance data — but the rows multiply with the tiers, past the `u16` index a file of interleaved fills and strokes needs, and the table is append-only, so every reorder would grow it.
- **A depth per object, by ID.** Depth from `(tier, ID)` would paint every object exactly in made order, fills and strokes interleaved as DXF does. It needs more depth precision than a 24-bit buffer has once IDs pass a few thousand per tier, and strokes would have to write depth, which makes their antialiased edges cut holes in the lines beneath.
- **Writing a `SORTENTSTABLE`.** The records in ID order, and a table pairing each one painted out of place with the handle of the one at its place. Built first, and measured: an area made after a stroke in the same tier — every hatch drawn inside its boundary — is out of place, and so is everything between it and where it is painted, so the 100k mixed drawing's table held most of its entities: 2 MB more file, and an export and an open each 36 ms slower. The records in paint order say the same with nothing added, and a reader that ignores the table — many do — paints them right.
- **A table of the reordered objects (`SORTENTSTABLE` in the document).** Only the objects moved would carry an order, the rest following their IDs. It needs a rule for where new objects go relative to the moved ones, and every renderer and the plot would merge two orders; a tier on every record says it once.

## Consequences

- A drawing nobody reorders is drawn and plotted as before.
- On screen, two strokes of different tiers that cross are drawn in instance order, not tier order: the depth test orders fills against everything, strokes only against fills. One pixel at a crossing of two lines of different colours may show the lower one. A text over a line is still readable; nothing hides behind a stroke.
- Files: a drawing whose solid hatches were made after the lines they lie on is written with the hatches first, so that other programs paint them beneath, as we do; IDs, which no file keeps, come back in that order. A file from another program whose hatches lie over earlier lines is drawn with them over the lines, which before this decision it was not.
- Objects in a block definition are painted in the definition's order; a block's own `SORTENTSTABLE` is reported.

## Revisit when

- A drafter needs two crossing strokes ordered to the pixel: strokes writing depth, with their edges discarded below half coverage.
- A real GPU (#110) shows the depth test costs a frame something measurable.
