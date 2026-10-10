#!/usr/bin/env bash
# Requires full Xcode and rustup targets installed explicitly by the developer.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
case "${1:-simulator}" in
  simulator) rust_target=aarch64-apple-ios-sim; destination='generic/platform=iOS Simulator' ;;
  device) rust_target=aarch64-apple-ios; destination='generic/platform=iOS' ;;
  *) echo 'usage: scripts/apple-ios-build.sh [simulator|device]' >&2; exit 2 ;;
esac
xcodebuild -version
# Generate the same bindings using the host library first.
scripts/apple-build.sh
IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build -p cs-mail-client-ffi --target "$rust_target"
CS_MAIL_RUST_LIBRARY="$root/target/$rust_target/debug/libcs_mail_client_ffi.a" \
  xcodebuild -project apps/cs-mail-apple/CsMail.xcodeproj -scheme CsMail \
  -destination "$destination" -derivedDataPath "$root/target/apple-ios-$rust_target" \
  CODE_SIGNING_ALLOWED=NO build
