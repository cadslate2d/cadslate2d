# ADR-0030: A viewport is an entity of a layout's paper, and the model's scene drawn again

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design (#192; the drawing of its contents lands with #193)  
**Design refs:** §12, §19; ADR-0003, ADR-0021, ADR-0022, ADR-0028, ADR-0029

## Context

A layout shows the model through viewports at a scale; without them a sheet is blank. ADR-0029 gave the drawing its spaces and kept a `VIEWPORT` as an opaque record. The render pipeline has one committed scene per drawing (ADR-0003, ADR-0022), drawn under one transform per region (ADR-0028).

## Decision

1. **A viewport is an entity** (`ViewportEntity`): a rectangle on paper — centre, width, height — the model point shown at its centre, the scale in paper units per model unit, and whether it is on. It is on a layout's paper and nowhere else: a transaction refuses one in the model or in a block (`ViewportOffPaper`), and one with no width, height, or scale (`BadViewport`). Its bounds are its frame; it is picked on its frame, not inside it, since the inside is the model's; its frame's corners, edge midpoints, and centre snap; its grips are its corners, which resize it about the opposite corner, and its centre, which moves it. It moves, scales, and is erased like any entity; a turn or a mirror moves its centre and leaves its frame square to the paper, as a sheet's windows are. It has no primitives: nothing trims against it, and it is never an edge. `MVIEW` makes one by two corners or fitted to the sheet's printable area, showing the model's extents at the scale that fits them.
2. **DXF both ways.** `VIEWPORT` is read for its frame, view centre, view height, and status, and written back with its number in the order written and the view straight down, untwisted, with no lens and no UCS, as writers lay it out; a kept `LAYOUT` object's active viewport (331) is the first viewport written in its block. A `VIEWPORT` in model space — a tiled viewport of an old file — is reported, since the drawing has no place for it.
3. **The session has a current space** (`Session::set_space`, `CommandContext::space`, `CommandContext::transaction`): what the pointer finds, what commands make, and what the selection keeps are the space's; `insert` goes into it. #193 gives it commands and tabs.
4. **How a viewport is drawn** (the plan #193 implements): a viewport's frame is walked like any entity, into the layout's scene. Its contents are the model's committed scene — the same buffers, uploaded once — drawn again under the viewport's own clip transform, from its view centre and scale through the paper's view, scissored to its frame on screen, before the layout's own scene so the sheet's entities lie over it. One call carries the viewports' frames and transforms, like the regions of ADR-0028; the model's scene is never packed a second time, and pan and zoom of the paper upload nothing. A viewport that is off draws only its frame.

## Alternatives considered

- **Keeping viewports opaque until layouts are drawn.** Then `MVIEW`, grips, and the fixture's viewport would have had nowhere to go; an entity now is the same work later.
- **A viewport as a block reference of the model.** The model is not a block (ADR-0017's `*Model_Space` cannot be referenced), and a reference's geometry is copied into the scene, where a viewport's is drawn from the scene the model already has.
- **Refusing rotation.** `Entity::transformed` returns an entity for every transform (DESIGN §22); a viewport that moved with the turn and stayed square is what a drafter who rotates a sheet's windows gets.

## Consequences

- The layouts fixture reads its viewport as an entity of `Layout1`, writes it back, and stays byte-stable; the LAYOUT's 331 follows it.
- Thirty-one `match`es gained an arm; the modify matrix gained a column.
- `ImportReport.viewports` and the kind "viewport" in the panel, `SELECT`, and `QSELECT`.

## Revisit when

- Layouts are shown (#193): §4 above becomes code, with a render-boundary test and a baseline.
- A viewport with a twist or a clipping boundary is met in a file: then the entity grows a twist and a clip path, and the walk draws them.
