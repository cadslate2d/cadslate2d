#!/usr/bin/env bash
# Builds cad-wasm and generates JS bindings into web/src/cad/pkg (ADR-0004).
# Usage: scripts/build-wasm.sh [--release]
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup installs cargo outside PATH for non-login shells.
command -v cargo >/dev/null || . "$HOME/.cargo/env"

profile_dir=debug
cargo_flags=()
if [[ "${1:-}" == "--release" ]]; then
  profile_dir=release
  cargo_flags+=(--release)
fi

if ! command -v wasm-bindgen >/dev/null; then
  echo "error: wasm-bindgen-cli is not installed. See docs/development.md → Prerequisites." >&2
  exit 1
fi

# The CLI and the crate must be the same version or the bindings will not load.
want=$(cargo tree -p cad-wasm -i wasm-bindgen --depth 0 --prefix none | awk '{ sub(/^v/, "", $2); print $2; exit }')
have=$(wasm-bindgen --version | awk '{ print $2 }')
if [[ "$want" != "$have" ]]; then
  echo "error: wasm-bindgen-cli is $have but the workspace pins wasm-bindgen $want." >&2
  echo "       Run: cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

cargo build -p cad-wasm --target wasm32-unknown-unknown "${cargo_flags[@]}"

out=web/src/cad/pkg
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" \
  "target/wasm32-unknown-unknown/$profile_dir/cad_wasm.wasm"

if [[ $profile_dir == release ]]; then
  # pnpm exec runs inside web/, so pass absolute paths.
  pnpm --dir web exec wasm-opt -Os "$PWD/$out/cad_wasm_bg.wasm" -o "$PWD/$out/cad_wasm_bg.wasm"
fi

echo "wasm: $(wc -c <"$out/cad_wasm_bg.wasm") bytes ($profile_dir) → $out"
