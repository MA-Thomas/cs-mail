#!/usr/bin/env bash
# Build the native Rust bridge and SwiftUI macOS development app using local SDKs.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
export MACOSX_DEPLOYMENT_TARGET=14.0
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$root/target/apple-module-cache}"
export SWIFTPM_MODULECACHE_OVERRIDE="$CLANG_MODULE_CACHE_PATH"
mkdir -p "$CLANG_MODULE_CACHE_PATH" "$root/target/apple-swift-cache"
cargo build -p cs-mail-client-ffi --features bindgen
work="$root/target/apple-bindings"
mkdir -p "$work" apps/cs-mail-apple/Generated/CsMailRust apps/cs-mail-apple/Generated/CsMailFFI
"$root/target/debug/cs-mail-bindgen" generate --library "$root/target/debug/libcs_mail_client_ffi.dylib" --language swift --config crates/cs-mail-client-ffi/uniffi.toml --out-dir "$work"
cp "$work/CsMailRust.swift" apps/cs-mail-apple/Generated/CsMailRust/
cp "$work/CsMailFFI.h" apps/cs-mail-apple/Generated/CsMailFFI/
cp "$work/CsMailFFI.modulemap" apps/cs-mail-apple/Generated/CsMailFFI/module.modulemap
swift build --package-path apps/cs-mail-apple --cache-path "$root/target/apple-swift-cache" --disable-sandbox
app="$root/target/apple/CsMail.app"
mkdir -p "$app/Contents/MacOS"
cp apps/cs-mail-apple/.build/debug/CsMailMac "$app/Contents/MacOS/CsMail"
cp apps/cs-mail-apple/Info-macOS.plist "$app/Contents/Info.plist"
codesign --force --sign - --identifier com.csqd.member.dev "$app"
echo "Built $app"
