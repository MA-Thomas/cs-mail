#!/usr/bin/env bash
# Package the same member executable in explicit, offline visual demo mode.
# Run apple-build.sh first. No account, credentials or backend configuration is needed.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Library/Developer/CommandLineTools}"
app="$root/target/apple/CsMailLocalDemo.app"
mkdir -p "$app/Contents/MacOS"
cp "$root/target/apple/CsMail.app/Contents/MacOS/CsMail" "$app/Contents/MacOS/CsMail"
python3 - "$root/apps/cs-mail-apple/Info-macOS.plist" "$app/Contents/Info.plist" <<'PY'
import plistlib, sys
with open(sys.argv[1], 'rb') as source:
    info = plistlib.load(source)
info['CFBundleIdentifier'] = 'com.csqd.member.local-demo'
info['CFBundleName'] = 'cs-mail Local Demo'
info['CsMailVisualDemo'] = True
with open(sys.argv[2], 'wb') as target:
    plistlib.dump(info, target)
PY
codesign --force --sign - --identifier com.csqd.member.local-demo "$app"
echo "Built $app in offline visual demo mode"
