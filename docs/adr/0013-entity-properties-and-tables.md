# ADR-0013: Entity properties, tables, and one change model for both

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Bootstrap (fills a gap in the baseline before M4; changes no LOCKED decision)  
**Design refs:** §10, §11, §19.1, §19.2, §19.3, §32

## Context

Through M3 an entity was its geometry and nothing else, on an implied layer 0, and the history's `Change` had a single variant. M4 adds layers, colours, linetypes, lineweights, text styles, and blocks. Every one of them needs the same two things first, and the baseline states both only as outcomes:

- entities carry properties (§11 lists the tables; §32 lists what they hold);
- every table changes inside the same transactions and the same linear history as entities (§19.1–19.3), never in an undo stack of its own.

Three questions had to be settled before any table could be written:

1. **Where do an entity's properties live?**
2. **How does an entity refer to a layer** — or later a linetype, a text style, a block?
3. **What does a modify command do with properties**, given that the modify operations are pure functions of geometry?

## Decision

### 1. Properties are stored beside the geometry, not in it

`Entity` remains pure geometry. The drawing stores each entity as an `EntityRecord`: the shared geometry and, beside it, a small copyable `Properties { layer, color, linetype, lineweight }`.

```rust
pub struct EntityRecord {
    pub entity: SharedEntity,     // Arc<Entity>
    pub properties: Properties,
}
```

`Drawing::entity(id)` still returns the geometry; `Drawing::properties(id)` returns the rest. Nothing in `cad-geometry`, and none of the per-entity operations in `cad-document` — transform, offset, trim, extend, fillet, break, join, stretch, grips — sees a property or can lose one.

The transaction builder is where the two meet, and it makes the common case automatic:

| Call | Geometry | Properties |
| --- | --- | --- |
| `replace(id, entity)` | new | **kept** |
| `set_properties(id, properties)` | kept, shared | new |
| `insert(entity)` | new | those new objects are given |
| `insert_like(source, entity)` | new | those of `source` |
| `insert_with(entity, properties)` | new | as given |

An `EntityChange` carries records, so a change of properties alone is a `Replace` whose two records share one geometry: restyling a hundred thousand objects copies no geometry and leaves the spatial index alone.

### 2. Tables have rows with stable IDs; entities refer to rows by ID

A `Table<Row>` holds rows under `TableId<Row>`s that never change and are never reused — the rule entity IDs follow (§10.2), for the same reason. An entity's `layer` is a `LayerId`. Renaming a layer therefore changes one row and no entity, and a layer's name is free to be changed without anything else knowing.

Every table starts with one **permanent first row** that cannot be removed: layer `0` now; `Continuous`, `Standard`, and their like as those tables arrive. `Properties::default()` names it, so a default reference is always valid.

A row is changed by a `TableChange<Row>` — insert, remove, replace, the same three shapes as an entity change, inverted the same way.

### 3. One `Change`, one history

```rust
pub enum Change {
    Entity(EntityChange),
    Layer(TableChange<Layer>),
    // linetypes, text styles, blocks, settings: as their tables arrive
}
```

A transaction is an ordered list of these, of any mix; undo applies the inverses in reverse order. A layer and the entities on it can be created in one transaction and are undone in one step.

The builder refuses, with a typed reason and without recording anything, a change that would leave the drawing inconsistent: an entity put on a layer that does not exist, layer `0` removed, a layer removed while an entity is on it. Because each entity and each row can be changed at most once per transaction, every recorded change is valid against the drawing as it was when the transaction began, and commit cannot fail.

### 4. `ByLayer` and `ByBlock` are stored, and resolved only where a value is needed

`Color`, `Linetype`, and `Lineweight` each have `ByLayer` (the default) and `ByBlock` beside their concrete values. The indirection is what is stored and what is written to a file. It is resolved where a concrete value is needed — when the scene is built, when a property is shown — by one function per property:

- `ByLayer`: the value of the entity's layer;
- `ByBlock`: the value of the block reference the entity is being drawn through; outside any block, the drawing's default.

### 5. What each modify command does with properties

Since the operations know nothing of properties, the rule is stated once, at the commands, and tested per command:

- an object that is **changed** — moved, rotated, scaled, mirrored, stretched, trimmed, extended, broken, filleted, edited by a grip — keeps its own;
- an object **made from another** — a copy, a mirror image kept beside its original, an offset, the further pieces of a trim or a break — takes that object's;
- a **fillet arc or chamfer bevel** takes those of the object picked first;
- a **joined** object takes those of its oldest member, whose identity it also keeps;
- a **newly drawn** object gets the properties new objects are given — the defaults, until the current layer arrives.

## Alternatives considered

- **Properties inside each entity struct** (`LineEntity { start, end, properties }`). Rejected: every operation that builds an entity would have to carry them through, in dozens of `match` arms across three crates, and forgetting once would silently reset an object to layer 0. Geometry tests would all acquire a field they do not care about.
- **A side map from entity ID to properties.** Rejected: a second store to keep in step with the first on every insert, remove, undo, and redo, and a second lookup on the render path. The dense slot already exists.
- **Entities refer to layers by name**, as DXF does. Rejected: a rename would be a rewrite of every entity on the layer — one change per entity in the history, or a special case. Names are a file-format concern; `cad-dxf` maps them at the boundary.
- **A separate `EntityChange::Restyle` variant.** Rejected as unnecessary: `Replace` with shared geometry says the same thing with no new case for undo, redo, or the "changed once per transaction" rule.
- **A trait object per table instead of a generic `Table<Row>`.** Rejected: the tables differ only in their row type, and the ID type should say which table it belongs to.

## Consequences

- Adding a table is a row type, a `Change` variant, and one arm in `Drawing::apply`. The compiler points at every `match` on `Change`.
- A command that makes a new object from an existing one must say so with `insert_like`. Using plain `insert` there is the one way left to lose properties; `AGENTS.md` names it, and each modify command has a test.
- `ApplyReport` gains `tables_changed`, so derived state that depends on a table — what colour a layer's objects are drawn in — can follow a commit, an undo, or a redo.
- Refusing to remove a layer in use scans the entities. Layers are removed rarely; if it shows up in a profile, a count per layer is derived state and easy to add.
- DXF still reads and writes one layer until its own issue; a drawing with properties loses them on export until then.

## Revisit when

- Blocks arrive: a block definition holds entity records of its own, and `ByBlock` gets its meaning. If definitions need their records addressed differently from the drawing's, that is the block ADR's decision.
- A table needs rows that are large or shared (a block's contents): `Table<Row>` clones rows into the history; such a row should hold its payload behind an `Arc`, as entities do.
