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
- **The document is mutated only through transactions** (`cad-document`). No public raw mutators.
- **React holds no drawing data.** No entity geometry, selection membership, grips, or view matrices in React state or props.
- **Coarse WASM boundary (ADR-0002).** Events in, dirty flags out, frames pulled in batches. No per-entity or per-vertex getters. Rust never issues GL calls.
- **Two render lifetimes (DESIGN §12).** Committed geometry → `RenderScene`, re-uploaded only when the scene revision changes. Transient geometry → `OverlayFrame`. Pointer movement, pan, and zoom must never regenerate or re-upload committed geometry (ADR-0003).
- **Transient edits do not touch the document.** Previews live in the session; one committed gesture produces one transaction.
- **No magic epsilons.** Use the tolerance classes in `cad-core`; they are not interchangeable (DESIGN §21).
- **Only `web/src/renderer/` touches WebGL; only `web/src/cad/` imports the generated WASM package.**

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
- **Geometry:** unit tests, property tests where there are algebraic laws, and a permanent regression test for every fixed geometry bug.
- **Commands:** headless tests driving `Session` — the same methods `cad-wasm` calls.
- **Render boundary:** determinism, revision invariance under pointer movement, buffer lengths and batch ranges.
- **DXF:** fixture files under `test-data/dxf/`; arbitrary bytes must never panic.
- **Browser:** Vitest for TypeScript units, Playwright for end-to-end flows against the static build.

## Commands

Build, test, and lint commands are listed in [`README.md`](README.md#development). Run the full check before every push:

```bash
scripts/check.sh
```

## Workflow

- Work through issues in number order within the current milestone; respect "Blocked by".
- Commit directly to `main` and push. No pull requests, no feature branches (maintainer preference).
- One issue per commit where practical. Reference the issue in the commit message and close it with `Closes #N` once every acceptance criterion is met.
- Never push with failing checks.
- Keep `README.md`, `AGENTS.md`, and `CLAUDE.md` accurate in the same commit as the change that affects them.
- Decisions that resolve a PROVISIONAL or EXPERIMENT item, or change the baseline, get an ADR with evidence.

## Scope discipline

- M0 non-goals are listed in DESIGN §27. Do not add circles, layers, grips, workers, or anything else on that list while M0 is open.
- Product constraints in DESIGN §4 and §45 apply always: no accounts, no cloud documents, no telemetry on drawing content, no 3D.
- When an issue's scope turns out to be wrong, update the issue; do not quietly expand or shrink it.
