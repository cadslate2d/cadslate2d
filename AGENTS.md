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
  `cad-wasm → cad-render → cad-commands → cad-document → cad-geometry → cad-core`, with `cad-input → cad-core`, `cad-commands → cad-input`, `cad-text → cad-core`, `cad-document → cad-text`, `cad-dxf → cad-document`, `cad-dwg → cad-dxf` (ADR-0036), `cad-intent → cad-core` and `cad-commands → cad-intent` (ADR-0037), `cad-layout → cad-core` and `cad-commands → cad-layout` (ADR-0039), and `cad-plot → cad-render` (ADR-0027). `scripts/check-boundaries.sh` enforces this.
- **`wasm-bindgen`, `js-sys`, `web-sys` appear only in `cad-wasm`.** Every other crate builds and tests on the host with plain `cargo test`. `cad-wasm` contains marshalling only.
- **The document is mutated only through transactions** (`cad-document`). No public raw mutators. Entities and table rows (layers, …) change in the same transactions and the same history; there is never a second undo stack.
- **Geometry that depends on a table is read through `Tables`** (ADR-0017, ADR-0020). A block reference's geometry is its definition's; a dimension's is laid out by its style, a multileader's by its multileader style (ADR-0033). `Entity::bounds`, `distance_to`, `primitives`, `crosses`, `push_entity`, and `entity_snap_points` take `Drawing::tables()`; nothing derived is stored in the entity, so a redefinition or a style change is one row changing (§19.6) and `Drawing::apply` updates the index entries of what depends on it.
- **Properties live beside geometry, not in it** (ADR-0013). `Entity` is pure geometry; the layer, colour, linetype, and lineweight are in the stored record. `replace` keeps an object's properties. A command that makes a new object *from* an existing one — a copy, an offset, a piece — must insert it with `insert_like`, or the new object silently lands on layer 0.
- **What is on a layer that is off or frozen is not there; what is on a locked layer is not changed.** Visibility is `Layer::is_shown` — on and not frozen, two switches kept apart (#213). Anything the pointer does looks entities up with `Drawing::query_shown`, and the scene is built from `Drawing::shown`. The session keeps locked objects out of the selection and refuses them as the object a command picks, so a command needs no check of its own — unless, like STRETCH, it finds its objects itself. A command that changes the layer rather than the object (`LAYULK`) opts in with `Command::picks_locked_objects`.
- **Constraints are rows of the drawing's constraint table** (ADR-0038): they refer to parts of entities (`Reference`: an entity and its `Part`), and the solver is `cad_geometry::constraint`, pure functions. Removing an entity, replacing it with geometry a constraint no longer fits, or moving it to another space takes the constraint out in the same transaction; never leave a row naming a part that is not there. **`TransactionBuilder::commit` settles them** over the drawing as it will be: what the transaction set is held (and what a command `hold`s), the rest of each constrained part follows in the same transaction, and an edit that cannot be settled is not applied — the report's `refused` and `Drawing::take_refusals` say why, and the session logs it. A command needs no constraint code of its own.
- **Objects from another drawing come in through `TransactionBuilder::bring_in`** (#222): every row they refer to is matched by name — the drawing's own kept where a name is taken — so an inserted file, a written block, and the clipboard share one rule.
- **A group is a table row, and picking one member picks it** (#214). Groups change in transactions like every table; removing an entity takes it out of its groups in the same transaction, and a group may change more than once in one (`group_now` reads the newest). The selection grows to whole groups in one place, `Session::change_selection`, when `DraftingSettings::group_selection` is on; a command that picks one object (`InputKind::Object`) gets that object.
- **React holds no drawing data.** No entity geometry, selection membership, grips, or view matrices in React state or props.
- **Coarse WASM boundary (ADR-0002).** Events in, dirty flags out, frames pulled in batches. No per-entity or per-vertex getters. Rust never issues GL calls.
- **Three committed instance kinds: segments, arcs, and triangles** (ADR-0003, ADR-0009, ADR-0021). A filled region is triangles in the frame's own buffer, with a style index each and no path distance; nothing else is drawn filled. The overlay carries triangles too, for a fill being previewed or highlighted.
- **Two render lifetimes (DESIGN §12).** Committed geometry → `RenderScene`, re-uploaded only when the scene revision changes — and then only the chunks holding what changed (ADR-0022): the scene is packed in chunks of entity IDs, `Drawing::shown_changes_since` says which an edit touched, and `scene_dirty_ranges` says which instance runs the frame uploads. Transient geometry → `OverlayFrame`. Pointer movement, pan, and zoom must never regenerate or re-upload committed geometry (ADR-0003).
- **What is painted over what is a tier on the record** (ADR-0032). `EntityRecord::order` is an `i16`; a new object takes the drawing's top tier, one made from another (`insert_like`) that one's, a wipeout the tier above the top (`insert_on_top`). Within a tier, fills (`Entity::is_fill`) lie beneath strokes. The scene carries the tier in the high half of each instance's style word and the adapter draws it as depth — fills write it, strokes only test it — so never sort the scene by order or draw a pass per tier. The page paints in `draw_order::paint_key` order, and a file's records are written in it; a file's order — its records', or its `SORTENTSTABLE`'s — is read back through `draw_order::tiers_of`.
- **Committed colour is a style index, not a batch** (ADR-0014). Every committed instance carries the index of a row in the scene's style table — the low half of its style word; `ByLayer` is resolved when the table is, and the adapter supplies the theme's colour for a "foreground" row. Recolouring a layer must change the table and no geometry: the geometry follows `Drawing::shown_revision`, the table `Drawing::revision`. Batches and style roles are for the overlay. A lineweight is a byte of the same row: how wide it is on screen, and whether lineweights are shown at all (LWT), is the adapter's business.
- **View-dependent background is overlay content.** The grid is rebuilt with the `OverlayFrame`; the adapter draws its style roles beneath the scene. It is not a third render lifetime and never touches committed buffers.
- **The view is session state.** A command moves it through `CommandContext::view` (ZOOM, PAN); the session reports `Dirty::VIEW` when it changed and never records it in the history, so `U` undoes drawing changes only. `ZOOM Previous` has a stack of its own.
- **The current space is session state too** (ADR-0029, ADR-0030). Every entity is in a space — the model, row 0 of the layout table, or a layout — and the session is in one: what the pointer finds, what a command makes (`CommandContext::transaction`, `CommandContext::space`), and what the scene shows are its. A command changes it only through `CommandContext::request_space`, applied after its step. `Drawing::shown`, `query_shown`, and `extents` are the model's; the `_in` forms take a space. The committed scene of a layout is the model's chunks and then the layout's, each region flagged, with the model drawn through each viewport by the adapter under `scene_viewports`; never a second scene per viewport.
- **Transient edits do not touch the document.** Previews live in the session; one committed gesture produces one transaction.
- **A grip edit is a command** (ADR-0010). What a grip does is a pure function, `Entity::grip_moved`; the interaction is `GripEditCommand`, so it shares point acquisition, snapping, previews, and undo with every other command. Do not build a parallel interaction path.
- **Entities being transiently edited are never snap targets** (DESIGN §18.2). A command that moves existing geometry reports it through `Command::transient_sources`.
- **One pipeline turns the pointer into a point:** `Session::aim` — object snap, then object snap tracking from acquired points (`OTRACK`), then grid snap of the free cursor (`SNAP`), then ORTHO or POLAR from that point. Rust keeps no clock: a pointer at rest is the host's `pointer_rest` call. Previews, the status-bar readout, direct-distance entry, and the click all use it. Typed coordinates bypass it. Do not add a second path. A command asking for a place on the screen rather than a point of the drawing — the corner of a window — opts out of object snap with `Command::takes_object_snap`.
- **An entity without bounds stays out of the tree** (ADR-0031). An xline or a ray is kept in the drawing's side list beside the spatial index, left out of `extents`, read along `CONSTRUCTION_REACH` for geometry — clipped locally (`Entity::within`) where precision matters — and drawn by the overlay clipped to the view, never by the committed scene. A new unbounded kind answers `Entity::is_unbounded` and `Entity::construction`.
- **Geometry is read through primitives.** `Entity::primitives` turns every entity into segments, circles, and arcs — including the arc segments of a polyline (a bulge per vertex). Snapping, intersection, selection, trimming, and rendering work from those; do not walk `PolylineEntity::vertices` as if the segments between them were straight.
- **Pixel tolerances are not geometric tolerances.** Pick and snap apertures are `PixelTolerance`s converted through the `View` at the moment of use (§18.3).
- **No magic epsilons.** Use the tolerance classes in `cad-core`; they are not interchangeable (DESIGN §21). A linear tolerance compares coordinates at their own magnitude: far from the origin use `LinearTolerance::around` the points involved, and decide whether a computed point is on a segment or arc by where it is along it, never by measuring its distance from the curve again (ADR-0012).
- **Only `web/src/renderer/` touches WebGL; only `web/src/cad/` imports the generated WASM package.** ESLint enforces both.
- **Rust never calls into JavaScript.** When a command needs the browser (a file picker, a download), it raises a `HostRequest`; the host serves it and reports back with an ordinary call.
- **The WASM API surface is snapshot-tested** (`web/src/cad/render-boundary.test.ts`). Adding an export fails that test on purpose: justify the new call as coarse, then update the list.
- **A model's output reaches the document only as a validated intent IR compiled to registered commands, previewed, and committed on Accept as one undo** (ADR-0037). The same single entry point as typing and the menus' macros (ADR-0034); never a direct edit, never a file command. The AI service is optional and elsewhere: a build with no service configured has no AI, and nothing else changes.

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
- **DXF and DWG:** fixture files under `test-data/dxf/` and `test-data/dwg/` (the DWG ones written through acadrust by `crates/cad-dwg/examples/fixtures.rs`, never by hand); arbitrary bytes must never panic. `scripts/dwg-sweep.sh` opens every DWG under a directory and prints what the import makes of it; not part of the check. `scripts/fuzz.sh` runs the coverage-guided fuzzers (`fuzz/`, nightly and cargo-fuzz; not part of the check): the readers on bytes, the input grammar on every kind of prompt, the intent parser on text. A crash becomes a fixture (or a table row in `cad-input`'s tests) and a fix, in the same commit. Export → import → export is byte-identical for the curated fixtures and float-exact under the fuzzer, since derived values — degrees from radians, a radius dimension's points — come back an ulp off.
- **Browser:** Vitest for TypeScript units, Playwright for end-to-end flows against the static build.
- **Visual baselines:** small clipped screenshots under `test-data/screenshots/`, compared in Chromium only. Re-record them deliberately (`--update-snapshots`) and look at the result before committing.

## Where things go

| You are adding… | It belongs in… | And needs… |
| --- | --- | --- |
| A command | `crates/cad-commands/src/commands/`, registered in `registry.rs` | Headless tests using `cad_commands::testing::Harness` |
| A command that answers a question (`DIST`, `AREA`, `LIST`, …) | `crates/cad-commands/src/commands/inquiry.rs`: it reads through `CommandContext` and commits no transaction; what it says of an entity comes from `summary::geometry_of`, the one description the panel uses too | Headless tests that the log says it and the revision is unchanged |
| A command that moves, turns, scales, or reflects a selection | An `Operation` of `TransformCommand`, or a state machine built on `TransientEdit` and `cad_geometry::Transform` | Headless tests per entity type; preview leaves the revision unchanged; one undo |
| A geometry-heavy modify command (offset, trim, extend, fillet, break, join, stretch, …) | Maths on primitives in `cad-geometry`; the per-entity operation, returning a typed error, in `cad-document/src/editing.rs` (or `joining.rs`, for what takes several entities); the state machine in `cad-commands` | Every supported combination tested and listed in `docs/modify-matrix.md`; unsupported ones refused with a message |
| A DXF record, table, or header variable | `crates/cad-dxf/src/import.rs` (a `RecordBody` group, an arm in `finish_record`) and `export.rs` (a `Writer` method); a fixture from an independent writer under `test-data/dxf/` | A row in `docs/dxf-matrix.md` naming its test or fixture — a test checks every name exists; export → import → export byte-identical; a report key for whatever is not held |
| A DWG container, section, or object kind | `crates/cad-dwg`: the bit codes in `bits.rs`, a container per version (`r2000.rs`, `r2004.rs`, `r2007.rs`), objects decoded by the prescriptions of the ODA specification in `objects.rs`, `entities.rs`, `tables.rs`, `records.rs`, feeding `cad-dxf`'s records (ADR-0036); written back from `cad_dxf::export_records` by the mirror of each decoder under `write/` — a decoder and its encoder change together; fixtures written by `examples/fixtures.rs` through acadrust, never by hand | A row in `docs/dwg-matrix.md` naming its test or fixture, and the specification errata table kept current; every bound of `ImportLimits` reached by a test; the `dwg_read` fuzz target finding nothing; every DXF fixture round-tripping byte for byte through the writer and read by acadrust with nothing lost (`tests/writer.rs`); `scripts/dwg-check.sh` clean when LibreDWG is at hand |
| An entity type | `cad-document` (`Entity`), then every `match` the compiler points at: `cad_render::walk_entity` (the one walk that feeds the screen and the plot), picking, snapping, `transformed`, grips, DXF | Bounds and hit-test property tests; DXF fixtures |
| Something on paper | `cad-plot`: the page from `PlotSettings`, the content through a `GeometrySink` of `walk_entity`, never a second walk over the entity kinds (ADR-0027) | A unit test of what lands on the page; the property test that every primitive lands inside it |
| Something drawn filled | Triangles through `PackedFrame::push_triangle`, cut by `cad_geometry::tessellate` from rings (ADR-0021); a third committed instance kind beside segments and arcs, with a style index per triangle and no path distance. The adapter draws fills first, then strokes | A render-boundary test; the fill-inside-the-boundary property test |
| A kind of dimension | A `Measure` variant in `cad-document/src/dimension.rs` with its arm of `layout`, `value`, `points`, and `with_point`; the geometry it needs as pure functions in `cad-geometry/src/dimension.rs` (ADR-0020). Its geometry is never stored: `Entity::bounds` and the rest take `Tables` and lay it out from the style | Layout unit tests; the kind in the `measure()` strategy of the property tests; a DXF subclass in `cad-dxf` |
| A table (linetypes, text styles, blocks, …) | A row type and a `Table<Row>` in `cad-document`, a `Change` variant, one arm in `Drawing::apply`, builder methods that refuse what would leave the drawing inconsistent | The history property test in `drawing.rs` gains its operations; a test per refusal |
| A drawing setting (the current layer, the unit) | A field of `DrawingSettings` in `cad-document`, set through the transaction builder; read and written by `cad-dxf` | An undo test; the history property test gains its operation; a DXF round trip |
| Geometry maths | `cad-geometry`, as pure functions | Unit and property tests; a regression case per fixed bug |
| Input syntax | `cad-input` | Table-driven tests and a never-panics property test |
| Text metrics or glyphs | `cad-text`: the stroke font's table and the functions over it (ADR-0016). `Font::metrics` is what a layout engine — dimensions — may rely on: width, ascent, descent at a height, with every stroke inside that box. Text reaches the renderer as line segments; the renderer does not know text exists | Host property tests: width adds up and is linear in the height, strokes lie in the metrics box at any angle, the document's frame follows the text through transforms, and the scene's segments lie inside the frame |
| A prompt for a number | `ExpectedInput::distance()` for a length, which takes a unit suffix; `ExpectedInput::factor()` for a plain number — a scale, a count — which does not | A test that `2mm` is refused where it makes no sense |
| A prompt for a name or other free text | `ExpectedInput::text()`; the command receives `CommandInput::Text`. Space does not submit there, which the host learns from `ui_state.space_accepts` | Headless tests with a name that has a space in it |
| A preview of what picking an object would do | `Command::preview_pick`, computed by the same function the pick then commits | A test that the previewed result equals the committed one |
| Something drawn transiently | An `OverlayItem` in `cad-commands`, turned into segments in `cad-render` | A render-boundary test that the scene revision is unchanged |
| An operation of the intent IR (ADR-0037) | A variant of `Operation` in `crates/cad-intent/src/lib.rs`, its checks in `validate.rs`, its command and typed answers in `compile.rs`; the command must be in `COMMANDS`, the allowlist, which never holds a file, host, or settings command. The engine runs it through `Session::run_intent` (`crates/cad-commands/src/intent.rs`): a selection by ID, a pick as a click on the object's nearest point, each answer typed through the one entry point; what an operation made is the selection for the next. A model's result is a proposal (`Session::propose`, `crates/cad-commands/src/proposal.rs`): the intent run on a clone of the drawing, the difference drawn by the overlay, and run again on the drawing itself on Accept inside one undo group | A test in `crates/cad-commands/src/tests/intent.rs` of the drawing it makes; a case in `testing::operation()`; the schema snapshot `test-data/intent/schema-v<VERSION>.json` re-recorded with `UPDATE_SNAPSHOTS=1` and `cad_intent::VERSION` bumped, the versions it still reads in `cad_intent::READS` |
| A part generator (#272) | A `Spec` in `crates/cad-document/src/generate.rs`: its checks with the reason refused, its entities and constraint templates in `shapes`; a `Form` of fields in `crates/cad-commands/src/commands/generators.rs`, registered as a command; an intent operation compiled to that command (the row below) | Headless tests of its prompts, the part placed with its constraints holding as one undo, its refusals, and the same drawing by command and by intent |
| A placement against what is drawn (#273) | Its geometry as pure functions in `cad-geometry` (`tangent.rs`: circles tangent to two curves with a radius, to three, and the one the picks mean); the command in `crates/cad-commands/src/commands/placement.rs`, the constraints that keep it so added in the transaction that makes it, the picked objects held (`commit_tangent`) | Headless tests per placement — tangency to lines, arcs, circles, and polyline segments — kept through an edit of what it was placed against; a case per configuration in `test-data/geometry/tangents.txt`; the same drawing by command and by intent |
| A kind of constraint (ADR-0038) | A `Kind` in `crates/cad-document/src/constraint.rs` with the parts it relates in `Constraint::check`; its equations in `equations_of` (`crates/cad-document/src/drawing/settle.rs`), from `cad_geometry::constraint::Equation`; the command that makes it in `crates/cad-commands/src/commands/constrain.rs` | Headless tests per entity type it takes, the geometry settled in one undo, and its refusals |
| A floor-plan layout rule (ADR-0039) | `crates/cad-layout`: the brief in `spec.rs`, a necessary condition (a proof, refused as `Refusal::Infeasible`) in `search.rs`'s `necessary`, a hard constraint in `judge` and again, independently, in `verify.rs`; how a plan is drafted — wall faces cut by doors, jambs, doors, labels — in `draft.rs`, and drawn as entities on `WALL-EXT`, `WALL-INT`, `DOOR`, and `ROOM-TEXT` by `cad_commands::floorplan` | A unit test at the condition's bound; an adversarial `verify` test; a brief in `test-data/layout/briefs.jsonl` with what should come of it; the reliability study (`cargo run --release -p cad-layout --example reliability`) run again and its table in the ADR updated |
| A snap mode | `SnapMode` and its points in `crates/cad-commands/src/snap.rs`; its marker glyph in `cad-render` | Headless tests per entity type; a marker baseline in `test-data/screenshots/` |
| A drafting aid or its setting | Pure functions in `crates/cad-commands/src/aids.rs`; state in `Toggles` or `DraftingSettings`; applied in `Session::aim` | Unit tests for the function; headless precedence tests |
| A browser capability | `web/src/app/host-services.ts`, behind the `HostServices` interface | A `CadApp` test with a fake host |
| A menu item | `web/src/ui/menus.ts`, data in the model of `menu-model.ts`: a macro over the command line (`app/macro.ts`, ADR-0034), a panel, or a dialog; greyed or checked by a pure function of the UI snapshot | A Vitest test that its command is registered and its macro runs; an e2e flow for a new kind of item |
| Something the AI panel does (ADR-0037) | `web/src/app/ai-assistant.ts` (the state, behind `AiClient` and `AssistantHost`), `web/src/app/ai-client.ts` (the service's routes), `web/src/ui/AiPanel.tsx`; the account — plan, usage, billing, sign-out, deletion — is not in the panel but in the top bar and `web/src/ui/AccountDialog.tsx`. The proposal lives in the engine's session; drawing context is read from `CadEngine::aiContext` only when the user is asked about it | A test in `ai-assistant.test.ts` with the fakes; an e2e flow in `web/e2e/ai-panel.spec.ts` against the stand-in service `web/e2e/ai-fake.ts`, which follows the service's routes and replays the corpus's recorded model answers. The e2e bundle is `pnpm build:e2e`, built with the stand-in's address |
| A request the AI should handle | A case in `test-data/ai/<level>/cases.jsonl` — `l1` drafting, `l2` parts, constraints, and parametric edits, `l3` floor plans and their revisions, judged by what their brief must have — in English and Portuguese, with the drawing it should make (`test-data/ai/README.md`) | The requests written again (`UPDATE_SNAPSHOTS=1`), recorded by the service's `scripts/record.sh`, and the replay passing: `cad_commands::evaluation` judges, `ai-eval` in `make check` scores every level and language |
| UI | `web/src/ui/`, styled only through tokens in `theme.css` | No drawing data in React state |

## Commands

Build, test, and lint commands are listed in [`README.md`](README.md#development); the root `Makefile` wraps them (`make` lists the targets). A target only calls a script or a pnpm script — put new logic in `scripts/`, not in the Makefile. Run the full check before every push:

```bash
make check
```

End-to-end tests run against the production bundle, which `make e2e` builds first:

```bash
make e2e
```

## Workflow

- Work through issues in number order within the current milestone; respect "Blocked by".
- The maintainer commits directly to `main` and pushes; no feature branches for the maintainer's own work. Contributions from outside come by pull request from a fork, with the CI check and the CLA check passing; the maintainer reviews and merges (`CONTRIBUTING.md`, #294).
- One issue per commit where practical. Reference the issue in the commit message and close it with `Closes #N` once every acceptance criterion is met.
- Never push with failing checks.
- Keep `README.md`, `AGENTS.md`, and `CLAUDE.md` accurate in the same commit as the change that affects them.
- Decisions that resolve a PROVISIONAL or EXPERIMENT item, or change the baseline, get an ADR with evidence.
- A dependency version is taken only once it has been published for two weeks: pnpm refuses younger ones (`minimumReleaseAge` in `web/pnpm-workspace.yaml`), and Dependabot waits as long (`cooldown` in `.github/dependabot.yml`). Cargo has no such setting: check a crate's release date by hand before raising a version in `Cargo.toml`. A security fix younger than that is taken deliberately, saying so in the commit.

- Contributors sign the Contributor License Agreement (`CLA.md`) once, on their first pull request; the `CLA` workflow asks and records it. It is what lets the maintainer license the code under other terms as well (ADR-0035).

## Scope discipline

- The open milestone's issues are the scope. Do not pull work forward from a later milestone (layers, text, dimensions, workers, …) because it would be convenient; DESIGN Part IV says what belongs where.
- Product constraints in DESIGN §4 and §45 apply always: no accounts except for the AI input method, no cloud documents, no telemetry on drawing content, no drawing context sent without the user's consent to that request, no 3D (ADR-0037).
- When an issue's scope turns out to be wrong, update the issue; do not quietly expand or shrink it.
