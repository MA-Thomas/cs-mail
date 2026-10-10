# Applications

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](../docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

The binaries in this directory are the product layer described in the
[product build plan](../cs_mail_build_plan.tex). Each is an **adapter**: it
validates input, invokes application use cases, and renders outcomes. None of
them makes domain decisions.

| Binary | Role | Arrives |
|---|---|---|
| `cs-maild` | Service process: HTTP API, workers, sender request page hosting, outbound guest and notice email | Milestone 1 |
| `cs-mail-admin` | Operator administration client, through the service's authenticated API | Milestone 1 |
| `cs-mail-cli` | Developer member client, over the client core | Milestone 2 |
| `cs-mail-mx` | Receive-only SMTP receiver (MX) for the deployment domain | Milestone 3c |
| `cs-mail-desktop` | macOS member application (Tauri) | Milestone 4 |
| `cs-mail-web` | Sender request page (static assets) | Milestone 3b |

## Boundary rules

- **Dependencies point inward.** Applications may depend on `crates/`. No crate in
  `crates/` may depend on an application. `scripts/quality-gate.sh` enforces this.
- **No domain decisions in adapters.** Permission, settlement, eligibility, and
  billing rules belong to their owning domain crates, and orchestration to
  `cs-mail-application`. An adapter never translates a signed command into a
  different meaning.
- **Time and authority.** Adapters obtain trusted time only as the
  [application and persistence boundaries](../docs/application-persistence-boundaries.md)
  prescribe: after acquiring the required protection when freshness matters.
- **No direct canonical writes.** The admin CLI uses the authenticated API. No
  application writes canonical tables outside the storage adapters' transactions.
- **Secrets stay behind ports.** Key material is reached only through key-custody
  ports and never enters logs, configuration, or diagnostics.

Shared metadata (version, edition, minimum Rust version, `publish = false`), lints,
and dependency versions come from the workspace `Cargo.toml`.

## Running the service (Milestone 1)

`scripts/m1-demo.sh` runs the whole sequence below against an empty database and
prints the transcript:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/cs_mail_demo scripts/m1-demo.sh
```

1. `cs-mail-admin keygen --out <file>` writes an Ed25519 secret (a JSON array of 32
   bytes, mode 0600) and prints only its public key. The provider, operator,
   processor and bank keys are made this way.
2. `cs-maild migrate --config maild.json` applies the schema.
   `cs-maild bootstrap --config maild.json --deployment bootstrap.json` creates the
   deployment from a record of public keys and trust roots; repeating it with the
   same record changes nothing, and a different record is refused.
3. `cs-maild serve --config maild.json` listens on loopback only; terminate TLS at the
   deployment proxy. `/health` is process health, `/ready` is database and bootstrap
   readiness. If its database connection is lost, the service exits with a failure
   status so its supervisor restarts it.
4. `cs-mail-admin configure --config admin.json --command <file>` signs and sends an
   operator command (`PublishServiceOffer`, `PublishRequestPricing`). A repeat of the
   same command returns the recorded outcome. Development builds
   (`--features simulation`) add `cs-mail-admin simulate` for simulator scenarios.

Enrollment is served at `POST /v1/enrollments` (begin), `POST
/v1/enrollments/{operation}/renew`, `POST /v1/enrollments/{operation}/complete` and
`GET /v1/enrollments/{operation}` (reconcile). It needs the `identity` section of the
configuration, pointing at the identity-model enrollment service's `/v1/enrollment`
endpoint, and a bank-evidence source: the simulated bank in development builds.
Completing an enrollment sends the applicant's name, date of birth, address and phone
(`applicant`) and the bank-integration request again (`bank`); cs-maild forwards them
to the identity service and never stores or logs them. The outcome is `enrolled`,
`under-review` (the evidence overlaps another enrollment's), `review-approved`,
`review-required` (by the bank) or `denied`.

`cs-mail-admin reviews --config admin.json` lists enrollments held for review, with the
accounts they overlap and the matching fields.
`cs-mail-admin review --config admin.json --case <id> --resolution approve|deny`
decides one; the identity service records the operator and the signed command. After
approval the applicant renews the enrollment and completes it again with a fresh
login.

Logs carry typed error codes only (SQLSTATE for database errors); no secrets, keys,
request bodies or database URLs.
