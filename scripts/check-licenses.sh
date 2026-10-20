#!/usr/bin/env bash
# The licences of the web app's production dependencies (#296, ADR-0035):
# everything the bundle ships must be under a licence compatible with the
# repository's AGPL-3.0-or-later. Part of scripts/check.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
pnpm --dir web licenses list --prod --json | python3 scripts/check-licenses.py
