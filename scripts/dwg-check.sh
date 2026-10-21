#!/usr/bin/env bash
# Reads every DWG fixture, and this crate's AC1015 and AC1018 output of every DXF
# fixture, with LibreDWG's dwgread (#315, ADR-0036 §3): the second oracle.
# Not part of scripts/check.sh: it needs a LibreDWG build, found as
# `dwgread` on PATH or named by DWGREAD.
#
#   scripts/dwg-check.sh [extra .dwg files or directories...]
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || . "$HOME/.cargo/env"
dwgread="${DWGREAD:-dwgread}"
if ! command -v "$dwgread" >/dev/null; then
  echo "dwgread is not installed: build LibreDWG and name it with DWGREAD=/path/to/dwgread" >&2
  exit 1
fi

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
# Our output of every DXF fixture, written once.
cargo build -q --release -p cad-dwg --example write
for dxf in test-data/dxf/valid/*.dxf; do
  name=$(basename "$dxf" .dxf)
  for version in AC1015 AC1018; do
    target/release/examples/write "$dxf" "$out/$name.$version.dwg" "$version" >/dev/null 2>&1 || true
  done
done

# dwgread prints SUCCESS and exits 0 when a file reads; its log at -v2
# names any error it recovered from, which counts as a failure here.
check() {
  local label=$1
  shift
  local ok=0 failed=0 file log
  for file in "$@"; do
    if log=$("$dwgread" -v2 "$file" 2>&1) && ! grep -qE 'ERROR' <<<"$log"; then
      ok=$((ok + 1))
    else
      failed=$((failed + 1))
      echo "FAIL $file"
      grep -E 'ERROR' <<<"$log" | head -3 | sed 's/^/  /'
    fi
  done
  echo "$label: $ok read without an error, $failed with errors"
  return "$failed"
}

# The fixtures are the independent writer's (and two are crafted to be
# unreadable): their errors are reported, not failed on.
check "fixtures" test-data/dwg/valid/*.dwg || true
extra=()
for arg in "$@"; do
  if [ -d "$arg" ]; then
    while IFS= read -r f; do extra+=("$f"); done < <(find "$arg" -iname '*.dwg' | sort)
  else
    extra+=("$arg")
  fi
done
if [ "${#extra[@]}" -gt 0 ]; then
  check "given" "${extra[@]}" || true
fi
# Our output must read without an error: the acceptance of #315.
check "our AC1015 and AC1018 output" "$out"/*.dwg
