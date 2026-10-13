#!/usr/bin/env bash
# Build the unified development app for a persistent account, using stable signing.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export CS_MAIL_APPLE_REQUIRE_STABLE_SIGNING=1
exec bash "$root/scripts/apple-build.sh"
