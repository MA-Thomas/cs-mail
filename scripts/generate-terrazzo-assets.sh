#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
work="$(mktemp -d "${TMPDIR:-/tmp}/cs-mail-terrazzo.XXXXXX")"
trap 'rm -rf "$work"' EXIT
swiftc -module-cache-path "$root/target/apple-module-cache" -O -D TERRAZZO_ASSET_GENERATOR tools/terrazzo-assets/main.swift \
  apps/cs-mail-apple/Sources/CsMailPresentation/Interfaces/B/InterfaceBTerrazzoMinerals.swift \
  -o "$work/generate"
"$work/generate" apps/cs-mail-apple/Sources/CsMailPresentation/Resources "$@"
