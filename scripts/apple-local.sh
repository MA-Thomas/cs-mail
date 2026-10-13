#!/usr/bin/env bash
# Persistent local development backend: existing identity-model enrollment, controlled
# browser logins, fictional enrollment evidence and simulated banking. Never use real
# bank information. The first initialization requires two empty local databases.
set -euo pipefail

: "${CS_MAIL_DEMO_DATABASE_URL:?set CS_MAIL_DEMO_DATABASE_URL to an empty PostgreSQL database}"
: "${CS_MAIL_DEMO_IDENTITY_DATABASE_URL:?set CS_MAIL_DEMO_IDENTITY_DATABASE_URL to another empty database}"
root="$(cd "$(dirname "$0")/.." && pwd)"
port="${CS_MAIL_DEMO_PORT:-8480}"
identity_port="${CS_MAIL_DEMO_IDENTITY_PORT:-8481}"
work="${CS_MAIL_LOCAL_STATE_DIR:-$root/target/apple-local-state}"
mkdir -p "$work"
chmod 700 "$work"
work="$(cd "$work" && pwd)"
# The Python supervisor holds an advisory lock for this entire shell lifetime.
# It releases on exit/crash, so no stale lock file can demand destructive cleanup.
if [[ "${1:-}" != "--locked" ]]; then
  exec python3 - "$work" "$0" <<'PYLOCK'
import fcntl, os, subprocess, sys
with open(os.path.join(sys.argv[1], '.owner.lock'), 'a') as owner:
    os.chmod(owner.name, 0o600)
    try:
        fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        sys.exit('This local development environment is already running.')
    try:
        sys.exit(subprocess.call(['bash', sys.argv[2], '--locked']))
    except KeyboardInterrupt:
        sys.exit(130)
PYLOCK
fi
oidc=""
oidc_port="${CS_MAIL_DEMO_OIDC_PORT:-8482}"
umask 077
server=""
identity=""
cleanup() {
  if [[ -n "$oidc" ]]; then kill "$oidc" 2>/dev/null || true; fi
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi
  if [[ -n "$identity" ]]; then kill "$identity" 2>/dev/null || true; fi
  for child in "$oidc" "$server" "$identity"; do
    if [[ -n "$child" ]]; then wait "$child" 2>/dev/null || true; fi
  done
}
trap cleanup EXIT

step() { printf '\n==> %s\n' "$*"; }
say() { printf -- '--- %s\n' "$*"; }

step "build (simulation feature)"
cargo build -q --manifest-path "$root/Cargo.toml" --features simulation -p cs-maild -p cs-mail-admin
cargo build -q --manifest-path "$root/Cargo.toml" -p cs-mail-test-support --example browser-identity
bin="$root/target/debug"
maild="$bin/cs-maild"
admin="$bin/cs-mail-admin"
echo "work directory: $work"

step "validate durable environment identity"
python3 - "$work" "$port" "$identity_port" "$oidc_port" <<'PYSTATE'
import json, os, socket, subprocess, sys
from urllib.parse import urlsplit
from pathlib import Path
root = Path(sys.argv[1]).resolve()
expected = {'format': 1, 'database': os.environ['CS_MAIL_DEMO_DATABASE_URL'],
            'identity_database': os.environ['CS_MAIL_DEMO_IDENTITY_DATABASE_URL'],
            'ports': list(map(int, sys.argv[2:])), 'directory': str(root)}
for database in (expected['database'], expected['identity_database']):
    parsed = urlsplit(database)
    if parsed.scheme not in ('postgres', 'postgresql') or parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
        sys.exit('Development databases must use explicit loopback PostgreSQL URLs.')
for port in expected['ports']:
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            listener.bind(('127.0.0.1', port))
        except OSError:
            sys.exit('A configured service port is already in use. Stop its owner or use a separate environment.')
path = root / 'environment.json'
if path.exists():
    if json.loads(path.read_text()) != expected:
        sys.exit('Environment configuration changed. Preserve this state directory and its databases; use a separate environment for different settings.')
else:
    if any(root.glob('*.key')):
        sys.exit('Unbound key files exist. Refusing to replace or adopt them automatically.')
    for database in (expected['database'], expected['identity_database']):
        result = subprocess.run(['psql', database, '-XAt', '-c', "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'"], check=True, capture_output=True, text=True)
        if result.stdout.strip() != '0':
            sys.exit('First initialization requires two empty databases. Existing databases are never reset or assigned new keys.')
    temporary = path.with_suffix('.new')
    temporary.write_text(json.dumps(expected))
    temporary.chmod(0o600)
    temporary.replace(path)
    (root / '.create-keys').touch(mode=0o600)
PYSTATE
step "keys (persisted 0600; never printed or replaced)"
for name in provider scheduler operator processor bank-authority bank-ownership \
            identity-decision identity-product evidence relationships principals custody; do
  if [[ -f "$work/.create-keys" ]]; then
    if [[ -e "$work/$name.key" || -e "$work/$name.pub" ]]; then
      echo "Interrupted key initialization: preserve the state directory and inspect it before retrying." >&2
      exit 1
    fi
    "$admin" keygen --out "$work/$name.key" > "$work/$name.pub"
  elif [[ ! -f "$work/$name.key" || ! -f "$work/$name.pub" ]]; then
    echo "A durable key or public record is missing. Refusing to generate a replacement." >&2
    exit 1
  fi
done
rm -f "$work/.create-keys"
public() { python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["verifying_key"]))' "$work/$1.pub"; }
public "identity-product" > "$work/identity-product.verifying"
public "bank-ownership" > "$work/bank-ownership.verifying"

