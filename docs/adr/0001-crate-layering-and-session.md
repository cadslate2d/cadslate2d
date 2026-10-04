# ADR-0001: Crate layering and the interactive session

**Status:** Proposed  
**Date:** 2026-10-04  
**Kind:** Bootstrap (fills a gap in the baseline; changes no LOCKED decision)  
**Design refs:** §7.2, §8, §9, §11, §19.5, §41

## Context

DESIGN.md §9 names eight crates but does not say which may depend on which, what `cad-core` contains, or where the interactive root object lives. Three baseline requirements constrain the answer:

- Rust owns command state, selection, transient edits, and snaps as well as the document (§7.2).
- Command execution must be testable headlessly (§41).
- The spatial index and render caches are derived state and must not leak into undo history (§19.5).

The workspace scaffold is the first implementation task, so the dependency direction has to be fixed before any crate has content.

## Decision

### Dependency graph

Dependencies point strictly downward. No cycles, no exceptions.

```text
cad-wasm      → cad-commands, cad-render, cad-dxf
cad-render    → cad-commands, cad-document, cad-geometry, cad-core
cad-dxf       → cad-document, cad-core
cad-commands  → cad-document, cad-input, cad-geometry, cad-core
cad-document  → cad-geometry, cad-core
cad-input     → cad-core
cad-geometry  → cad-core
cad-core      → (no workspace dependencies)
```

### Responsibilities

| Crate | Owns | Must not |
| --- | --- | --- |
| `cad-core` | `Point2`, `Vec2`, `Aabb`, `EntityId`, tolerance types and values (§21.1), `View` screen↔world math | know about entities, commands, or rendering |
| `cad-geometry` | pure functions: distances, intersections, predicates | hold state or know about `Drawing` |
| `cad-input` | tokenizing and parsing text into typed input (§13) | know about `Drawing` or any specific command |
| `cad-document` | `Drawing`, entities, tables, `EntityStore`, `SpatialIndex`, transactions, `History` | know about commands, selection, view, or rendering |
| `cad-commands` | `Session`, command state machines, selection, snaps, grips, transient edit sessions | know about packed GPU geometry or JS |
| `cad-dxf` | DXF ↔ `Drawing` translation and import reports | leak DXF group codes into `cad-document` |
| `cad-render` | `RenderScene` from `Drawing`; `OverlayFrame` from session interaction state | mutate the document or session |
| `cad-wasm` | marshalling between JS and `Session`/`cad-render`/`cad-dxf` | contain CAD logic |

### The `Session`

`cad_commands::Session` is the single interactive root. It owns:

- the `Drawing`;
- the active command state machine;
- the selection set;
- the current transient edit session;
- the `View` (see ADR-0003);
- drafting toggles and input history.

Every user action reaches the core as a method call on `Session`. `cad-wasm` wraps exactly one `Session` per open drawing.

### Rules

1. `wasm-bindgen`, `js-sys`, and `web-sys` appear only in `cad-wasm`.
2. Every crate except `cad-wasm` builds and runs its tests on the host target with plain `cargo test`.
3. Canonical document state is mutated only through the transaction API in `cad-document`. Raw mutators are not public.
4. `cad-document` updates its own spatial index as part of applying a transaction. Render caches live in `cad-render`/`cad-wasm`, keyed by scene revision, never inside `Drawing`.
5. Transient state is described semantically by `cad-commands` (preview entities, markers, highlights). `cad-render` turns that description into packed geometry. Commands never build vertex data.

## Alternatives considered

- **`Session` in `cad-wasm`.** Rejected: command flows would only be testable through a WASM runtime, violating §41.
- **A separate `cad-session` crate.** Deferred. §9 is the baseline crate list; split `cad-commands` later if it grows unwieldy. The dependency direction would not change.
- **`cad-commands` depends on `cad-render` and emits overlay geometry directly.** Rejected: every command would be coupled to the packed-buffer format, which is PROVISIONAL (§12.2).
- **`EntityId` and primitives in `cad-document`.** Rejected: `cad-input` and `cad-geometry` would need the whole document crate to name a point.

## Consequences

- The M0 user story can be executed as a native Rust test before any browser code exists.
- `cad-render` depends on `cad-commands`, so the overlay description types are a compatibility surface between the two.
- Because `Drawing` owns its spatial index, index maintenance cost is paid inside transaction commit; if incremental updates degrade, the rebuild-on-idle option (§20) is implemented inside `cad-document` without affecting other crates.
- `cad-core` risks becoming a dumping ground. Anything added there must be needed by at least two otherwise-unrelated crates.

## Revisit when

- `cad-commands` becomes large enough that compile times or ownership boundaries suffer.
- A worker architecture is adopted (§37) and requires a crate usable without `Session`.
