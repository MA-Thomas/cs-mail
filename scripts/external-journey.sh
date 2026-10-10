# Sourced by m3b-demo.sh after clean deployment and enrollment.
step "build the controlled local receiver"
cargo build -q --manifest-path "$root/Cargo.toml" -p cs-mail-test-support --example dev-external-reply
as Bob register-recipient-key
printf 'Hello Carol, from the external conversation demo.\n' > "$work/hello.txt"
printf 'Thanks for your replies.\n' > "$work/followup.txt"
step "member composes and the development relay accepts"
as Bob email --conversation 7001 --to carol@example.test --message-file "$work/hello.txt" --disclose-to-external-mail --operation first
for _ in $(seq 1 100); do
  "$cli" --home "$work/Bob" external-read --conversation 7001 > "$work/read.txt"
  if grep -q '"Accepted"' "$work/read.txt"; then break; fi
  sleep 0.1
done
grep -q '"Accepted"' "$work/read.txt"
curl -fsS -H 'Content-Type: application/json' -d "{\"member\":$bob,\"mailbox\":\"carol@example.test\"}" \
  "http://127.0.0.1:$port/v1/external/prepare" > "$work/preparation.json"
reply() { "$bin/examples/dev-external-reply" "$work/maild.json" "$work/preparation.json" "$bob" carol@example.test "$1" "$2"; }
step "two uncharged replies arrive while the conversation remains pending"
reply 7001 7101
reply 7001 7102
as Bob external-read --conversation 7001
step "member replies, then closes only the unpaid invitation"
as Bob email-reply --conversation 7001 --message-file "$work/followup.txt" --operation second
as Bob external-reject --conversation 7001
if reply 7001 7103; then echo "rejected invitation accepted a reply"; exit 1; fi
step "new explicit invitation and existing relationship acceptance"
as Bob email --conversation 7002 --to carol@example.test --message-file "$work/hello.txt" --disclose-to-external-mail --operation new-invitation
as Bob external-accept --conversation 7002
reply 7002 7104
step "slow the relay polling, queue, withdraw, then restart normally"
kill "$server"; wait "$server" 2>/dev/null || true; server=""
python3 - "$work/maild.json" "$work/maild-no-workers.json" <<'PYCONFIG'
import json,sys
c=json.load(open(sys.argv[1])); c['workers']['interval_ms']=600000
json.dump(c,open(sys.argv[2],'w'))
PYCONFIG
"$maild" serve --config "$work/maild-no-workers.json" >> "$work/serve.log" 2>&1 &
server=$!
for _ in $(seq 1 100); do
  curl -sf "http://127.0.0.1:$port/ready" > /dev/null && break
  sleep 0.1
done
sleep 1 # Allow the initial empty worker tick before queuing.
as Bob email --conversation 7003 --to carol@example.test --message-file "$work/hello.txt" --disclose-to-external-mail --operation cancel-me
as Bob withdraw-disclosure --conversation 7003
as Bob grant-disclosure --conversation 7003 --operation restore-consent
kill "$server"; wait "$server" 2>/dev/null || true; server=""
serve
"$cli" --home "$work/Bob" external-read --conversation 7003 > "$work/cancelled.txt"
grep -q '"Cancelled"' "$work/cancelled.txt"
python3 - "$work/outbox" <<'PYOUTBOX'
import json,pathlib,sys
handoffs=[json.loads(p.read_text()) for p in pathlib.Path(sys.argv[1]).glob('*.external.json')]
assert any(x['conversation']==7001 for x in handoffs)
assert all(x['conversation']!=7003 for x in handoffs)
print('cancelled message absent from development relay handoffs')
PYOUTBOX
step "a separate mailbox receives an existing express-lane grant"
as Bob email --conversation 7004 --to dave@example.test --message-file "$work/hello.txt" --disclose-to-external-mail --operation lane-invitation
as Bob external-accept --conversation 7004 --lane-lifetime-ms 60000
"$cli" --home "$work/Bob" external-read --conversation 7004 > "$work/lane.txt"
grep -q '"ExpressLane"' "$work/lane.txt"
step "CLI journey complete; artifacts retained at $work"
