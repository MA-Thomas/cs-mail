#!/usr/bin/env bash
# Local SwiftUI backend: fresh disposable databases, real identity-model enrollment,
# fictional browser logins and simulated banking. Never use real applicant information.
# Set CS_MAIL_DEMO_DATABASE_URL and CS_MAIL_DEMO_IDENTITY_DATABASE_URL to EMPTY databases.
set -euo pipefail

: "${CS_MAIL_DEMO_DATABASE_URL:?set CS_MAIL_DEMO_DATABASE_URL to an empty PostgreSQL database}"
: "${CS_MAIL_DEMO_IDENTITY_DATABASE_URL:?set CS_MAIL_DEMO_IDENTITY_DATABASE_URL to another empty database}"
root="$(cd "$(dirname "$0")/.." && pwd)"
port="${CS_MAIL_DEMO_PORT:-8480}"
identity_port="${CS_MAIL_DEMO_IDENTITY_PORT:-8481}"
work="$(mktemp -d)"
oidc=""
oidc_port="${CS_MAIL_DEMO_OIDC_PORT:-8482}"
umask 077
server=""
identity=""
cleanup() {
  if [[ -n "$oidc" ]]; then kill "$oidc" 2>/dev/null || true; fi
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi
  if [[ -n "$identity" ]]; then kill "$identity" 2>/dev/null || true; fi
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

step "keys (secrets are written 0600 and never printed)"
for name in provider scheduler operator processor bank-authority bank-ownership \
            identity-decision identity-product evidence relationships principals; do
  "$admin" keygen --out "$work/$name.key" > "$work/$name.pub"
done
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
{"database_url": "$CS_MAIL_DEMO_DATABASE_URL", "listen": "127.0.0.1:$port",
 "browser_login": {"issuer": "$oidc_issuer", "client_id": "cs-mail-apple", "redirect_uri": "com.csqd.member:/oauth/callback"},
 "workers": {"interval_ms": 300, "lease_ms": 30000, "batch": 50},
 "identity": {"endpoint": "http://127.0.0.1:$identity_port/v1/enrollment",
              "product_secret_file": "$work/identity-product.key"},
 "members": {"provider_secret_file": "$work/provider.key",
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
python3 "$root/scripts/apple-local/oidc-provider.py" --port "$oidc_port" > "$work/oidc.log" 2>&1 &
oidc=$!
IDENTITY_DATABASE_URL="$CS_MAIL_DEMO_IDENTITY_DATABASE_URL" \
IDENTITY_ISSUER="$identity_issuer" IDENTITY_PRODUCT="$product" \
OIDC_ISSUER="$oidc_issuer" OIDC_CLIENT_ID="cs-mail-apple" \
PRODUCT_KEY_FILE="$work/identity-product.verifying" BANK_KEY_FILE="$work/bank-ownership.verifying" \
DECISION_SECRET_FILE="$work/identity-decision.key" EVIDENCE_SECRET_FILE="$work/evidence.key" \
IDENTITY_PORT="$identity_port" \
  "$root/target/debug/examples/browser-identity" >> "$work/identity.log" 2>&1 &
identity=$!

step "migrate and bootstrap an empty database"
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
 "backoff":[0,1000],"expiry_cooldown":1000,"rejection_cooldown":1000,
 "corporate_basis_points":300,"maturity_delay":1000}}
EOF
for command in offer pricing protocol; do
  "$admin" configure --config "$work/admin.json" --command "$work/$command.json"
done


echo "Local service: http://127.0.0.1:$port"
echo "Sign in through the app using fictional details, then choose Alice or Bob in the browser."
echo "Configuration and redacted logs: $work"
echo "Press Ctrl-C to stop the three local services."
wait "$server" "$identity" "$oidc"
