# ADR-0028: Each scene chunk is packed against an origin within reach of its geometry

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design, with evidence (#188)  
**Design refs:** §12, §23.4; ADR-0002, ADR-0003, ADR-0022

## Context

The committed scene had one origin, the centre of the drawing's extents, and every instance was `f32` offsets from it (ADR-0003). A drawing wholly far from the world origin was exact; a drawing that *spans* ten million units — a title block at the origin, a site at 10⁷ — was placed to a unit at its far end, which is a hundred pixels at a hundred pixels per unit (ADR-0003, extended for #151). The scene was already packed in chunks of 512 entity IDs (ADR-0022), and the extension named the fix: a chunk can carry an origin of its own.

## Decision

1. **A chunk is packed against the origin of the chunk before it while that origin is within reach of all of its geometry, and against its own extents' centre otherwise.** Within reach is every corner of the chunk's extents within `MAX_ORIGIN_OFFSET` = 100,000 units of the origin on both axes: there an `f32` offset resolves to under a hundredth of a unit, a pixel at a hundred pixels per unit. A drawing of ordinary size — the benchmark drawings span ten and thirty thousand units — is therefore packed as before, against one origin; a drawing that spans ten million units has an origin per end.
2. **A repacked chunk keeps its origin while its geometry stays within reach of it**, so an edit leaves what was there where it was, as ADR-0022 promised; a chunk whose geometry is drawn out of reach takes the previous chunk's origin if that fits, else its own centre, and its whole run is re-uploaded as it would be anyway.
3. **Consecutive chunks with the same origin are one region**, and the adapter draws each region's run of each kind under the region's own clip transform. Two calls carry it: `scene_regions`, read with the scene buffers when the revision changes (`segment_first, segment_count, arc_first, arc_count, triangle_first, triangle_count` per region), and `scene_transform`, which now returns four floats per region each frame instead of four in all. The calls per frame stay what they were (ADR-0002); the buffers stay one run per kind; the dirty ranges are untouched. An ordinary drawing is one region and three draws, as before.

## Evidence

The spanning drawing — 511 lines at the origin filling the first chunk, a plot at 10⁷ in the second — has two regions, the second's origin at the plot's centre, and the plot's instances placed to under a hundredth of a unit (`chunks_out_of_reach_of_the_previous_origin_take_their_own_and_make_a_region`, `regions_tile_the_buffers_and_an_ordinary_drawing_is_one`). End to end, zoomed to about a hundred pixels per unit on the plot, its horizontal line lies on the centre row to a pixel, with nothing in the rows a pixel-off placement would light (`web/e2e/far-from-origin.spec.ts`).

Pan and zoom still upload nothing: the scene revision and the build count are unchanged by pointer movement (`pointer_only_updates_do_not_change_the_scene_revision`, and the new boundary test). The edit figures of ADR-0022 hold. Native, `cargo bench -p cad-benchmarks --bench mixed`, release, the same drawings, 2026-10-13:

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| A line in the middle moved, its chunk re-packed (ADR-0022: 0.32 / 0.32 / 0.33 ms) | 0.30 ms | 0.29 ms | 0.33 ms |
| A line drawn, the last chunk re-packed (ADR-0022: 0.026 / 0.026 / 0.030 ms) | 0.025 ms | 0.026 ms | 0.026 ms |
| The same edit, the scene rebuilt whole (ADR-0022: 6.8 / 37 / 76 ms) | 7.0 ms | 37 ms | 75 ms |

The reach check reads the offsets just packed and costs nothing measurable; a first version that read every entity's bounds before packing cost a fifth more on a repack and a rebuild, and was replaced.

In Firefox (software WebRender), `pnpm --dir web bench:mixed`, mean / worst of the frames, measured the same hour on the commit before this change and on it — the machine was slower that day than when ADR-0022 was measured (its pointer frames were 6.7 / 30 / 60 ms), so the comparison that holds is the one between the two columns:

| | 10k before / after | 50k before / after | 100k before / after |
| --- | --- | --- | --- |
| Pointer | 13 / 13 ms | 57 / 52 ms | 138 / 168 ms |
| Edit | 17 / 17 ms | 73 / 71 ms | 123 / 142 ms |
| Pan | 14 / 15 ms | 50 / 49 ms | 133 / 124 ms |
| Pointer, zoomed in twenty times | 7 / 8 ms | 20 / 20 ms | 41 / 42 ms |

The edit frame is still the pointer frame plus one chunk's upload; the 100k column moves both ways between runs by the amount it moves between the commits. An ordinary drawing is one region, so the draw is the same three calls per kind as before.

## Alternatives considered

- **Chunking by locality as well as ID.** Would let a chunk that itself spans ten million units — a single line from the title to the site — be exact at both ends; ADR-0022 chose ID chunks for editing, and a chunk's own span is halved by centring its origin. Not taken up: no drawing of that kind has been asked for.
- **One origin per chunk always.** Two hundred regions and six hundred draws a frame at 100k, for drawings that need none of it.
- **Rebasing the whole scene at the view.** What ADR-0003 deferred; it would re-pack everything on every pan.

## Consequences

- The spanning drawing is placed to a pixel at a hundred pixels per unit; ADR-0003's extension is superseded for that case.
- One more coarse call in the boundary snapshot; `scene_transform` is longer by four floats per extra region.
- Packing a chunk now reads its extents first: the bounds of each shown entity, before its geometry. The native repack and rebuild figures are in the table above.

## Revisit when

- A chunk's own geometry spans more than `MAX_ORIGIN_OFFSET` and is looked at closely at both ends: then chunking by locality.
- The draw count of a many-region drawing shows in a frame: then regions could merge chunks whose origins are within reach of each other, not only consecutive ones.
