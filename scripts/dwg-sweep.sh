#!/usr/bin/env bash
# Opens every .dwg under the directories given and prints what the import
# makes of each (#313): the version, the report's summary, or the error.
# For drawings that are not fixtures, such as LibreDWG's samples (not
# committed: their provenance is mixed). Not part of scripts/check.sh.
#
#   scripts/dwg-sweep.sh /path/to/drawings [...]
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || . "$HOME/.cargo/env"
cargo run --release -q -p cad-dwg --example sweep -- "$@"
