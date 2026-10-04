# ADR-0004: WASM toolchain and build pipeline

**Status:** Proposed  
**Date:** 2026-10-04  
**Kind:** Bootstrap (fills a gap in the baseline; changes no LOCKED decision)  
**Design refs:** §6, §9, §26.3 (criterion 18)

## Context

The baseline locks Rust, WebAssembly, `wasm-bindgen`, TypeScript, React, Vite, and static hosting (§6). It does not say how the WASM artifact is produced, where it lands, or which package manager the web app uses. The workspace scaffold, the dev loop, and CI all depend on the answer.

## Decision

### Rust side

- Stable Rust, pinned in `rust-toolchain.toml`, with the `wasm32-unknown-unknown` target.
- Build in two explicit steps, without `wasm-pack`:

  ```text
  cargo build -p cad-wasm --target wasm32-unknown-unknown [--release]
  wasm-bindgen --target web --out-dir web/src/cad/pkg <path to cad_wasm.wasm>
  ```

- `wasm-bindgen-cli` is pinned to exactly the `wasm-bindgen` crate version in `Cargo.lock`. CI installs that exact version and fails on mismatch.
- `wasm-opt` runs on release builds only.
- The steps live in one script, `scripts/build-wasm.sh`, used by both developers and CI.

### Web side

- `pnpm` as package manager, with a committed lockfile.
- Generated bindings go to `web/src/cad/pkg/` and are git-ignored. `web/src/cad/` holds the hand-written TypeScript wrapper that is the only importer of `pkg/`.
- Vite consumes the `--target web` output directly. No WASM-specific Vite plugin.
- Scripts: `pnpm wasm` (dev build), `pnpm wasm:release`, `pnpm dev`, `pnpm build` (release WASM, then `vite build`).

### Output

`pnpm build` produces a fully static `web/dist/`. Serving that directory from any plain file server is the deployment. No server-side code, no runtime network dependency beyond the files themselves.

Choice of hosting provider is not decided here; it does not affect architecture.

## Alternatives considered

- **`wasm-pack`.** It wraps the same two steps plus npm packaging we do not need, and hides the CLI/crate version coupling. The rustwasm organization that maintained it was sunset in 2025, while `wasm-bindgen` continues under its own organization. Calling the underlying tools directly removes a dependency without adding work.
- **`vite-plugin-wasm` / `vite-plugin-wasm-pack`.** Unnecessary for `--target web` output, and one more thing to break on Vite upgrades.
- **`--target bundler`.** Relies on bundler-specific WASM ESM integration. `--target web` uses only standard `fetch` + `WebAssembly.instantiateStreaming`.
- **Trunk.** Aimed at Rust-first frontends; the baseline frontend is React + Vite.
- **npm instead of pnpm.** Equally workable. pnpm is chosen for a stricter `node_modules` and faster CI installs; swapping later touches only scripts and the lockfile.
- **`cargo xtask` or `just`.** Deferred until there is more than one script to coordinate.

## Consequences

- Contributors need Rust, the wasm target, the pinned `wasm-bindgen-cli`, Node, and pnpm. The README must list them with exact install commands.
- A version mismatch between crate and CLI fails loudly at build time rather than at runtime.
- Rust changes require re-running `pnpm wasm`; there is no automatic watch in the baseline. Add one only if the loop proves painful.
- Criterion 18 (no hidden server dependency) is checked by serving `web/dist/` statically in the E2E run.

## Revisit when

- Build times make the two-step loop painful.
- Threads or other features require non-default target flags or cross-origin isolation headers, which would constrain static hosting.
