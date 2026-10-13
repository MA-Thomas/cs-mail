#!/usr/bin/env bash
# Select an existing valid code-signing identity. Never changes certificate trust.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
python3 - "$root/target/apple-signing" <<'PY'
import os, re, subprocess, sys
from pathlib import Path
state = Path(sys.argv[1])
state.mkdir(mode=0o700, parents=True, exist_ok=True)
state.chmod(0o700)
result = subprocess.run(['security', 'find-identity', '-v', '-p', 'codesigning'], capture_output=True, text=True)
if result.returncode:
    sys.exit('Could not inspect code-signing identities in Keychain.')
identities = re.findall(r'\) ([0-9A-F]{40}) "([^"]+)"', result.stdout)
configured = os.environ.get('CS_MAIL_APPLE_SIGNING_IDENTITY')
record = state / 'identity.txt'
if record.exists():
    configured = configured or record.read_text().strip()
if configured:
    identities = [item for item in identities if configured in item]
if len(identities) != 1:
    sys.exit('Set CS_MAIL_APPLE_SIGNING_IDENTITY to one existing valid signing identity. No certificate or trust setting was changed.')
fingerprint, name = identities[0]
record.write_text(fingerprint + '\n')
record.chmod(0o600)
print('Selected existing signer: ' + name + '. Certificate trust was not changed.')
PY
