# ADR-0017: Blocks — definitions as table rows, references resolved through the table

**Status:** Accepted (2026-10-06)  
**Date:** 2026-10-06  
**Kind:** Bootstrap (settles the model DESIGN §19.6 constrains before any block code; no LOCKED item changes)  
**Design refs:** §11, §19.2, §19.6, §32; ADR-0013

## Context

A block is the first thing in the document that contains other entities. Four questions have to be answered before one is written, and §19.6 answers the shape of one of them in advance: redefining a block is **one** semantic change, never one change per reference, though committing, undoing, or redoing it may invalidate derived state for every reference.

1. How does a definition store its entities, and what are their identities?
2. How is nesting represented, and how are cycles refused?
3. What do `ByBlock` and layer `0` mean inside a definition?
4. How is derived state — bounds, the spatial index, render caches — kept right for every reference when a definition changes, within the one-change rule?

## Decision

### 1. A definition is a row; its entities are records without IDs

`Block` is a row of `BlockTable`, the fourth table (ADR-0013): a name, a base point, and `Arc<Vec<EntityRecord>>` — geometry with properties, exactly what the drawing stores, shared so that a redefinition's history entry copies nothing. The entities of a definition have **no `EntityId`**: they are not in the drawing, only in the definition, and are addressed by position. Editing inside a block is not a thing here; `EXPLODE` (#148) makes drawing entities, with fresh IDs, from the placed geometry. Row 0 is `*Model_Space` — the drawing itself, which no reference may place and no transaction may remove — so that a default `BlockId` names a row, as for every table.

### 2. A reference holds an ID and a placement, and no geometry

`BlockReferenceEntity` is an `Entity` holding the row's ID, an insertion point, a uniform scale, a rotation, and whether it is mirrored (a negative X scale, in DXF's terms, so that a mirrored reference stays a reference). Every question about its geometry — bounds, distance for picking, primitives for snapping and intersection, what crosses a selection box, what the renderer draws — is answered **through the table**: `Entity::bounds(&self, blocks)` and its siblings take the block table, and for a reference flatten the definition's entities, placed, nested definitions included. Nothing derived is copied into the reference, so there is nothing in it to go stale.

### 3. Nesting is by reference; cycles are refused when a block is defined

A definition may hold references to other blocks, and those to others, and so on: `BlockTable::flatten` follows them (with a depth guard of 32, a guard and not a rule). A definition that would hold itself, directly or through other blocks, is refused when it is defined or redefined — `TransactionError::BlockCycle` — by `BlockTable::would_cycle`, which asks whether any block the new contents reference reaches the block being defined. A block that is placed, in the drawing or inside another definition, cannot be removed.

### 4. Layer `0` and `ByBlock` mean the reference's own

When a definition's entity is drawn through a reference, its properties are resolved against the reference's (ADR-0013 §4): an entity on layer `0` is on the reference's layer; a `ByBlock` colour, linetype, or lineweight is the reference's. Everything else is the entity's own. Nested references resolve against the outer first. The scene gives each resolved result its own style row, so a reference on a red layer whose definition is all layer `0` and `ByLayer` draws red, and recolouring that layer changes the style table and no geometry.

### 5. One change; derived state recomputed on apply

Redefining is `TableChange::Replace` on the one row. `Drawing::apply`, having applied it, finds every reference that reaches the row — directly or through other blocks — updates its entry in the spatial index from the table as it now is, and reports it as replaced, so the selection and the UI follow. The *shown* revision changes, so the scene is rebuilt; the style table follows the drawing revision as always. Undo and redo apply the inverse `Replace` and do the same. A redefinition of a block with ten thousand references is therefore one change in history, ten thousand index updates, and one scene rebuild — which is what §19.6 asks for.

## Alternatives considered

- **Entities with IDs inside definitions.** A second ID space, or shared one, buys addressability nobody needs until in-place block editing, and complicates every "what is this ID" question. Rejected; EXPLODE gives entities IDs when they become the drawing's.
- **References that cache their world geometry or bounds.** Fast to read and wrong after a redefinition unless every reference is rewritten — exactly the one-change-per-reference §19.6 forbids. Rejected; the table is read instead, and the one derived store that must be current, the spatial index, is updated in `apply`.
- **A context-free `Entity::bounds()` that lies for references.** Returning the insertion point as a reference's bounds would keep every call site unchanged and put wrong entries in the index. Rejected; the table is a parameter, and the compiler found every site.
- **Non-uniform scale.** Not a similarity: a scaled circle stops being a circle (DESIGN §22). Out of scope; the mirror flag covers the one reflection DXF commonly carries as a negative scale.

## Consequences

- `BlockTable` joins the drawing beside layers, linetypes, and text styles; `Change::Block` joins the one history; `Drawing::from_tables` will take blocks when DXF reads them (#149).
- `Entity::bounds`, `distance_to`, `primitives`, `intersections`, `is_inside`, and `crosses` take `&BlockTable`; `Drawing::bounds_of` is the drawing's own answer. The editing operations, which never take a reference, use the entity's own geometry.
- `PackedFrame::push_entity` takes the table, and the scene flattens references itself so that each part gets its resolved style.
- A reference offers its insertion point to snap to, and its definition's geometry offers what it would alone.
- `TransactionBuilder::insert` and `replace` refuse a reference to a block that does not exist, to `*Model_Space`, or at a scale that is not positive.

## Revisit when

- Attributes, dynamic blocks, or external references are wanted: attributes are entities of a reference's own, and this model has no place for them yet.
- In-place editing of a definition is wanted: then its entities need identities.
- A drawing with tens of thousands of nested references makes the per-redefinition reindex the slow step; a reverse index from block to references would make it proportional to the references touched.
