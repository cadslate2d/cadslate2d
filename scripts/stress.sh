#!/usr/bin/env bash
# The long stress run (DESIGN §31, §41, §43): every property test of the
# geometry, document, and DXF crates, with many more random cases than the
# normal check — the DXF ones are the fuzz run against hostile files — and
# the DXF reader's time budget on a hundred-megabyte file.
#
#   scripts/stress.sh            200,000 cases per property
#   scripts/stress.sh 2000000    as many as you have patience for
#
# It is not part of scripts/check.sh: it takes minutes, and what it looks
# for is rare. A failure is printed with its smallest failing input and
# saved next to the test in a proptest-regressions file, which the normal
# check replays from then on. Fix the bug, add a line to the corpus under
# test-data/geometry/ where one fits, and commit both.
set -euo pipefail
cd "$(dirname "$0")/.."

CASES="${1:-200000}"
echo "Stress run: ${CASES} cases per property."

# Tests that discard unsuitable inputs need room to discard in proportion.
PROPTEST_CASES="${CASES}" \
PROPTEST_MAX_GLOBAL_REJECTS="${CASES}" \
PROPTEST_MAX_LOCAL_REJECTS="${CASES}" \
  cargo test --release -p cad-geometry -p cad-document -p cad-dxf

# The reader's work is proportional to the bytes read: a hundred megabytes
# of groups within the budget the test states.
cargo test --release -p cad-dxf -- --ignored budget
