#!/usr/bin/env bash
# Build the native Rust bridge and SwiftUI macOS development app using local SDKs.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [[ -n "${CS_MAIL_APPLE_SIGNING_IDENTITY:-}" ]]; then
  signer="$CS_MAIL_APPLE_SIGNING_IDENTITY"
elif [[ -f "$root/target/apple-signing/identity.txt" ]]; then
  signer="$(cat "$root/target/apple-signing/identity.txt")"
else
  signer="-"
fi
if [[ "$signer" == "-" ]]; then
  echo "The unified account app requires its original stable code-signing identity. Set up app signing before building." >&2
  exit 1
fi
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
# SwiftPM does not track the external Rust archive as a linker input. Force both products to relink.
rm -f apps/cs-mail-apple/.build/debug/CsMailMac apps/cs-mail-apple/.build/debug/CsMailAppleCheck
swift build --package-path apps/cs-mail-apple --cache-path "$root/target/apple-swift-cache" --disable-sandbox
bundle_identifier="com.csqd.member.dev.continuity-20261012"
destination="$root/target/apple/CsMail-continuity-20261012.app"
mkdir -p "$root/target/apple"
app="$(mktemp -d "$root/target/apple/.CsMail-build.XXXXXX")/CsMail.app"
mkdir -p "$app/Contents/MacOS"
cp apps/cs-mail-apple/.build/debug/CsMailMac "$app/Contents/MacOS/CsMail"
cp apps/cs-mail-apple/Info-macOS.plist "$app/Contents/Info.plist"
python3 - "$app/Contents/Info.plist" "$signer" "$root/target/apple-continuity-state-20261012" <<'PYPLIST'
import os, plistlib, sys
from pathlib import Path
with open(sys.argv[1], 'rb') as source:
    info = plistlib.load(source)
info['CFBundleName'] = 'cs-mail Development'
info['CFBundleIdentifier'] = 'com.csqd.member.dev.continuity-20261012'
info['CsMailPersistentAccountSigning'] = True
info['CsMailClientNamespace'] = 'com.csqd.member.development'
info['CsMailClientStoreDirectory'] = str(Path(sys.argv[3]) / 'device-v3')
info['CsMailDevelopmentService'] = 'http://127.0.0.1:9693'
browser_file = Path('target/apple-signing/login-browser.txt')
browser = os.environ.get('CS_MAIL_LOGIN_BROWSER_BUNDLE_ID')
if browser:
    browser_file.parent.mkdir(parents=True, exist_ok=True)
    browser_file.write_text(browser)
elif browser_file.exists():
    browser = browser_file.read_text().strip()
if browser:
    info['CsMailLoginBrowserBundleID'] = browser
with open(sys.argv[1], 'wb') as target:
    plistlib.dump(info, target)
PYPLIST
signing_identifier="com.csqd.member.dev"
codesign --force --sign "$signer" --identifier "$signing_identifier" "$app"
codesign --verify --strict "$app"
if [[ -d "$destination" ]]; then
  prior_requirement="$(codesign -d -r- "$destination" 2>/dev/null)"
  built_requirement="$(codesign -d -r- "$app" 2>/dev/null)"
  if [[ "$prior_requirement" != "$built_requirement" ]]; then
    echo "Signing requirement changed. The existing account app has not been replaced." >&2
    exit 1
  fi
fi
previous="${app%/CsMail.app}/previous.app"
if [[ -d "$destination" ]]; then mv "$destination" "$previous"; fi
if ! mv "$app" "$destination"; then
  if [[ -d "$previous" ]]; then mv "$previous" "$destination"; fi
  exit 1
fi
rm -rf "${app%/CsMail.app}"
echo "Built $destination"
