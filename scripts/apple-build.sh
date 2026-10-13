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
if [[ "${CS_MAIL_APPLE_REQUIRE_STABLE_SIGNING:-0}" == "1" && "$signer" == "-" ]]; then
  echo "Persistent account packaging requires a stable code-signing identity. Set CS_MAIL_APPLE_SIGNING_IDENTITY; ad hoc signing is for visual review only." >&2
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
swift build --package-path apps/cs-mail-apple --cache-path "$root/target/apple-swift-cache" --disable-sandbox
environment_name="${CS_MAIL_APPLE_ENVIRONMENT_NAME:-}"
if [[ -n "$environment_name" ]] && ! [[ "$environment_name" =~ ^[a-z0-9][a-z0-9-]{0,47}$ ]]; then
  echo "CS_MAIL_APPLE_ENVIRONMENT_NAME must be a lowercase name of at most 48 characters." >&2
  exit 1
fi
bundle_identifier="com.csqd.member.dev${environment_name:+.$environment_name}"
destination="$root/target/apple/CsMail${environment_name:+-$environment_name}.app"
mkdir -p "$root/target/apple"
app="$(mktemp -d "$root/target/apple/.CsMail-build.XXXXXX")/CsMail.app"
mkdir -p "$app/Contents/MacOS"
cp apps/cs-mail-apple/.build/debug/CsMailMac "$app/Contents/MacOS/CsMail"
cp apps/cs-mail-apple/Info-macOS.plist "$app/Contents/Info.plist"
python3 - "$app/Contents/Info.plist" "$signer" "${CS_MAIL_LOCAL_STATE_DIR:-$root/target/apple-local-state}" <<'PYPLIST'
import json, os, plistlib, sys
from pathlib import Path
from urllib.parse import urlsplit
with open(sys.argv[1], 'rb') as source:
    info = plistlib.load(source)
info['CFBundleName'] = 'cs-mail Development'
info['CsMailPersistentAccountSigning'] = sys.argv[2] != '-'
name = os.environ.get('CS_MAIL_APPLE_ENVIRONMENT_NAME')
if name:
    info['CFBundleIdentifier'] = f'com.csqd.member.dev.{name}'
    info['CsMailClientNamespace'] = f'com.csqd.member.development.{name}'
    info['CFBundleName'] = f'cs-mail Development ({name})'
credential = os.environ.get('CS_MAIL_APPLE_CREDENTIAL_SERVICE')
store = os.environ.get('CS_MAIL_APPLE_STORE_DIRECTORY')
if credential or store:
    if credential != 'com.csqd.member.development' or not store or not Path(store).is_absolute():
        sys.exit('Account continuity requires the original credential service and an explicit absolute cache directory.')
    info['CsMailClientNamespace'] = credential
    info['CsMailClientStoreDirectory'] = store
environment = Path(sys.argv[3]) / 'environment.json'
service = os.environ.get('CS_MAIL_DEVELOPMENT_SERVICE_URL')
if not service:
    port = json.loads(environment.read_text())['ports'][0] if environment.exists() else 8480
    service = f'http://127.0.0.1:{port}'
parsed = urlsplit(service)
if parsed.scheme != 'http' or parsed.hostname not in ('localhost', '127.0.0.1', '::1') or parsed.username or parsed.password:
    sys.exit('The development app requires a loopback HTTP service URL.')
info['CsMailDevelopmentService'] = service
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
signing_identifier="$bundle_identifier"
if [[ "${CS_MAIL_APPLE_CREDENTIAL_SERVICE:-}" == "com.csqd.member.development" ]]; then
  signing_identifier="com.csqd.member.dev"
fi
codesign --force --sign "$signer" --identifier "$signing_identifier" "$app"
codesign --verify --strict "$app"
previous="${app%/CsMail.app}/previous.app"
if [[ -d "$destination" ]]; then mv "$destination" "$previous"; fi
if ! mv "$app" "$destination"; then
  if [[ -d "$previous" ]]; then mv "$previous" "$destination"; fi
  exit 1
fi
rm -rf "${app%/CsMail.app}"
echo "Built $destination"
