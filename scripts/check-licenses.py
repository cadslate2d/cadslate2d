#!/usr/bin/env python3
"""The licences of the web app's production dependencies (#296, ADR-0035).

Reads `pnpm licenses list --prod --json` from stdin. Everything the bundle
ships must be under a licence compatible with the repository's
AGPL-3.0-or-later; dev dependencies are tooling and are not shipped.
"""
import json
import sys

ALLOWED = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "0BSD", "Zlib", "MPL-2.0", "CC0-1.0", "Unlicense"}

report = json.load(sys.stdin)
bad = [
    f"{package['name']}@{package.get('version', '?')}: {licence}"
    for licence, packages in sorted(report.items())
    if licence not in ALLOWED
    for package in packages
]
if bad:
    print("licences: production dependencies outside the allow list:", file=sys.stderr)
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
print("licences: web production dependencies ok (" + ", ".join(sorted(report)) + ")")
