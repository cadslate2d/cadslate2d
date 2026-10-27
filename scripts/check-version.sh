#!/usr/bin/env bash
# The program has one version: the workspace's (Cargo.toml), which the
# About box shows as the core's, and the web app's (web/package.json).
# They move together, by semantic versioning from 0.1.0.
set -euo pipefail
cd "$(dirname "$0")/.."
core=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
web=$(sed -n 's/^  "version": "\(.*\)",/\1/p' web/package.json)
if [ -z "$core" ] || [ "$core" != "$web" ]; then
  echo "Cargo.toml says ${core:-nothing}, web/package.json ${web:-nothing}: one version, set in both." >&2
  exit 1
fi
echo "version $core"
