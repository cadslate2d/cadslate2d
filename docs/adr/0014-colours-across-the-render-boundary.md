# ADR-0014: Colours across the render boundary — a style index per instance and a style table

**Status:** Accepted (2026-10-05)  
**Date:** 2026-10-05  
**Kind:** Evidence (extends ADR-0008's layout for committed geometry; changes no LOCKED decision)  
**Design refs:** §7.1, §12.2–12.4, §12.6, §23, §32

## Context

Until now every committed entity was drawn in one colour. A frame's instances were grouped into *batches* — runs of consecutive instances with the same style role — and the adapter issued one draw call per batch with the role's colour in a uniform. That is the right shape for the overlay, where roles are few and decided by Rust (selection, preview, snap marker).

With layers, committed geometry has colours of its own: each entity is on a layer, the layer has a colour, and an entity may override it (ADR-0013). Three things must stay true:

- pointer movement, pan, and zoom upload no committed geometry (ADR-0003);
- data crosses the boundary in a few whole buffers, never per entity (ADR-0002);
- theme colours are not hard-coded in Rust (§7.1) — and colour 7 is a theme colour: light on a dark canvas, dark on a light one.

## The candidates

1. **Batches.** Keep one draw call per run of instances with the same colour, the colour in a uniform.
2. **A colour per instance.** Four bytes of colour beside each instance; one draw call.
3. **A style index per instance and a style table.** Two bytes beside each instance: the row of a small table that says what the instance is drawn with. The vertex shader looks the row up in a texture; one draw call.

## Evidence

`web/src/renderer/webgl/style-probe.ts` draws the same generated segments each way in its own WebGL2 context and times them (`pnpm --dir web bench:styles`, after a build). Measured in headless Chromium with software WebGL — the only renderer available here — so frame times are dominated by rasterisation and say little about a GPU; they are given for completeness. What differs between the candidates is what the table shows.

100,000 segments on a 1024 × 1024 canvas:

| Styles, order | Candidate | Bytes per instance | Draw calls | Frame | Recolour one style |
| --- | --- | --- | --- | --- | --- |
| 8, grouped | batches | 16 | 8 | 1.92 s | nothing to upload |
| | colour | 20 | 1 | 1.87 s | 3.0 ms |
| | table | 18 | 1 | 2.01 s | 0.19 ms |
| 8, as drawn | batches | 16 | 87,624 | 2.31 s | nothing to upload |
| | colour | 20 | 1 | 1.99 s | 3.4 ms |
| | table | 18 | 1 | 2.03 s | 0.20 ms |
| 256, as drawn | batches | 16 | 99,572 | 2.35 s | nothing to upload |
| | colour | 20 | 1 | 1.89 s | 2.9 ms |
| | table | 18 | 1 | 1.94 s | 0.17 ms |

"Grouped" means all instances of a style are contiguous; "as drawn" means styles are interleaved the way entities on several layers are in the order they were drawn.

What the measurement establishes:

- **Batches do not survive real drawings.** Entities are stored in the order they were made. With colours interleaved, nearly every instance is its own batch: 87,000 to 100,000 draw calls. Even in software, where a draw call costs almost nothing beside rasterising, that is 15–20 % slower; on a GPU, draw-call overhead is the cost. The alternative — sorting instances by colour — changes which line is drawn over which, and has to be redone whenever a colour changes.
- **Colour and table draw at the same speed**, within the noise of the run (±5 %).
- **Recolouring is fifteen times cheaper with a table** and does not grow with the drawing: a table of a few rows against a buffer with an entry per instance.
- **The table is the smaller of the two** per instance.

Not established: frame time on a real GPU, as in ADR-0008 and ADR-0009.

## Decision

### 1. Every committed instance carries a style index; styles are rows of a table

The committed scene gains, beside each instance buffer, a buffer of one `u16` per instance: the index of its style. The scene also has a **style table**, one row per distinct `Properties` value among the entities shown, in the order they are first met.

The geometry buffers and their layout are unchanged (ADR-0008, ADR-0009). The indices travel in buffers of their own, so the overlay — whose instances have no style index — keeps its layout too.

### 2. A row is four bytes: red, green, blue, kind

`kind` is 255 for "this colour" and 0 for "the foreground", in which case the first three bytes mean nothing. Rust resolves `ByLayer` and `ByBlock` (ADR-0013) and looks the index colour up in the palette; the adapter turns a foreground row into the theme's colour for ordinary geometry. So colour 7 follows the theme and Rust still names no theme colour.

The index palette — the red, green, and blue of colours 1 to 255 — is data in `cad-document`, taken from the table ezdxf 1.4.4 ships.

### 3. Two lifetimes inside the committed scene

| Part | Depends on | Rebuilt and uploaded when |
| --- | --- | --- |
| Instances and their style indices | The entities shown, their geometry and properties | `Drawing::shown_revision` changes: an entity inserted, removed, or replaced, or a layer switched on or off |
| The style table | The above, and the layers | `Drawing::revision` changes: anything at all |

A layer renamed, recoloured, or locked changes the table and none of the geometry. The table is a few bytes per style; uploading it after every edit costs less than deciding whether it changed.

Objects on a layer that is off produce no instances, as before; switching a layer on or off rebuilds the geometry.

### 4. The shaders look the style up; the overlay does not

Both programs — strokes and arcs — take the style index as a third instanced attribute and the table as a texture, and a uniform says which of the two a draw call uses: the table, for committed geometry, or the colour uniform set per batch, for the overlay. The table texture is 1024 texels wide and as many rows as needed; the index is read by the shader as a float, which is exact for all 65,536 values.

Committed geometry is still drawn through its batches — now one batch per buffer — and its lineweight still comes from the theme's entity role.

### 5. Across the boundary

Four exports are added, each one whole buffer or one number: `scene_segment_styles`, `scene_arc_styles`, `scene_styles`, and `scene_style_revision`. `scene_revision` now identifies the geometry alone. A steady-state frame makes one more call than before — the second revision check — and still a constant number, whatever the drawing holds.

## Alternatives considered

- **Batches, with instances sorted by colour.** Rejected: see the evidence. It also turns "recolour a layer" into "rebuild and re-upload the scene".
- **A colour per instance.** Workable and the simplest shader. Rejected because every further property — lineweight, linetype — would be another per-instance attribute and another reason to re-upload everything, where a table row simply grows.
- **A float texture for the table**, so that a row can hold a width directly. Deferred: RGBA8 is enough for colour, and the row layout is private to the two sides of one function call. Lineweights will choose.
- **Resolving the foreground in Rust**, by passing the theme's colour in. Rejected: it would make the committed scene depend on the theme, and a theme switch would be a document-side rebuild.
- **Hiding off layers through the table** — an invisible style — instead of dropping their instances. Tempting, because switching a layer would then upload nothing. Rejected for now: hidden instances would still be processed for every frame, and "off means not there" is one rule with picking and snapping. Revisit if toggling layers on large drawings is slow.

## Consequences

- Layers are drawn in their colours, and an entity's own colour will be when it can be given one.
- One draw call draws all committed segments and one all committed arcs, in creation order, in any number of colours.
- The style index is the hook for lineweight and linetype: rows gain fields; instances do not change.
- At most 65,536 distinct combinations of properties are distinguished; beyond that the surplus share the last row. A drawing would need that many layer-and-override combinations in use at once.
- A context loss drops the table with everything else; the adapter forgets both revisions and the next frame uploads both.

## Revisit when

- Rendering is measured on a real GPU.
- Lineweights and linetypes arrive: the row format is decided there.
- Layer toggling on large drawings needs to be faster than a geometry rebuild.
