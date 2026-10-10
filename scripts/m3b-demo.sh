#!/usr/bin/env bash
# Milestone 3b demonstration: guests write to a member from the request page, from clean
# databases. Bob enrolls and publishes request classes; guests verify an email address,
# authorize one charge on a card and send, in headless Chromium. Bob decides with the
# developer CLI. Requests end accepted, rejected, cancelled before sending and expired; a
# declined card makes no request; a second address with the same card is tied to the
# first address's history. Notices go to a development outbox, DKIM-signed. Finally no
# guest message text, secret or database URL may appear in the database or the log.
#
#   CS_MAIL_DEMO_DATABASE_URL=postgresql://.../empty_db \
#   CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://.../another_empty_db \
#     scripts/m3b-demo.sh
#
# Both databases must be empty. Enrollment runs through the identity-model enrollment
# service with a development login in place of an OIDC provider (the
# `cs-mail-test-support` example `dev-identity`); it proves nothing about a login. The
# browser half is scripts/m3b-browser.py: it needs Python Playwright and its Chromium, and
# pg_dump for the database check.
set -euo pipefail

: "${CS_MAIL_DEMO_DATABASE_URL:?set CS_MAIL_DEMO_DATABASE_URL to an empty PostgreSQL database}"
: "${CS_MAIL_DEMO_IDENTITY_DATABASE_URL:?set CS_MAIL_DEMO_IDENTITY_DATABASE_URL to another empty database}"
root="$(cd "$(dirname "$0")/.." && pwd)"
port="${CS_MAIL_DEMO_PORT:-8480}"
identity_port="${CS_MAIL_DEMO_IDENTITY_PORT:-8481}"
work="$(mktemp -d)"
server=""
identity=""
cleanup() {
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi
  if [[ -n "$identity" ]]; then kill "$identity" 2>/dev/null || true; fi
}
trap cleanup EXIT

step() { printf '\n==> %s\n' "$*"; }
say() { printf -- '--- %s\n' "$*"; }

step "build (simulation feature)"
cargo build -q --manifest-path "$root/Cargo.toml" --features simulation -p cs-maild -p cs-mail-admin
cargo build -q --manifest-path "$root/Cargo.toml" -p cs-mail-cli
cargo build -q --manifest-path "$root/Cargo.toml" -p cs-mail-test-support --example dev-identity
bin="$root/target/debug"
maild="$bin/cs-maild"
admin="$bin/cs-mail-admin"
cli="$bin/cs-mail-cli"
echo "work directory: $work"

step "keys (secrets are written 0600 and never printed)"
for name in provider scheduler operator processor bank-authority bank-ownership \
            identity-decision identity-product evidence relationships principals guest-content; do
  "$admin" keygen --out "$work/$name.key" > "$work/$name.pub"
done
(umask 077; openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$work/dkim.pem" 2>/dev/null)
public() { python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["verifying_key"]))' "$work/$1.pub"; }
public "identity-product" > "$work/identity-product.verifying"
public "bank-ownership" > "$work/bank-ownership.verifying"

domain='[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]'
account='[9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9]'
identity_issuer="https://identity.localhost"
product="cs-mail/dev"
oidc_issuer="https://login.dev.invalid"
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
 "workers": {"interval_ms": 300, "lease_ms": 30000, "batch": 50},
 "identity": {"endpoint": "http://127.0.0.1:$identity_port/v1/enrollment",
              "product_secret_file": "$work/identity-product.key"},
 "members": {"provider_secret_file": "$work/provider.key",
             "relationship_secret_file": "$work/relationships.key",
             "principal_secret_file": "$work/principals.key"},
 "simulation": {"processor_secret_file": "$work/processor.key",
                "bank_authority_secret_file": "$work/bank-authority.key",
                "bank_ownership_secret_file": "$work/bank-ownership.key"},
 "external": {"content_secret_file": "$work/guest-content.key", "mail_domain": "mail.localhost",
              "delivery": {"kind": "development", "directory": "$work/outbox"},
              "feedback": {"kind": "disabled"}},
 "guests": {"public_url": "http://127.0.0.1:$port", "content_secret_file": "$work/guest-content.key", "mail_domain": "mail.localhost",
            "notices": {"kind": "development", "dkim_selector": "cs1", "dkim_secret_file": "$work/dkim.pem",
            "directory": "$work/outbox"}}}
EOF
cat > "$work/admin.json" <<EOF
{"server": "http://127.0.0.1:$port", "operator": 1,
 "operator_secret_file": "$work/operator.key", "deployment_domain": $domain}
EOF

step "start the development identity service"
IDENTITY_DATABASE_URL="$CS_MAIL_DEMO_IDENTITY_DATABASE_URL" \
IDENTITY_ISSUER="$identity_issuer" IDENTITY_PRODUCT="$product" \
OIDC_ISSUER="$oidc_issuer" OIDC_CLIENT_ID="cs-mail-cli" \
PRODUCT_KEY_FILE="$work/identity-product.verifying" BANK_KEY_FILE="$work/bank-ownership.verifying" \
DECISION_SECRET_FILE="$work/identity-decision.key" EVIDENCE_SECRET_FILE="$work/evidence.key" \
IDENTITY_PORT="$identity_port" \
  "$root/target/debug/examples/dev-identity" >> "$work/identity.log" 2>&1 &
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
# Short windows so expiry happens during the demonstration.
cat > "$work/protocol.json" <<'EOF'
{"PublishProtocolPolicy":{"version":1,"privacy_profile_version":1,"retention_policy_version":1,
 "submission_window":120000,"decision_window":20000,"quote_lifetime":120000,
 "backoff":[0,1000],"expiry_cooldown":1000,"rejection_cooldown":1000,
 "corporate_basis_points":300,"maturity_delay":1000}}
