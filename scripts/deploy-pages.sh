#!/usr/bin/env bash
# Publishes the built app (web/dist, built with the production AI
# service's address) to Cloudflare Pages: the project `cadslate2d`, its
# production branch `main`, served at cadslate2d.com (the AI service's
# go-live issue, #14 there).
#
#   CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… scripts/deploy-pages.sh [DIST]
#
# The token needs "Cloudflare Pages: Edit" on the account. Without it the
# script says so and does nothing, so CI passes before the project exists.
set -euo pipefail
dist=${1:-web/dist}
if [ -z "${CLOUDFLARE_API_TOKEN:-}" ] || [ -z "${CLOUDFLARE_ACCOUNT_ID:-}" ]; then
  echo "CLOUDFLARE_API_TOKEN or CLOUDFLARE_ACCOUNT_ID is not set: nothing published."
  exit 0
fi
grep -q 'http-equiv="Content-Security-Policy"' "$dist/index.html" || {
  echo "$dist/index.html has no content-security policy: not the production build." >&2
  exit 1
}
# What the published app reads, for the AI service's deploy to wait on:
# the service goes to production only once the app reads the intent
# version it writes (#347).
intent=$(sed -n 's/^pub const VERSION: u32 = \([0-9]*\);/\1/p' crates/cad-intent/src/lib.rs)
[ -n "$intent" ] || { echo "cad_intent::VERSION not found" >&2; exit 1; }
printf '{"commit":"%s","intent":%s}\n' "${GITHUB_SHA:-$(git rev-parse HEAD)}" "$intent" > "$dist/version.json"
# Pinned: a version published well over two weeks ago (AGENTS.md).
wrangler=(npx --yes wrangler@4.20.0)
"${wrangler[@]}" pages project create cadslate2d --production-branch=main 2>/dev/null || true
"${wrangler[@]}" pages deploy "$dist" --project-name=cadslate2d --branch=main --commit-dirty=true
