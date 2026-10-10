#!/usr/bin/env bash
# Milestone 1 demonstration: clean start, idempotent bootstrap, authenticated operator
# commands, simulator scenarios, a killed and restarted service, and redacted logs.
#
#   CS_MAIL_DEMO_DATABASE_URL=postgresql://.../empty_db scripts/m1-demo.sh
#
# The database must be empty; the demonstration writes a deployment into it. Keys and
# configuration go to a fresh temporary directory, printed at the start. Enrollment is
# not part of this script: it needs the identity-model enrollment service and an OIDC
# provider (see apps/README.md).
set -euo pipefail

: "${CS_MAIL_DEMO_DATABASE_URL:?set CS_MAIL_DEMO_DATABASE_URL to an empty PostgreSQL database}"
root="$(cd "$(dirname "$0")/.." && pwd)"
port="${CS_MAIL_DEMO_PORT:-8480}"
work="$(mktemp -d)"
server=""
cleanup() { if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi; }
trap cleanup EXIT

step() { printf '\n==> %s\n' "$*"; }
show() { printf '$ %s\n' "$*"; "$@" 2>&1 || printf '(exit %s)\n' "$?"; }

step "build (simulation feature)"
cargo build -q --manifest-path "$root/Cargo.toml" --features simulation -p cs-maild -p cs-mail-admin
bin="$root/target/debug"
maild="$bin/cs-maild"
admin="$bin/cs-mail-admin"
echo "work directory: $work"

step "keys (secrets are written 0600 and never printed)"
for name in provider operator processor bank-authority bank-ownership identity-decision rogue; do
  "$admin" keygen --out "$work/$name.key" > "$work/$name.pub"
done
ls -l "$work"/*.key | awk '{print $1, $NF}'

public() { python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["verifying_key"]))' "$work/$1.pub"; }
domain='[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]'
account='[9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9]'
record() { # $1: provider key file stem
  cat <<EOF
{"provider": 30, "deployment_domain": $domain,
 "financial_scope": {"deployment_domain": $domain, "operator": 30, "program": 1,
                     "payment_account": $account, "protocol_version": 2},
 "unit": 1,
 "provider_key": {"reference": 3, "verifying_key": $(public "$1")},
 "operators": {"1": $(public operator)},
 "payment": {"processor_key": $(public processor), "bank_authority": $(public bank-authority)},
 "identity": {"issuer": "https://identity.localhost", "product": "cs-mail/dev",
              "decision_key": $(public identity-decision)}}
EOF
}
record provider > "$work/bootstrap.json"
record rogue > "$work/bootstrap-other-provider.json"
cat > "$work/maild.json" <<EOF
{"database_url": "$CS_MAIL_DEMO_DATABASE_URL", "listen": "127.0.0.1:$port",
 "workers": {"interval_ms": 500, "lease_ms": 30000, "batch": 50},
 "simulation": {"processor_secret_file": "$work/processor.key",
                "bank_authority_secret_file": "$work/bank-authority.key",
                "bank_ownership_secret_file": "$work/bank-ownership.key"}}
EOF
for who in operator rogue; do
  cat > "$work/admin-$who.json" <<EOF
{"server": "http://127.0.0.1:$port", "operator": 1,
 "operator_secret_file": "$work/$who.key", "deployment_domain": $domain}
EOF
done

step "migrate an empty database, twice"
show "$maild" migrate --config "$work/maild.json"
show "$maild" migrate --config "$work/maild.json"

step "bootstrap CSQD, repeat it, then try a different provider key"
show "$maild" bootstrap --config "$work/maild.json" --deployment "$work/bootstrap.json"
show "$maild" bootstrap --config "$work/maild.json" --deployment "$work/bootstrap.json"
show "$maild" bootstrap --config "$work/maild.json" --deployment "$work/bootstrap-other-provider.json"

serve() {
  "$maild" serve --config "$work/maild.json" >> "$work/serve.log" 2>&1 &
  server=$!
  for _ in $(seq 1 100); do
    curl -sf "http://127.0.0.1:$port/ready" > /dev/null && return 0
    sleep 0.1
  done
  echo "service did not become ready"; return 1
}

step "start the service"
serve
show curl -s "http://127.0.0.1:$port/health"; echo
show curl -s "http://127.0.0.1:$port/ready"; echo
show "$admin" status --config "$work/admin-operator.json"

echo '{"PublishServiceOffer":{"version":1,"period":{"year":2027,"month":1,"day":1,"leap_day":"February28"},"collect_at":1790000000000,"price":12000,"unit":1}}' > "$work/offer.json"
echo '{"PublishRequestPricing":{"version":1,"unit":1,"processing_charge":50,"collateral_min":100,"collateral_max":2500}}' > "$work/pricing.json"
echo '{"QueueDirectives":[{"unavailable":false,"outcome":"Pending","lose_response":false}]}' > "$work/scenario.json"
offer="$work/offer.json"; pricing="$work/pricing.json"; scenario="$work/scenario.json"
head "$offer" "$pricing" "$scenario"

step "operator commands: publish, replay, and a key the deployment does not trust"
show "$admin" configure --config "$work/admin-operator.json" --command "$offer"
show "$admin" configure --config "$work/admin-operator.json" --command "$offer"
show "$admin" configure --config "$work/admin-operator.json" --command "$pricing"
show "$admin" configure --config "$work/admin-rogue.json" --command "$pricing"

step "simulator scenario"
show "$admin" simulate --config "$work/admin-operator.json" --command "$scenario"

step "kill the service without shutdown, then restart it"
kill -9 "$server"; wait "$server" 2>/dev/null || true; server=""
echo "killed"
serve
show curl -s "http://127.0.0.1:$port/ready"; echo
show "$admin" configure --config "$work/admin-operator.json" --command "$offer"
show "$admin" simulate --config "$work/admin-operator.json" --command "$scenario"

step "graceful shutdown"
kill -INT "$server"; wait "$server" 2>/dev/null || true; server=""

step "service log"
cat "$work/serve.log"

step "redaction: no secret, key or database URL appears in the log"
leaks=0
for name in provider operator processor bank-authority bank-ownership identity-decision rogue; do
  secret="$(tr -d ' \n' < "$work/$name.key")"
  if tr -d ' \n' < "$work/serve.log" | grep -qF "$secret"; then echo "LEAK: $name secret"; leaks=1; fi
done
if grep -qF "$CS_MAIL_DEMO_DATABASE_URL" "$work/serve.log"; then echo "LEAK: database URL"; leaks=1; fi
if [[ $leaks -eq 0 ]]; then echo "none found"; else exit 1; fi
