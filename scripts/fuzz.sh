#!/usr/bin/env bash
# Runs the coverage-guided fuzzers (#177) for a bounded time each. Not part
# of scripts/check.sh: it needs a nightly toolchain and cargo-fuzz —
#   rustup toolchain install nightly && cargo install cargo-fuzz
#
#   scripts/fuzz.sh [seconds per target] [target …]
#
# The corpus each target grows is kept under fuzz/corpus/<target>/ (not
# committed); a crash lands in fuzz/artifacts/<target>/ and becomes a
# fixture under test-data/dxf/malformed/ or test-data/dwg/malformed/ (or a
# row of cad-input's tests)
# and a fix, in the same commit.
set -euo pipefail
cd "$(dirname "$0")/.."

seconds="${1:-600}"
shift || true
targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  targets=(dxf_import input_resolve dwg_read)
fi

if ! command -v cargo-fuzz >/dev/null; then
  echo "cargo-fuzz is not installed: rustup toolchain install nightly && cargo install cargo-fuzz" >&2
  exit 1
fi

# Seeds: every DXF fixture, valid, malformed, and from the corpus; every
# DWG fixture for the DWG reader.
mkdir -p fuzz/corpus/dxf_import fuzz/corpus/input_resolve fuzz/corpus/dwg_read
for file in test-data/dxf/valid/*.dxf test-data/dxf/malformed/*.dxf test-data/dxf/corpus/*.dxf; do
  cp -n "$file" "fuzz/corpus/dxf_import/$(basename "$file")" 2>/dev/null || true
done
for file in test-data/dwg/valid/*.dwg test-data/dwg/malformed/*.dwg; do
  cp -n "$file" "fuzz/corpus/dwg_read/$(basename "$file")" 2>/dev/null || true
done
# And this crate's own output of every DXF fixture (#315): the reader fed
# what the writer makes.
if cargo build -q --release -p cad-dwg --example write 2>/dev/null; then
  for dxf in test-data/dxf/valid/*.dxf; do
    name=$(basename "$dxf" .dxf)
    for version in AC1015 AC1018 AC1021; do
      target/release/examples/write "$dxf" "fuzz/corpus/dwg_read/ours-$name.$version.dwg" "$version" >/dev/null 2>&1 || true
    done
  done
fi

# The host triple, named: a cargo-fuzz built for musl would otherwise pick
# the musl target, where sanitizers do not link.
host=$(rustc +nightly -vV | sed -n 's/^host: //p')

for target in "${targets[@]}"; do
  echo "== fuzz: $target for ${seconds}s"
  cargo +nightly fuzz run --target "$host" "$target" -- -max_total_time="$seconds" -max_len=65536 -timeout=30
done
