# ADR-0025: Layouts are kept opaque through a round trip, not drawn

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design (#184)  
**Design refs:** §24.2, §36; ADR-0005, ADR-0018

## Context

Files from other programs carry paper space: one or more layouts, each a `LAYOUT` object naming a `BLOCK_RECORD`, whose block (`*Paper_Space`, `*Paper_Space0`, …) holds the title block, the viewports, and whatever else was drawn on the sheet. The drawing holds model space only. Until #184 the reader skipped paper-space blocks and the `OBJECTS` section, so a file opened and saved lost its sheets, and the status bar said LOSSY for them. Drawing a layout is a different product feature — a second space with its own view, viewports that render model space through a window, plotting — and is not what M8 asks for. Losing them need not wait for that.

## Decision

### 1. What is kept, and as what

The reader keeps, as the file's own groups, three things it does not interpret: every `LAYOUT` object but the model's, every `BLOCK_RECORD`'s handle and name (only to tie a layout to its block), and every entity of a paper-space block (`VIEWPORT` included), as its type name and its groups. They live on the drawing as `KeptLayout`s (`cad-document/src/kept.rs`): a name, a block name, the record handle the file used, the layout's groups, the block's entities. They are not entities, not in the spatial index, not in the history — nothing edits them — and they are not drawn. The Model tab is the only tab.

The reader resolves nothing in them. A kept entity is never counted as unsupported, and a layout with something on its paper is named in the import report ("Kept 2 layouts, not drawn: Layout1, Layout2.") and in a status-bar chip. A layout with nothing on its paper — every file has an empty default — is kept without a word.

### 2. How they are written back

The writer writes its own structure as before — the dictionary, the layouts dictionary, `*Model_Space` and its `Model` layout — and then the kept layouts in their tab order, each as a `BLOCK_RECORD`, a `BLOCK` … `ENDBLK` with the kept entities, and a `LAYOUT` under the layouts dictionary. The default `*Paper_Space` and `Layout1` are written only when nothing was kept.

Handles are the writer's, never the file's (ADR-0018 writes every handle in order). Each kept record gets a new handle; the file's old handle maps to it, and every reference group (330 to 369) in a kept record is rewritten through that map: the layout's 330 to the block record the writer wrote, an entity's 330 to its block record, a layout's 331 (the active viewport) to the viewport's new handle. A reference to something the writer does not have — a plot style, a dictionary the file had and we did not keep — is written as `0`, which a reader treats as none. Everything else is copied verbatim, so a `VIEWPORT`'s view, a title block's text, and a plot setting pass through untouched.

The first kept layout takes the handles the default `Layout1` would have, so a file of ours, read back, is written identically: export → import → export stays byte-for-byte stable for every fixture and corpus file, kept layouts included (`every_valid_fixture_round_trips_stably()`).

### 3. What drawing a layout would need

A layout is not a block to be rendered: it is a sheet of a different size, in different units (`$PLIMMAX`, the plot settings' paper size), with its own view, and its viewports are windows onto model space with a scale and a centre each. Drawing one needs a second space in the document — a `Space` the session is in, with its own entity store, view, and selection — a `Viewport` entity that renders the model's scene clipped to a rectangle at a scale, a tab strip, and a notion of plotting for it to be worth anything. That is a milestone of its own, and these kept records are the input to it: a `KeptLayout` holds every group that work would read.

## Alternatives considered

- **Parse viewports and paper entities into the document now.** They would be entities on a space the drawing has no concept of; every command and the renderer would have to know not to touch them. Opaque groups are smaller and cannot be wrong.
- **Keep the whole `OBJECTS` section verbatim.** Dictionaries, plot styles, and extension dictionaries reference each other by handle; writing them back next to ours would need the same remapping for a great deal more, and nothing in them matters to the drawing. Only the layouts are kept.
- **Keep the file's handles.** Would make every one of our handles depend on the file's, breaking the writer's determinism and the one-pass handle allocation of ADR-0018.

## Consequences

- A file with layouts and viewports is no longer LOSSY for them, and a round trip keeps them (`layouts_are_kept_through_a_round_trip()`, `web/e2e/layouts.spec.ts`).
- The reader now reads the `OBJECTS` section (for `LAYOUT`) and the paper-space blocks, which it used to skip; a hostile file's groups there are bounded by the same limits as everywhere else.
- A kept record whose references point outside what is kept is written with `0` references. That is a loss a strict reader could notice; it is not reported, since nothing drawn or plotted depends on it.
- A new drawing, and a drawing from a file with no layouts, write the same bytes as before (`an_empty_drawing_is_a_complete_file_with_its_permanent_rows()`).

## Revisit when

- Layouts are to be drawn or plotted: then `KeptLayout` becomes the import of a real space (§3 above), and this ADR is superseded.
- A file is found whose kept records are not written back readably by its own program — the remap rule (`0` for the unknown) would then need to keep more.

> **Narrowed in M9 (#191):** a layout is a row of the drawing's layout table and a file's paper-space entities are entities of their layout ([ADR-0029](0029-spaces.md)); what this ADR keeps opaque is now only the `LAYOUT` object's groups and the records of a sheet the drawing has no entity for, on the layout's row.
