# ADR-0029: Every entity is in a space; the model is the first layout

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design (#191)  
**Design refs:** §19, §24; ADR-0013, ADR-0018, ADR-0025 (narrowed)

## Context

The drawing held model space only. A file's layouts were kept as opaque records (ADR-0025): not lost, but not drawn, not edited, and not the entities they are. Drawing a layout needs a second space in the document with its own entities, extents, and queries, and a row to hang a name and a sheet on. The visual identity shows layout tabs.

## Decision

1. **A layout table, the model its first row.** `Layout` is a row of `Table<Layout>` like a layer or a text style: a name, and what the file said of it that the drawing does not read — the `LAYOUT` object's groups, the paper block's name and record handle, and the records of its paper the drawing has no entity for (a `VIEWPORT`), kept as they were (ADR-0025 narrowed to those). Row 0 is `Model`, permanent: it cannot be renamed or removed. Adding, renaming, and removing a layout are `Change::Layout`s in the one history; removing a layout removes what is on it in the same transaction, undone together.
2. **A space on every entity record.** `EntityRecord` carries `space: LayoutId` beside its properties, `MODEL` unless told otherwise; `with_entity` and `with_properties` keep it, `insert_like` copies it, and `set_space` moves an entity between spaces as one change. The space is not a property (ADR-0013): it is where the entity is, not how it looks, and the scene's style key is unchanged.
3. **The model's reads are the defaults.** `shown`, `query_shown`, and `extents` read the model, as every caller meant; `shown_in`, `query_shown_in`, `entities_in`, `extents_in`, and `is_shown_in` take a space. One spatial index holds every space, and a query filters by space with its result: a layout holds a sheet's worth of entities, and nothing crosses the boundary (ADR-0002) per space. `entities`, `records`, `len`, and the in-use checks of the tables stay document-wide: a layer with a title block on a sheet is in use. The committed scene packs the model's entities only.
4. **DXF both ways.** A paper-space block's records are read through the entity path like the model's, with the layout the `LAYOUT` object names for the block (by its record's handle), in tab order; a record of the kinds the drawing has no entity for is kept on the row, not reported. An entity the ENTITIES section flags as paper (67) is the active layout's, `*Paper_Space`; a paper block with something on it that no `LAYOUT` object names — an R12 file's — becomes a layout of its own. The writer puts each layout's entities in its paper block, flagged 67, kept records after them, and writes the `LAYOUT` object the file gave it under the name it has now, or a sheet of its own for a layout made here; the ENTITIES section is the model's. The first layout takes the handles a drawing without any gives its `Layout1`, so our own files stay byte-stable; a drawing with no layouts writes exactly what it did.
5. **The session's current space is the model** until #193: the pointer, the commands, and the scene read the model. The report and the status bar name the layouts with something on them as "not drawn yet".

## Alternatives considered

- **A `Space` enum beside a layout table.** Two names for one thing; the model as row 0 of the same table is what every other permanent row (`0`, `Continuous`, `Standard`, `*Model_Space`) already does.
- **An index per space.** Faster queries on a drawing with a large sheet beside a large model; a filter on one index is simpler, and a sheet is small. Revisit if measured.
- **Keeping the opaque path whole beside the new one.** Two representations of a line on a sheet; the opaque path is narrowed to what the drawing cannot represent.

## Consequences

- The history property test gains the layout operations and the per-space invariants; a removed layout's entities come back with it on undo.
- The layouts fixture reads as three entities in three spaces; `every_valid_fixture_round_trips_stably` and the corpus hold.
- `Drawing::from_spaces` builds a drawing with layouts and entities in them; `from_tables` and `from_parts` build the model as before.
- ADR-0025's "Revisit when" has come: it is superseded for entities, and stands for the records kept on a row.

## Revisit when

- Viewports become entities (#192): the kept records narrow further, to what no reader of ours knows.
- A layout's queries are measured slow beside a large model: an index per space.
