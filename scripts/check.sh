#!/usr/bin/env bash
# The full local check. Run before every push; CI runs the same steps.
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup installs cargo outside PATH for non-login shells.
command -v cargo >/dev/null || . "$HOME/.cargo/env"

step() { printf '\n── %s\n' "$*"; }

step "rust: format";      cargo fmt --all --check
step "rust: clippy";      cargo clippy --workspace --all-targets -- -D warnings
step "rust: tests";       cargo test --workspace --quiet
step "crate boundaries";  scripts/check-boundaries.sh
step "wasm: build";       scripts/build-wasm.sh
step "web: typecheck";    pnpm --dir web typecheck
step "web: lint";         pnpm --dir web lint
step "web: tests";        pnpm --dir web test

printf '\nAll checks passed.\n'
