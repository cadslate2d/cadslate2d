#!/usr/bin/env bash
# Runs the coverage-guided fuzzers (#177) for a bounded time each. Not part
# of scripts/check.sh: it needs a nightly toolchain and cargo-fuzz —
#   rustup toolchain install nightly && cargo install cargo-fuzz
#
#   scripts/fuzz.sh [seconds per target] [target …]
#
# The corpus each target grows is kept under fuzz/corpus/<target>/ (not
# committed); a crash lands in fuzz/artifacts/<target>/ and becomes a
# fixture under test-data/dxf/malformed/ (or a row of cad-input's tests)
# and a fix, in the same commit.
set -euo pipefail
cd "$(dirname "$0")/.."

seconds="${1:-600}"
shift || true
targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  targets=(dxf_import input_resolve)
fi

if ! command -v cargo-fuzz >/dev/null; then
  echo "cargo-fuzz is not installed: rustup toolchain install nightly && cargo install cargo-fuzz" >&2
  exit 1
fi

# Seeds: every DXF fixture, valid, malformed, and from the corpus.
mkdir -p fuzz/corpus/dxf_import fuzz/corpus/input_resolve
for file in test-data/dxf/valid/*.dxf test-data/dxf/malformed/*.dxf test-data/dxf/corpus/*.dxf; do
  cp -n "$file" "fuzz/corpus/dxf_import/$(basename "$file")" 2>/dev/null || true
done

for target in "${targets[@]}"; do
  echo "== fuzz: $target for ${seconds}s"
  cargo +nightly fuzz run "$target" -- -max_total_time="$seconds" -max_len=65536 -timeout=30
done
