# Development

## Prerequisites

| Tool | Version | Install |
| --- | --- | --- |
| Rust | pinned in `rust-toolchain.toml` (installed automatically by rustup) | <https://rustup.rs> |
| `wasm-bindgen-cli` | exactly the `wasm-bindgen` version pinned in `Cargo.toml` | `cargo install wasm-bindgen-cli --version 0.2.129 --locked` |
| `cargo-deny` | any recent | `cargo install cargo-deny --locked` (the check audits dependency licences and advisories, #296) |
| `cargo-about` | any recent | `cargo install cargo-about --locked --features cli` (the check verifies the third-party notices, #296) |
| Node.js | 22 or newer | <https://nodejs.org> |
| pnpm | 10 or newer | `corepack enable pnpm` |

The `wasm32-unknown-unknown` target, `rustfmt`, and `clippy` come with the pinned toolchain. `wasm-opt` is provided by the `binaryen` dev dependency; nothing to install.

## First run

```bash
pnpm --dir web install
```

```bash
pnpm --dir web dev
```

`dev` builds the WASM core (debug) and starts Vite. After changing Rust code, rebuild with `pnpm --dir web wasm` and reload the page.

The optional AI input method ([ADR-0037](adr/0037-ai-input-method.md)) talks to a hosted service, whose address the build takes from `VITE_AI_SERVICE_URL` — `https://…`, or `http://localhost…` for development. A build without it has no AI and is otherwise the same program.

## Everyday commands

A `Makefile` at the root wraps the common ones: `make` lists its targets, for example `make dev`, `make check`, `make test`, `make lint`, `make fmt`, `make e2e` (which builds the bundle first), `make baselines`, and `make fuzz FUZZ_SECONDS=60`. Each target runs one of the commands below and adds nothing of its own.

| Task | Command |
| --- | --- |
| Full check (run before pushing) | `scripts/check.sh` |
| Rust tests | `cargo test --workspace` |
| Geometry regression corpus only | `cargo test -p cad-geometry --test regression` (see [`test-data/geometry/`](../test-data/geometry/README.md)) |
| Score the AI corpus against its recordings, per level and language (see [`test-data/ai/`](../test-data/ai/README.md)) | `cargo run -p cad-commands --features testing --example ai-eval -- test-data/ai` |
| Long stress run of the geometry property tests (not part of the check) | `scripts/stress.sh` |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust format | `cargo fmt --all` |
| Crate-boundary check | `scripts/check-boundaries.sh` |
| Dependency licences and advisories (part of the check) | `cargo deny check` / `scripts/check-licenses.sh` |
| Regenerate the third-party notices after a dependency changes ([`docs/provenance.md`](provenance.md)) | `scripts/third-party-notices.sh` (`make notices`) |
| Build WASM (debug / release) | `pnpm --dir web wasm` / `pnpm --dir web wasm:release` |
| Web typecheck, lint, unit tests | `pnpm --dir web typecheck` / `lint` / `test` |
| Production bundle | `pnpm --dir web build` → `web/dist/` |
| End-to-end tests (needs a production bundle) | `pnpm --dir web e2e` |
| Rust benchmarks (store, index, scene, snapping, grips, offset, blocks, annotation, the mixed drawing) | `cargo bench -p cad-benchmarks` |
| Coverage-guided fuzzing of the DXF and DWG readers, the input grammar, and the intent parser (#177, #312, #253), each target for a bounded time; needs `rustup toolchain install nightly && cargo install cargo-fuzz`; a crash lands in `fuzz/artifacts/` and becomes a fixture and a fix | `scripts/fuzz.sh [seconds] [target …]` |
| Open every DWG under a directory and print what the import makes of each (#313) | `scripts/dwg-sweep.sh <dir …>` |
| Read the DWG fixtures and our `AC1015`, `AC1018`, and `AC1021` output of every DXF fixture with LibreDWG's `dwgread` (#315–#317; needs a LibreDWG build, `DWGREAD=/path/to/dwgread`) | `scripts/dwg-check.sh [more .dwg …]` |
| Re-record visual baselines (Chromium) | `E2E_BROWSERS=chromium pnpm --dir web exec playwright test --update-snapshots` |
| Generate a benchmark drawing | `cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf` |
| Render-transport measurement in the browser | `pnpm --dir web bench:transport` (after `build` and generating `lines-1k`, `-10k`, `-100k`) |
| Curve-rendering experiment in the browser | `pnpm --dir web bench:curves` (after `build`) |
| Colour-transport experiment in the browser | `pnpm --dir web bench:styles` (after `build`) |
| Linetype experiment in the browser; rewrites the pictures in ADR-0015 | `pnpm --dir web bench:linetypes` (after `build`) |
| Text-rendering experiment in the browser; rewrites the pictures in ADR-0016 | `pnpm --dir web bench:text` (after `build`) |
| Local-persistence experiment: IndexedDB against OPFS, in Chromium and Firefox (ADR-0019) | `pnpm --dir web bench:persistence` (after `build`, with the generated drawings) |
| The §42 targets on the mixed drawings, in Chromium and Firefox; the figures are in [`docs/performance.md`](performance.md) | `pnpm --dir web bench:mixed` (after `build`, with the mixed drawings generated) |

First-time Playwright setup: `pnpm --dir web exec playwright install chromium firefox`. The suite runs in Chromium and Firefox by default; choose browsers with `E2E_BROWSERS=chromium,firefox,webkit` (WebKit needs `playwright install --with-deps webkit`). CI runs Chromium only, because headless Firefox on the hosted runner provides no WebGL2.

## Contributing

See [`CONTRIBUTING.md`](../CONTRIBUTING.md): start from an issue, run `make check`, ship tests with the change, and sign the [CLA](../CLA.md) on your first pull request.

## Deployment

`web/dist/` is fully static. Serve it from any file server or static host, under any base path. There is no server-side component.

The production build carries a content-security policy in its page: scripts and WebAssembly from its own origin, connections only to it and to the AI service its build names (`VITE_AI_SERVICE_URL`). `web/e2e/security.spec.ts` fails on any violation.

The official app is published from `main` to Cloudflare Pages, at cadslate2d.com, by CI's `publish` job (`scripts/deploy-pages.sh`). The job runs once the repository has the secret `CLOUDFLARE_API_TOKEN` and the variable `CLOUDFLARE_ACCOUNT_ID`; until then it says so and passes. The variable `AI_SERVICE_URL` names the production AI service, `https://api.cadslate2d.com` by default.

## Repository layout

```text
crates/
  cad-core/       points, bounds, entity IDs, tolerances, view math
  cad-geometry/   pure geometric functions
  cad-input/      command-line tokenizer and parser
  cad-text/       the stroke font, its metrics, and the strokes of a string
  cad-document/   drawing, entities, storage, spatial index, transactions
  cad-commands/   Session, command state machines, selection
  cad-dxf/        DXF import/export
  cad-dwg/        DWG import/export (AutoCAD 2000, 2004, 2007/2008)
  cad-intent/     the intent IR a model writes, validated and compiled to commands (ADR-0037)
  cad-render/     RenderScene and OverlayFrame preparation
  cad-wasm/       the JS/WASM boundary (marshalling only)
web/
  src/cad/        TypeScript wrapper around the WASM package
  src/app/        CadApp, frame loop, UI store, key routing, host services
  src/renderer/   WebGL2 adapter (the only code that touches WebGL)
  src/ui/         React shell and theme tokens
  e2e/            Playwright tests and the plain static server they run against
  bench/          Browser measurements, run on demand
scripts/          build-wasm.sh, check.sh, check-boundaries.sh, stress.sh, fuzz.sh, dwg-sweep.sh, dwg-check.sh
Makefile          shortcuts for the everyday commands; `make` lists them
test-data/        DXF and DWG fixtures, geometry regression cases, the intent schema, the AI corpus, screenshots
benches/          cad-benchmarks crate: drawing generator and criterion benchmarks
docs/             DESIGN.md, ADRs, milestone reviews, roadmap
```
