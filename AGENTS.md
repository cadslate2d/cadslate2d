# AGENTS.md

Guidance for AI coding agents and human contributors working in this repository. `CLAUDE.md` imports this file, so there is one source of truth.

Keep this file current: when a command, rule, or directory changes, update it in the same commit.

## What this project is

CadSlate2D is a free, browser-native, local-first 2D CAD application in the spirit of classic command-driven drafting. Rust/WebAssembly owns all CAD state; TypeScript/React owns the UI shell and the WebGL2 context. There is no backend.

## Sources of truth, in order

1. [`docs/DESIGN.md`](docs/DESIGN.md) — the implementation baseline. Items marked **LOCKED** are not reopened without an ADR backed by evidence (§46).
2. [`docs/adr/`](docs/adr/README.md) — architecture decisions. Read the ADRs touching your area before writing code.
3. GitHub issues — the work plan, in implementation order, grouped into epics per milestone. Each issue lists scope, acceptance criteria, and what blocks it.
4. [`visual_identity.png`](visual_identity.png) — the look-and-feel target. It is styling guidance, not a scope statement.

If code and these documents disagree, fix the code or write an ADR. Do not silently diverge.

## Architecture rules that are easy to break

- **Crate layering (ADR-0001).** Dependencies point strictly downward:
  `cad-wasm → cad-render → cad-commands → cad-document → cad-geometry → cad-core`, with `cad-input → cad-core`, `cad-commands → cad-input`, and `cad-dxf → cad-document`. `scripts/check-boundaries.sh` enforces this.
- **`wasm-bindgen`, `js-sys`, `web-sys` appear only in `cad-wasm`.** Every other crate builds and tests on the host with plain `cargo test`. `cad-wasm` contains marshalling only.
- **The document is mutated only through transactions** (`cad-document`). No public raw mutators. Entities and table rows (layers, …) change in the same transactions and the same history; there is never a second undo stack.
- **Properties live beside geometry, not in it** (ADR-0013). `Entity` is pure geometry; the layer, colour, linetype, and lineweight are in the stored record. `replace` keeps an object's properties. A command that makes a new object *from* an existing one — a copy, an offset, a piece — must insert it with `insert_like`, or the new object silently lands on layer 0.
- **What is on a layer that is off is not there; what is on a locked layer is not changed.** Anything the pointer does looks entities up with `Drawing::query_shown`, and the scene is built from `Drawing::shown`. The session keeps locked objects out of the selection and refuses them as the object a command picks, so a command needs no check of its own — unless, like STRETCH, it finds its objects itself.
- **React holds no drawing data.** No entity geometry, selection membership, grips, or view matrices in React state or props.
- **Coarse WASM boundary (ADR-0002).** Events in, dirty flags out, frames pulled in batches. No per-entity or per-vertex getters. Rust never issues GL calls.
- **Two render lifetimes (DESIGN §12).** Committed geometry → `RenderScene`, re-uploaded only when the scene revision changes. Transient geometry → `OverlayFrame`. Pointer movement, pan, and zoom must never regenerate or re-upload committed geometry (ADR-0003).
- **View-dependent background is overlay content.** The grid is rebuilt with the `OverlayFrame`; the adapter draws its style roles beneath the scene. It is not a third render lifetime and never touches committed buffers.
- **Transient edits do not touch the document.** Previews live in the session; one committed gesture produces one transaction.
- **A grip edit is a command** (ADR-0010). What a grip does is a pure function, `Entity::grip_moved`; the interaction is `GripEditCommand`, so it shares point acquisition, snapping, previews, and undo with every other command. Do not build a parallel interaction path.
- **Entities being transiently edited are never snap targets** (DESIGN §18.2). A command that moves existing geometry reports it through `Command::transient_sources`.
- **One pipeline turns the pointer into a point:** `Session::aim` — object snap, then ORTHO or POLAR, then the free cursor. Previews, the status-bar readout, direct-distance entry, and the click all use it. Typed coordinates bypass it. Do not add a second path. A command asking for a place on the screen rather than a point of the drawing — the corner of a window — opts out of object snap with `Command::takes_object_snap`.
- **Geometry is read through primitives.** `Entity::primitives` turns every entity into segments, circles, and arcs — including the arc segments of a polyline (a bulge per vertex). Snapping, intersection, selection, trimming, and rendering work from those; do not walk `PolylineEntity::vertices` as if the segments between them were straight.
- **Pixel tolerances are not geometric tolerances.** Pick and snap apertures are `PixelTolerance`s converted through the `View` at the moment of use (§18.3).
- **No magic epsilons.** Use the tolerance classes in `cad-core`; they are not interchangeable (DESIGN §21). A linear tolerance compares coordinates at their own magnitude: far from the origin use `LinearTolerance::around` the points involved, and decide whether a computed point is on a segment or arc by where it is along it, never by measuring its distance from the curve again (ADR-0012).
- **Only `web/src/renderer/` touches WebGL; only `web/src/cad/` imports the generated WASM package.** ESLint enforces both.
- **Rust never calls into JavaScript.** When a command needs the browser (a file picker, a download), it raises a `HostRequest`; the host serves it and reports back with an ordinary call.
- **The WASM API surface is snapshot-tested** (`web/src/cad/render-boundary.test.ts`). Adding an export fails that test on purpose: justify the new call as coarse, then update the list.