EOF
for command in offer pricing protocol; do
  "$admin" configure --config "$work/admin.json" --command "$work/$command.json"
done

# Every member has their own bank account, address and phone: enrollment evidence that
# overlaps another enrollment's would be held for operator review.
person() { # name number
  local name="$1" n="$2" home="$work/$1"
  "$cli" --home "$home" init --server "http://127.0.0.1:$port" > /dev/null
  cat > "$home/bank.json" <<EOF
{"account": "acct-$name", "owner": {"name": "$name Example", "address": "$n Demo Street", "phone": "+1 555 010 $n"}}
EOF
  cat > "$home/applicant.json" <<EOF
{"name": "$name Example", "date_of_birth": "1990-01-$(printf %02d "$n")", "address": "$n Demo Street", "phone": "+1 555 010 $n"}
EOF
  local begun challenge now token
  begun="$("$cli" --home "$home" enroll-begin --bank "$home/bank.json")"
  challenge="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["challenge"])' <<< "$begun")"
  now="$(date +%s)"
  token="dev-oidc.$(python3 -c 'import json,sys
c={"issuer":sys.argv[1],"subject":sys.argv[2],"audience":"cs-mail-cli","nonce":sys.argv[3],
   "auth_time":int(sys.argv[4])-1,"issued_at":int(sys.argv[4])-1,"expires_at":int(sys.argv[4])+300}
print(json.dumps(c).encode().hex())' "$oidc_issuer" "login-$name" "$challenge" "$now")"
  "$cli" --home "$home" enroll-complete --oidc-token "$token" --login-issuer "$oidc_issuer" \
    --login-subject "login-$name" --applicant "$home/applicant.json" --bank "$home/bank.json" \
    > "$home/enrolled.json"
  "$cli" --home "$home" publish-content-key > /dev/null
  "$cli" --home "$home" purchase --offer 1 > /dev/null
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["persona"])' "$home/enrolled.json"
}
as() { local who="$1"; shift; printf '\n$ %s: cs-mail-cli %s\n' "$who" "$*"; "$cli" --home "$work/$who" "$@"; }

step "enroll Bob, who buys annual service"
bob=$(person Bob 1)
echo "Bob $bob"
for _ in $(seq 1 60); do
  "$cli" --home "$work/Bob" account | grep -q "covered now" && break
  sleep 0.5
done

if [[ "${CS_MAIL_DEMO_EXTERNAL:-0}" == 1 ]]; then
  source "$root/scripts/external-journey.sh"
  exit 0
fi

step "Bob registers a recipient key and publishes two request classes"
as Bob register-recipient-key
as Bob publish-classes --version 1 --class "1=Introductions=500" --class "2=Business proposals=2000"
say "Bob's request page: http://127.0.0.1:$port/r/$bob"

CS_DEMO_BASE="http://127.0.0.1:$port" CS_DEMO_BOB="$bob" CS_DEMO_OUTBOX="$work/outbox" \
CS_DEMO_CLI="$cli" CS_DEMO_BOB_HOME="$work/Bob" CS_DEMO_SHOTS="$work/screenshots" \
  python3 "$root/scripts/m3b-browser.py"
echo "screenshots: $work/screenshots"

step "kill the service without shutdown, restart it, and read again"
kill -9 "$server"; wait "$server" 2>/dev/null || true; server=""
serve
as Bob inbox

step "redaction: no guest message, secret or database URL in the database or the log"
pg_dump --data-only "$CS_MAIL_DEMO_DATABASE_URL" > "$work/database.sql"
leaks=0
for marker in marker-ana-7f3c marker-ben-19ad marker-dan-5b21 marker-eve-c0de \
              marker-cleo-a11e marker-alias-e5e5; do
  for file in database.sql serve.log; do
    if grep -qF "$marker" "$work/$file"; then echo "LEAK: $marker in $file"; leaks=1; fi
  done
done
for name in provider scheduler operator processor bank-authority bank-ownership \
            identity-decision identity-product evidence relationships principals guest-content; do
  secret="$(tr -d ' \n' < "$work/$name.key")"
  if tr -d ' \n' < "$work/serve.log" | grep -qF "$secret"; then echo "LEAK: $name secret"; leaks=1; fi
done
# A PEM key is not a JSON seed; check its private base64 lines independently.
if sed '1d;$d' "$work/dkim.pem" | while IFS= read -r line; do
  grep -qF -- "$line" "$work/serve.log" "$work/database.sql" && exit 0
done; then echo "LEAK: DKIM private key"; leaks=1; fi
if grep -qF "$CS_MAIL_DEMO_DATABASE_URL" "$work/serve.log"; then echo "LEAK: database URL"; leaks=1; fi
if grep -qi "code is\|4242424242424242\|5555555555554444" "$work/serve.log" "$work/database.sql"; then
  echo "LEAK: verification code or card number in the log or database"; leaks=1
fi
if [[ $leaks -eq 0 ]]; then echo "none found"; else exit 1; fi
