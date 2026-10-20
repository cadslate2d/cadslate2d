#!/usr/bin/env bash
# The third-party notices shipped with the build (#296, ADR-0035):
# web/public/third-party-notices.txt — every Rust crate the WebAssembly
# core contains and every JavaScript package the bundle contains, each
# with its licence text, and the Hershey font's acknowledgements, which
# its terms require to travel with the font data.
#
#   scripts/third-party-notices.sh          writes the file
#   scripts/third-party-notices.sh --check  fails when it is out of date
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || . "$HOME/.cargo/env"

out=web/public/third-party-notices.txt
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

{
  echo "CadSlate2D — third-party notices"
  echo
  echo "CadSlate2D itself is free software under the GNU Affero General Public"
  echo "License, version 3 or later (see LICENSE in the repository). It contains"
  echo "the following third-party work, under the licences reproduced below."
  echo
  echo "################################################################################"
  echo "# The stroke font"
  echo "################################################################################"
  echo
  cat crates/cad-text/fonts/LICENSE.md
  echo
  echo "################################################################################"
  echo "# JavaScript packages in the web bundle"
  echo "################################################################################"
  echo
  python3 scripts/third-party-notices.py
  echo "################################################################################"
  echo "# Rust crates in the WebAssembly core"
  echo "################################################################################"
  echo
  cargo about generate --manifest-path crates/cad-wasm/Cargo.toml --config about.toml about.hbs
} > "$tmp"

if [[ "${1:-}" == "--check" ]]; then
  if ! diff -q "$out" "$tmp" >/dev/null 2>&1; then
    echo "third-party notices: $out is out of date; run scripts/third-party-notices.sh" >&2
    diff "$out" "$tmp" | head -20 >&2 || true
    exit 1
  fi
  echo "third-party notices: up to date"
else
  mkdir -p "$(dirname "$out")"
  mv "$tmp" "$out"
  trap - EXIT
  echo "wrote $out ($(wc -l < "$out") lines)"
fi