## Engineering principles

Apply these with judgement. The goal is code that is easy to change, not ceremony.

- **KISS first.** Write the simplest thing that satisfies the issue's acceptance criteria. No speculative abstractions for later milestones; the design explicitly forbids M7/M8 ideas from shaping M0 code.
- **SOLID.**
  - *Single responsibility:* each crate and module has one reason to change (see the responsibility table in ADR-0001).
  - *Open/closed:* new commands, entities, and snap modes are added by registering a new implementation, not by editing a central `match` in unrelated code.
  - *Liskov:* trait implementations (for example `SpatialIndex`) must be interchangeable; property tests compare them against a reference.
  - *Interface segregation:* small traits and small exported APIs; callers depend only on what they use.
  - *Dependency inversion:* higher layers depend on traits and semantic descriptions, never on packed GPU formats or browser APIs.
- **DRY.** One implementation of each rule: one screen↔world mapping (`View`), one write path into the document (transactions), one command entry point for typed input and UI actions. Do not deduplicate code that only looks similar.
- **Design patterns, where they fit the problem.**
  - *State:* every command is an explicit state machine (DESIGN §15).
  - *Command + Memento:* transactions record reversible `Change`s for linear undo/redo.
  - *Strategy:* `SpatialIndex` and `EntityStore` hide swappable implementations.
  - *Facade:* `Session` in Rust, `CadSession` across WASM.
  - *Adapter:* the TypeScript WebGL2 renderer adapts prepared buffers to GPU calls.
  - *Observer:* React subscribes to one external UI-state store.
  Do not introduce a pattern without a concrete need it removes.

## Testing rules (DESIGN §41, LOCKED)

- Every behaviour change ships with tests in the same commit.
- **Geometry:** unit tests, property tests where there are algebraic laws, and a permanent regression test for every fixed geometry bug. The corpora under `test-data/geometry/` (intersections, offsets, trims, extends, fillets) take one line per case and report a failure by file and line. Every modify operation has an invariant held over generated, nearly degenerate input (`crates/cad-geometry/tests/stress.rs`; the table is in `test-data/geometry/README.md`). `scripts/stress.sh` runs the property tests with many more cases; it is not part of the check.
- **Commands:** headless tests driving `Session` — the same methods `cad-wasm` calls.
- **Render boundary:** determinism, revision invariance under pointer movement, buffer lengths and batch ranges.
- **DXF:** fixture files under `test-data/dxf/`; arbitrary bytes must never panic.
- **Browser:** Vitest for TypeScript units, Playwright for end-to-end flows against the static build.
- **Visual baselines:** small clipped screenshots under `test-data/screenshots/`, compared in Chromium only. Re-record them deliberately (`--update-snapshots`) and look at the result before committing.

