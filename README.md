# CadSlate2D

**Classic 2D CAD. In your browser.**

CadSlate2D is a free, browser-native, local-first 2D CAD application inspired by classic command-driven drafting: a command line, keyboard aliases, precise coordinate entry, object snaps, grips, and local files. No installation, no account, no cloud workspace.

![CadSlate2D visual identity](visual_identity.png)

> The image above is the visual target, not a screenshot of the current build.

## Status

Milestone **M0 — The Line** is complete: a vertical slice that proves the architecture with a single entity type. **M1 — Input and Basic Entities** is in progress. You can draw lines by command or pointer, select them, undo and redo, export DXF, and reopen it — all in a static page with no backend. See the [milestones](https://github.com/LAA-Software-Engineering/cadslate2d/milestones) and [issues](https://github.com/LAA-Software-Engineering/cadslate2d/issues) for what comes next, in implementation order.

## What works today

Type anywhere on the page; you never need to click the command line first.

| Command | Aliases | What it does |
| --- | --- | --- |
| `LINE` | `L` | A chain of line segments. Options: `Close`, `Undo`. |
| `CIRCLE` | `C` | Centre and radius, or `Diameter`, `2P`, `3P`. |
| `ARC` | `A` | Three points; or start, `Center`, end; or start, centre, `Angle`. |
| `PLINE` | `PL` | One polyline through a chain of points. Options: `Close`, `Undo`. |
| `RECTANG` | `REC` | A closed rectangle from two corners. |
| `ERASE` | `E`, Delete | Erases the selected objects. |
| `MOVE` | `M` | Moves the selection from a base point to a second point. |
| `COPY` | `CO`, `CP` | Copies the selection; keeps placing until Enter. |
| `UNDO` | `U`, Ctrl+Z | Reverts the last command. |
| `REDO` | Ctrl+Y, Ctrl+Shift+Z | Re-applies the last undone command. |
| `OPEN` | `DXFIN`, Ctrl+O | Opens an ASCII DXF file. Lines, circles, arcs, and straight polylines are imported; everything else is counted and reported. |
| `DXFOUT` | `SAVE`, `SAVEAS`, `EXPORT`, Ctrl+S | Downloads the drawing as ASCII DXF (R12). |

Modify commands work in either order: select first and then give the command, or give the command and select at `Select objects:`, finishing with Enter.

Options shown in brackets, as in `Specify next point or [Close/Undo]:`, are chosen by their capital letter.

| Input | Meaning |
| --- | --- |
| `100,200` | Absolute point |
| `@500,0` | Point relative to the previous one |
| `@500<45` | Relative polar: distance `<` angle in degrees, counter-clockwise from +X |
| `500` | At a next-point prompt: that distance towards the cursor (direct distance) |
| Click | A point while a command asks for one; otherwise adds the object under the cursor to the selection (Shift+click removes it) |
| Click or drag on empty space | A selection box: left to right selects what is entirely inside (window); right to left selects what it touches (crossing) |
| Enter / Space | Accept or finish; at the idle prompt, repeat the last command |
| Escape | Cancel the command; pressed again, clear the selection |
| Up / Down | Recall earlier input |
| Wheel / middle-drag | Zoom about the cursor / pan |

Unexported changes exist only in the browser tab until autosave arrives (M5); the page asks before you leave with unexported work.

## Architecture in one paragraph

Rust compiled to WebAssembly owns all CAD state — entities, commands, selection, undo history, snapping, DXF — and prepares batched render data. TypeScript owns the React shell and the WebGL2 context and submits those batches to the GPU. The boundary between them is deliberately coarse: browser events go in, dirty flags come out, and frames are pulled in bulk. There is no backend; the production build is a folder of static files.

Details: [`docs/DESIGN.md`](docs/DESIGN.md) (implementation baseline), [`docs/adr/`](docs/adr/README.md) (architecture decisions), [`docs/roadmap.md`](docs/roadmap.md) (what is next and why), and [`docs/reviews/`](docs/reviews/m0-exit-review.md) (milestone reviews with evidence).

## Development

### Prerequisites

| Tool | Version | Install |
| --- | --- | --- |
| Rust | pinned in `rust-toolchain.toml` (installed automatically by rustup) | <https://rustup.rs> |
| `wasm-bindgen-cli` | exactly the `wasm-bindgen` version pinned in `Cargo.toml` | `cargo install wasm-bindgen-cli --version 0.2.129 --locked` |
| Node.js | 22 or newer | <https://nodejs.org> |
| pnpm | 10 or newer | `corepack enable pnpm` |

The `wasm32-unknown-unknown` target, `rustfmt`, and `clippy` come with the pinned toolchain. `wasm-opt` is provided by the `binaryen` dev dependency; nothing to install.

### First run

```bash
pnpm --dir web install
```

```bash
pnpm --dir web dev
```

`dev` builds the WASM core (debug) and starts Vite. After changing Rust code, rebuild with `pnpm --dir web wasm` and reload the page.

### Everyday commands

| Task | Command |
| --- | --- |
| Full check (run before pushing) | `scripts/check.sh` |
| Rust tests | `cargo test --workspace` |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust format | `cargo fmt --all` |
| Crate-boundary check | `scripts/check-boundaries.sh` |
| Build WASM (debug / release) | `pnpm --dir web wasm` / `pnpm --dir web wasm:release` |
| Web typecheck, lint, unit tests | `pnpm --dir web typecheck` / `lint` / `test` |
| Production bundle | `pnpm --dir web build` → `web/dist/` |
| End-to-end tests (needs a production bundle) | `pnpm --dir web e2e` |
| Rust benchmarks (store, index, scene) | `cargo bench -p cad-benchmarks` |
| Generate a benchmark drawing | `cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf` |
| Render-transport measurement in the browser | `pnpm --dir web bench:transport` (after `build` and generating `lines-1k`, `-10k`, `-100k`) |
| Curve-rendering experiment in the browser | `pnpm --dir web bench:curves` (after `build`) |

First-time Playwright setup: `pnpm --dir web exec playwright install chromium`.

### Deployment

`web/dist/` is fully static. Serve it from any file server or static host, under any base path. There is no server-side component.

### Repository layout

```text
crates/
  cad-core/       points, bounds, entity IDs, tolerances, view math
  cad-geometry/   pure geometric functions
  cad-input/      command-line tokenizer and parser
  cad-document/   drawing, entities, storage, spatial index, transactions
  cad-commands/   Session, command state machines, selection
  cad-dxf/        DXF import/export
  cad-render/     RenderScene and OverlayFrame preparation
  cad-wasm/       the JS/WASM boundary (marshalling only)
web/
  src/cad/        TypeScript wrapper around the WASM package
  src/app/        CadApp, frame loop, UI store, key routing, host services
  src/renderer/   WebGL2 adapter (the only code that touches WebGL)
  src/ui/         React shell and theme tokens
  e2e/            Playwright tests and the plain static server they run against
  bench/          Browser measurements, run on demand
scripts/          build-wasm.sh, check.sh, check-boundaries.sh
test-data/        DXF fixtures, geometry regression cases, screenshots
benches/          cad-benchmarks crate: drawing generator and criterion benchmarks
docs/             DESIGN.md, ADRs, milestone reviews, roadmap
```

## Contributing

Read [`AGENTS.md`](AGENTS.md) first. It holds the architecture rules, engineering principles, testing rules, and workflow that apply to every change, whether written by a person or an AI agent.

## License

Not yet decided. CadSlate2D is free to use; open-source licensing is a separate governance decision (DESIGN §44).
