#!/usr/bin/env bash
# Enforces ADR-0001: crate dependency direction and the rule that JS-facing
# crates appear only in cad-wasm. Checks direct dependencies in each manifest;
# a transitive violation is impossible without a direct one.
set -euo pipefail
cd "$(dirname "$0")/.."

declare -A ALLOWED=(
  [cad-core]=""
  [cad-geometry]="cad-core"
  [cad-input]="cad-core"
  [cad-text]="cad-core"
  [cad-document]="cad-core cad-geometry cad-text"
  [cad-commands]="cad-core cad-geometry cad-input cad-document"
  [cad-dxf]="cad-core cad-document"
  [cad-dwg]="cad-core cad-document cad-dxf"
  [cad-render]="cad-core cad-geometry cad-document cad-commands"
  [cad-plot]="cad-core cad-geometry cad-document cad-commands cad-render"
  [cad-wasm]="cad-core cad-geometry cad-document cad-commands cad-render cad-dxf cad-dwg cad-plot"
)
JS_ONLY='wasm-bindgen|js-sys|web-sys|serde-wasm-bindgen|console_error_panic_hook'

fail=0
for manifest in crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$manifest")")
  if [[ ! -v ALLOWED[$crate] ]]; then
    echo "boundary: unknown crate '$crate' — add it to ADR-0001 and this script"; fail=1; continue
  fi
  # Dependency keys: lines like `name = ...` or `name.workspace = true` outside [package]/[lib]/[lints].
  deps=$(awk '
    /^\[/ { in_deps = ($0 ~ /dependencies\]$/) ; next }
    in_deps && /^[A-Za-z0-9_-]+/ { split($0, a, /[ .=]/); print a[1] }
  ' "$manifest" | sort -u)
  for dep in $deps; do
    if [[ $dep == cad-* ]] && ! grep -qw -- "$dep" <<<"${ALLOWED[$crate]}"; then
      echo "boundary: $crate must not depend on $dep (ADR-0001)"; fail=1
    fi
    if [[ $crate != cad-wasm ]] && grep -Eqx -- "$JS_ONLY" <<<"$dep"; then
      echo "boundary: $dep is only allowed in cad-wasm, found in $crate (ADR-0001)"; fail=1
    fi
  done
done
[[ $fail -eq 0 ]] && echo "boundary: ok"
exit $fail