## Where things go

| You are adding… | It belongs in… | And needs… |
| --- | --- | --- |
| A command | `crates/cad-commands/src/commands/`, registered in `registry.rs` | Headless tests using `cad_commands::testing::Harness` |
| A command that moves, turns, scales, or reflects a selection | An `Operation` of `TransformCommand`, or a state machine built on `TransientEdit` and `cad_geometry::Transform` | Headless tests per entity type; preview leaves the revision unchanged; one undo |
| A geometry-heavy modify command (offset, trim, extend, fillet, break, join, stretch, …) | Maths on primitives in `cad-geometry`; the per-entity operation, returning a typed error, in `cad-document/src/editing.rs` (or `joining.rs`, for what takes several entities); the state machine in `cad-commands` | Every supported combination tested and listed in `docs/modify-matrix.md`; unsupported ones refused with a message |
| An entity type | `cad-document` (`Entity`), then every `match` the compiler points at: rendering, picking, snapping, `transformed`, grips, DXF | Bounds and hit-test property tests; DXF fixtures |
| A table (linetypes, text styles, blocks, …) | A row type and a `Table<Row>` in `cad-document`, a `Change` variant, one arm in `Drawing::apply`, builder methods that refuse what would leave the drawing inconsistent | The history property test in `drawing.rs` gains its operations; a test per refusal |
| Geometry maths | `cad-geometry`, as pure functions | Unit and property tests; a regression case per fixed bug |
| Input syntax | `cad-input` | Table-driven tests and a never-panics property test |
| A preview of what picking an object would do | `Command::preview_pick`, computed by the same function the pick then commits | A test that the previewed result equals the committed one |
| Something drawn transiently | An `OverlayItem` in `cad-commands`, turned into segments in `cad-render` | A render-boundary test that the scene revision is unchanged |
| A snap mode | `SnapMode` and its points in `crates/cad-commands/src/snap.rs`; its marker glyph in `cad-render` | Headless tests per entity type; a marker baseline in `test-data/screenshots/` |
| A drafting aid or its setting | Pure functions in `crates/cad-commands/src/aids.rs`; state in `Toggles` or `DraftingSettings`; applied in `Session::aim` | Unit tests for the function; headless precedence tests |
| A browser capability | `web/src/app/host-services.ts`, behind the `HostServices` interface | A `CadApp` test with a fake host |
| UI | `web/src/ui/`, styled only through tokens in `theme.css` | No drawing data in React state |

## Commands

Build, test, and lint commands are listed in [`README.md`](README.md#development). Run the full check before every push:

```bash
scripts/check.sh
```

End-to-end tests run against the production bundle:

```bash
pnpm --dir web build && pnpm --dir web e2e
```

## Workflow

- Work through issues in number order within the current milestone; respect "Blocked by".
- Commit directly to `main` and push. No pull requests, no feature branches (maintainer preference).
- One issue per commit where practical. Reference the issue in the commit message and close it with `Closes #N` once every acceptance criterion is met.
- Never push with failing checks.
- Keep `README.md`, `AGENTS.md`, and `CLAUDE.md` accurate in the same commit as the change that affects them.
- Decisions that resolve a PROVISIONAL or EXPERIMENT item, or change the baseline, get an ADR with evidence.

## Scope discipline

- The open milestone's issues are the scope. Do not pull work forward from a later milestone (layers, text, dimensions, workers, …) because it would be convenient; DESIGN Part IV says what belongs where.
- Product constraints in DESIGN §4 and §45 apply always: no accounts, no cloud documents, no telemetry on drawing content, no 3D.
- When an issue's scope turns out to be wrong, update the issue; do not quietly expand or shrink it.