domain='[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]'
account='[9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9]'
identity_issuer="https://identity.localhost"
product="cs-mail/dev"
oidc_issuer="http://127.0.0.1:$oidc_port"
cat > "$work/bootstrap.json" <<EOF
{"provider": 30, "deployment_domain": $domain,
 "financial_scope": {"deployment_domain": $domain, "operator": 30, "program": 1,
                     "payment_account": $account, "protocol_version": 2},
 "unit": 1,
 "provider_key": {"reference": 3, "verifying_key": $(public provider)},
 "scheduler_key": {"reference": 4, "verifying_key": $(public scheduler)},
 "operators": {"1": $(public operator)},
 "payment": {"processor_key": $(public processor), "bank_authority": $(public bank-authority)},
 "identity": {"issuer": "$identity_issuer", "product": "$product",
              "decision_key": $(public identity-decision)}}
EOF
cat > "$work/maild.json" <<EOF
{"database_url": "$CS_MAIL_DEMO_DATABASE_URL", "directory_domain": "csqd-mail.com", "listen": "127.0.0.1:$port",
 "browser_login": {"issuer": "$oidc_issuer", "client_id": "cs-mail-apple", "redirect_uri": "com.csqd.member.development:/oauth/callback"},
 "workers": {"interval_ms": 300, "lease_ms": 30000, "batch": 50},
 "identity": {"endpoint": "http://127.0.0.1:$identity_port/v1/enrollment",
              "product_secret_file": "$work/identity-product.key"},
 "members": {"custody_key_ref":900,"custody_secret_file":"$work/custody.key","provider_secret_file": "$work/provider.key",
             "relationship_secret_file": "$work/relationships.key",
             "principal_secret_file": "$work/principals.key"},
 "simulation": {"processor_secret_file": "$work/processor.key",
                "bank_authority_secret_file": "$work/bank-authority.key",
                "bank_ownership_secret_file": "$work/bank-ownership.key"}}
EOF
cat > "$work/admin.json" <<EOF
{"server": "http://127.0.0.1:$port", "operator": 1,
 "operator_secret_file": "$work/operator.key", "deployment_domain": $domain}
EOF

step "start controlled browser provider and existing identity service"
python3 "$root/scripts/apple-local/oidc-provider.py" --port "$oidc_port" --state-dir "$work/oidc" > "$work/oidc.log" 2>&1 &
oidc=$!
IDENTITY_DATABASE_URL="$CS_MAIL_DEMO_IDENTITY_DATABASE_URL" \
IDENTITY_ISSUER="$identity_issuer" IDENTITY_PRODUCT="$product" \
OIDC_ISSUER="$oidc_issuer" OIDC_CLIENT_ID="cs-mail-apple" \
PRODUCT_KEY_FILE="$work/identity-product.verifying" BANK_KEY_FILE="$work/bank-ownership.verifying" \
DECISION_SECRET_FILE="$work/identity-decision.key" EVIDENCE_SECRET_FILE="$work/evidence.key" \
IDENTITY_PORT="$identity_port" \
  "$root/target/debug/examples/browser-identity" >> "$work/identity.log" 2>&1 &
identity=$!

step "migrate and verify the same deployment"
"$maild" migrate --config "$work/maild.json"
"$maild" bootstrap --config "$work/maild.json" --deployment "$work/bootstrap.json"

serve() {
  "$maild" serve --config "$work/maild.json" >> "$work/serve.log" 2>&1 &
  server=$!
  for _ in $(seq 1 100); do
    curl -sf "http://127.0.0.1:$port/ready" > /dev/null && return 0
    sleep 0.1
  done
  echo "service did not become ready"; return 1
}
serve

step "operator: annual service, request pricing (C and collateral bounds), protocol policy"
cat > "$work/offer.json" <<'EOF'
{"PublishServiceOffer":{"version":1,"period":{"year":2026,"month":1,"day":1,"leap_day":"February28"},"collect_at":1760000000000,"price":12000,"unit":1}}
EOF
cat > "$work/pricing.json" <<'EOF'
{"PublishRequestPricing":{"version":1,"unit":1,"processing_charge":50,"collateral_min":100,"collateral_max":2500}}
EOF
# A day to read and decide through the app during local development.
cat > "$work/protocol.json" <<'EOF'
{"PublishProtocolPolicy":{"version":1,"privacy_profile_version":1,"retention_policy_version":1,
 "submission_window":120000,"decision_window":86400000,"quote_lifetime":120000,
 "reply_lane":{"lifetime":604800000,"rate_limit":{"max_messages":20,"interval":86400000},"purpose":{"Known":"Personal"}},"backoff":[0,1000],"expiry_cooldown":1000,"rejection_cooldown":1000,
 "corporate_basis_points":300,"maturity_delay":1000}}
EOF
for command in offer pricing protocol; do
  "$admin" configure --config "$work/admin.json" --command "$work/$command.json"
done


echo "Local service: http://127.0.0.1:$port"
echo "Persistent development environment · simulated banking and payments."
echo "Use fictional enrollment evidence and choose the intended development login in the browser."
echo "Configuration and redacted logs: $work"
echo "Press Ctrl-C to stop the three local services."
while kill -0 "$server" 2>/dev/null && kill -0 "$identity" 2>/dev/null && kill -0 "$oidc" 2>/dev/null; do
  sleep 1
done
echo "A development service stopped; the companion services are being stopped. State and keys remain preserved." >&2
exit 1
