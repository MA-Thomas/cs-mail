# Linux callback pilot

**Timing agreed 8 October 2026:** build and validate the macOS member application
locally before provisioning or paying for a rented Linux host (for example AWS
Lightsail). This package is preparation for the later hosted pilot. Public HTTPS,
MX and real SMTP2GO callback checks wait until that stage.

This package runs the existing Rust service with sending disabled, PostgreSQL on a
local Unix socket, and an HTTPS proxy exposing only the authenticated SMTP2GO
callback route. It does not enable enrollment, payments, the guest page, or incoming
SMTP. The member/operator listener remains on loopback for administration over SSH.
No server, DNS record, certificate or provider webhook is created by these files.

The current schema is **30**. Migrations are not declared forward-only: start a
fresh database for this revision. Preserve older development databases and their
keys; do not point this release at them or restart an old send helper. A fresh
deployment does not contain the historical outbound records needed to correlate
callbacks for earlier live tests. A new live callback experiment will need an
explicitly authorized, matching outbound attempt after host setup.

## Build and transfer

Use a supported Linux distribution with systemd, nginx, Python 3 and PostgreSQL
(local recovery was exercised with PostgreSQL 17). Install the Rust toolchain and
native build dependencies, including a C compiler, pkg-config and OpenSSL headers,
on the build machine. Build on the host's architecture. Keep `identity-model` beside
`cs-mail` and verify its source pin. No hosted CI is assumed.

```sh
# In the cs-mail checkout on Linux; output must not exist yet.
bash deploy/linux/build-release.sh /tmp/cs-mail-release-20261007
cd /tmp/cs-mail-release-20261007
sha256sum --check SHA256SUMS
```

The release contains `cs-maild`, `cs-mail-admin`, templates, this guide and the
database tool. Both binaries are built with `--locked --no-default-features`.
It records the toolchain, working-tree status, source hashes (including uncommitted
Rust/schema sources in both repositories) and packaged-file checksums. The hashes
are integrity/provenance records, not signatures from a trusted distributor. Keep
the matching source checkout. An interrupted build leaves an incomplete directory;
use a new output path and accept only a successfully completed build.

Transfer through an authenticated channel, verify the checksums again, then copy
the release into a new root-owned `/opt/cs-mail/releases/20261007` directory. Create
`/opt/cs-mail/current` pointing to it. Do not overwrite a running release.

## Private host and database

Create a system account `cs_mail` with no login shell. Create `/etc/cs-mail` and
`/etc/cs-mail/secrets`, owned by `root:cs_mail`, mode `0750`. Config and secret files
should be `root:cs_mail`, mode `0640`; release files must not be writable by the
service account. The database account needs ownership of its own database because
the current service applies migrations on connection. It must not be a PostgreSQL
superuser, create roles, or own other application databases.

On a **dedicated** PostgreSQL cluster, set `listen_addresses = ''` and keep the Unix
socket at `/var/run/postgresql`. Place this rule before broader matching rules in
`pg_hba.conf`, preserving the system administrator's local access:

```text
local   cs_mail   cs_mail   peer
```

As the PostgreSQL administrator, create only the new role/database:

```sql
CREATE ROLE cs_mail LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE DATABASE cs_mail OWNER cs_mail TEMPLATE template0;
REVOKE CONNECT ON DATABASE cs_mail FROM PUBLIC;
```

Restart/reload PostgreSQL as appropriate for those settings. Confirm that
`sudo -u cs_mail psql -d cs_mail -c 'SELECT current_user'` uses peer authentication.
No database password is needed in this profile. Allow public TCP 443 and restricted
SSH access at both the host and cloud firewalls. Do not open 5432, 8580, 8582 or SMTP
25. Port 80 is needed only if the chosen certificate issuer uses HTTP validation;
configure its challenge location separately. DNS-based validation is another option.

## Configuration and trust

Copy `maild.json.example` to `/etc/cs-mail/maild.json`; replace `mail.example.net`
with the intended mail domain. Keep `delivery.kind` set to `disabled`.

