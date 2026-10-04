# CadSlate2D

**Classic 2D CAD. In your browser.**

CadSlate2D is a free, browser-native, local-first 2D CAD application inspired by classic command-driven drafting: a command line, keyboard aliases, precise coordinate entry, object snaps, grips, and local files. No installation, no account, no cloud workspace.

![CadSlate2D visual identity](visual_identity.png)

> The image above is the visual target, not a screenshot of the current build.

## Status

Milestone **M0 — The Line** is functionally complete: a vertical slice that proves the architecture with a single entity type. You can draw lines by command or pointer, select them, undo and redo, export DXF, and reopen it — all in a static page with no backend. See the [milestones](https://github.com/LAA-Software-Engineering/cadslate2d/milestones) and [issues](https://github.com/LAA-Software-Engineering/cadslate2d/issues) for what comes next, in implementation order.

## What works today

Type anywhere on the page; you never need to click the command line first.

| Command | Aliases | What it does |
| --- | --- | --- |
| `LINE` | `L` | Draws a chain of line segments. Enter or Space finishes. |
| `UNDO` | `U`, Ctrl+Z | Reverts the last change. |
| `REDO` | Ctrl+Y, Ctrl+Shift+Z | Re-applies the last undone change. |
| `OPEN` | `DXFIN`, Ctrl+O | Opens an ASCII DXF file. Lines are imported; everything else is counted and reported. |
| `DXFOUT` | `SAVE`, `SAVEAS`, `EXPORT`, Ctrl+S | Downloads the drawing as ASCII DXF (R12). |

| Input | Meaning |
| --- | --- |
| `100,200` | Absolute point |
| `@500,0` | Point relative to the previous one |
| `@500<45` | Relative polar: distance `<` angle in degrees, counter-clockwise from +X |
| Click | A point while a command asks for one; otherwise selects the line under the cursor |
| Enter / Space | Accept or finish |
| Escape | Cancel the command; pressed again, clear the selection |
| Up / Down | Recall earlier input |
| Wheel / middle-drag | Zoom about the cursor / pan |

Unexported changes exist only in the browser tab until autosave arrives (M5); the page asks before you leave with unexported work.

## Architecture in one paragraph

Rust compiled to WebAssembly owns all CAD state — entities, commands, selection, undo history, snapping, DXF — and prepares batched render data. TypeScript owns the React shell and the WebGL2 context and submits those batches to the GPU. The boundary between them is deliberately coarse: browser events go in, dirty flags come out, and frames are pulled in bulk. There is no backend; the production build is a folder of static files.

Details: [`docs/DESIGN.md`](docs/DESIGN.md) (implementation baseline) and [`docs/adr/`](docs/adr/README.md) (architecture decisions).

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
  src/renderer/   WebGL2 adapter (the only code that touches WebGL)
  src/ui/         React shell
  e2e/            Playwright tests and the plain static server they run against
scripts/          build-wasm.sh, check.sh, check-boundaries.sh
test-data/        DXF fixtures, geometry regression cases, screenshots
benches/          generated benchmark drawings
docs/             DESIGN.md and ADRs
```

## Contributing

Read [`AGENTS.md`](AGENTS.md) first. It holds the architecture rules, engineering principles, testing rules, and workflow that apply to every change, whether written by a person or an AI agent.

## License

Not yet decided. CadSlate2D is free to use; open-source licensing is a separate governance decision (DESIGN §44).