Each referenced secret is a JSON array of 32 random bytes. For a genuinely new
deployment, `cs-mail-admin keygen --out /protected/new-key.json` creates an exclusive
mode-0600 file and prints its public key. It can generate the provider signing seed;
the same random-seed format is accepted for the relationship, principal, external
content and webhook secrets. Move files into the protected directory and set the
ownership/modes above. Never regenerate those secrets during restart or recovery:
relationship/principal derivation and encrypted content depend on the original keys.
Keep the webhook secret separate from the provider and content keys.

Prepare a **reviewed public bootstrap record** as `/etc/cs-mail/bootstrap.json`,
using the `BootstrapRecord` contract in `crates/cs-mail-operations/src/lib.rs`.
It specifies the provider, deployment domain, financial scope and unit, provider
and scheduler public keys, operator public keys, payment/bank trust roots and
identity issuer/product/key. The provider public key must match `provider.json`.
Those values define who is trusted: do not invent production bank/identity keys or
copy the synthetic smoke-test record. This package deliberately does not choose
those authorities. Keep operator signing keys off the server; public trust data can
be stored with the deployment. Disabled integrations still require a valid record.

```sh
sudo -u cs_mail /opt/cs-mail/current/bin/cs-maild migrate --config /etc/cs-mail/maild.json
sudo -u cs_mail /opt/cs-mail/current/bin/cs-maild bootstrap --config /etc/cs-mail/maild.json --deployment /etc/cs-mail/bootstrap.json
sudo install -m 0644 /opt/cs-mail/current/deploy/cs-mail.service /etc/systemd/system/cs-mail.service
sudo systemd-analyze verify /etc/systemd/system/cs-mail.service
sudo systemctl daemon-reload
sudo systemctl enable --now cs-mail
curl --fail http://127.0.0.1:8580/ready
```

`/ready` confirms a database connection and a bootstrapped deployment; it does not
establish provider availability or full product readiness. The systemd unit sends
SIGINT, which the current application handles by draining both HTTP listeners.
Unexpected process failure restarts the service; repeated failures hit a start
limit. After correcting the fault, use `systemctl reset-failed cs-mail` and start
it again. Inspect `journalctl -u cs-mail` without enabling payload/secret logging.

## HTTPS and host acceptance

Obtain a certificate for the chosen callback hostname and arrange renewal. Copy
`callbacks.nginx.conf.example` into nginx's HTTP configuration, replace hostname
and certificate paths, then run `nginx -t` before reloading. Remove any distribution
default site that unintentionally exposes another service. The template exposes
one exact POST path, limits bodies to 64 KiB and disables request buffering and
payload-bearing proxy logs. Provider authorization headers pass through to Rust;
nginx is not the authority for accepting an event. A proxy timeout can occur after
commit; the provider's retry must reach the existing idempotent event handler.

Before configuring SMTP2GO, check from outside the server:

- HTTPS certificate/hostname verification succeeds; unauthenticated POST to
  `/v1/external/smtp2go/events` returns 401.
- `/ready` and member/operator paths return 404 through the public proxy. Direct
  connections to 8580, 8582 and 5432 fail from outside.
- An oversized POST returns 413; non-POST methods cannot invoke the callback.
- `systemctl restart cs-mail` drains and reopens both local listeners. Rebooting
  the host returns the service to readiness. On this empty pilot, interrupting its
  database connection causes failure/restart and eventual readiness.
- The certificate renewal check succeeds; service failure, low disk space and
  backup failure have an operator-visible monitoring path before an unattended pilot.

Then configure the actual provider event destination and dedicated bearer value
as described in `docs/smtp2go-outbound.md`. Test a newly authorized matching send;
verify provider acceptance and committed callback evidence separately. Do not claim
delivery from readiness or manufacture an outbound record to accept an old event.

## Backup and recovery

Run as `cs_mail` with the matching PostgreSQL client tools. Use `PGHOST`, `PGPORT`,
`PGUSER`, `PGDATABASE` and a protected `PGPASSFILE` if a different deployment needs
passwords. Do not put a connection URI/password on the command line. For this peer
profile the standard local socket and user defaults are sufficient:

```sh
sudo -u cs_mail env PGDATABASE=cs_mail python3 /opt/cs-mail/current/deploy/database.py backup /protected-backups/cs-mail-20261007.dump
```

Create the backup directory beforehand with access restricted to its operator and
the backup process. The tool produces a consistent custom-format database dump,
checks that its archive catalog is readable, and publishes it without overwriting
an existing backup. Dump files are mode 0600. A catalog check does not replace a
restore drill. Backups contain sensitive records; keep encrypted off-host copies,
define retention and an acceptable loss window, and verify each scheduled backup.
No retention/deletion policy or unattended backup schedule is silently installed.

Separately retain an encrypted, access-controlled recovery bundle containing
configuration, bootstrap record, the exact secret files, release/checksums and the
operator's key custody instructions. A database archive alone is insufficient to
recover encrypted content. Certificates can be reissued; domain secrets cannot
simply be replaced. Never include this private bundle in Git or the public release.

To recover, fence the old host so it cannot resume work, stop the new service,
restore the same secret files, and create a **new empty database** owned by cs_mail
(for example `cs_mail_recovery`). Add its peer-authentication rule and revoke public
connect access as above. With no other writers connected:

```sh
sudo -u cs_mail env PGDATABASE=cs_mail_recovery python3 /opt/cs-mail/current/deploy/database.py restore /protected-backups/cs-mail-20261007.dump
```

Restore refuses a populated target and runs in one transaction with exit-on-error.
Only use trusted archives: PostgreSQL dumps can contain executable SQL. It neither
drops a database nor reconciles external effects. Repoint the protected config to
the recovery database, keep sending disabled and start the matching release. Check
readiness, schema 30, deployment identity and audited commands, then validate
callback isolation and key availability. A failed restore must leave the service
stopped until investigated; preserve the source archive and old database.

**Restoring an old snapshot can forget a handoff made after that snapshot.** Its
restored queue may therefore appear sendable even though a provider already acted.
Keep sending and any future financial workers disabled until that interval is
reconciled using independent provider evidence. This package does not implement
point-in-time recovery or automatic reconciliation. Never restart both old and new
hosts against divergent copies. Binary rollback also requires a compatible schema;
changing the `current` symlink does not roll back a database migration.

## Local evidence and its limits

The agreed recovery conjecture is that a callback-only process can restart and a
fresh database can recover its recorded deployment/operator history without adding
sending authority. Missing/changed facts, an unexpected exposed route, a process
that stays alive after losing its database, or an overwritten backup would falsify
the relevant claim and make recovery unsafe.

```sh
cargo build --locked --no-default-features -p cs-maild -p cs-mail-admin
# Use an isolated test cluster and a role allowed to CREATE DATABASE.
PGHOST=127.0.0.1 PGPORT=55432 PGUSER=thomm15 PGDATABASE=cs_mail_external_test \
  python3 scripts/deployment-smoke.py --allow-create-test-databases
```

The experiment creates only its own randomly named databases and deletes them on
exit. It uses random synthetic trust keys, bootstraps twice, records a signed
operator pricing command, kills/restarts the real binary, terminates only its own
tagged database sessions, backs up/restores to the second database, compares durable
deployment/audit/schema facts, checks rollback of an incomplete archive, rejects
overwrite/re-restore, starts the restored
service and checks graceful shutdown and log redaction. No providers are contacted.
Run the full `scripts/quality-gate.sh` separately; it does not invoke this destructive
to-its-own-fixtures deployment drill or require a CREATEDB-capable role.

Local results are recorded in `docs/smtp2go-checkpoint.txt`. This experiment does
not validate systemd/nginx on macOS, public TLS/firewalls/certificate renewal, real
SMTP2GO callbacks, large-database recovery, every financial/content lifecycle, or
power-loss durability. Those remain explicit host/product acceptance work.

References: [nginx proxy controls](https://nginx.org/en/docs/http/ngx_http_proxy_module.html),
[nginx request limits](https://nginx.org/en/docs/http/ngx_http_core_module.html),
[PostgreSQL restore options](https://www.postgresql.org/docs/current/app-pgrestore.html).
