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
| `cs-mail-cli` | Developer member client | Milestone 2 |
| `cs-mail-mx` | Receive-only SMTP receiver (MX) for the deployment domain | Milestone 3c |
| `cs-mail-apple` | SwiftUI member application; macOS first, shared Apple code with iOS targets | Milestone 4 |
| `cs-mail-web` | Sender request page (static assets) | Milestone 3b |

The Apple application uses the Rust member client facade and UniFFI Swift bindings. SwiftUI owns presentation; business rules and portable
client workflows remain in Rust. Apple adapters provide Keychain and platform
services. The web request page retains its own components. See the
[Apple client architecture](../docs/apple-client-architecture.md) for ownership,
local data, packaging, the first slice and validation scope. The
[Apple runbook](cs-mail-apple/README.md) records builds, local browser enrollment
and validation limitations.

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
   bytes, mode 0600) and prints only its public key. The provider, scheduler,
   operator, processor and bank keys are made this way.
2. `cs-maild migrate --config maild.json` applies the schema.
   `cs-maild bootstrap --config maild.json --deployment bootstrap.json` creates the
   deployment from a record of public keys and trust roots; repeating it with the
   same record changes nothing, and a different record is refused.
3. `cs-maild serve --config maild.json` listens on loopback only; terminate TLS at the
   deployment proxy. `/health` is process health, `/ready` is database and bootstrap
   readiness. If its database connection is lost, the service exits with a failure
   status so its supervisor restarts it.
4. `cs-mail-admin configure --config admin.json --command <file>` signs and sends an
   operator command (`PublishServiceOffer`, `PublishRequestPricing`,
   `PublishProtocolPolicy`). A repeat of the same command returns the recorded outcome. Development builds
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

## The core request loop (Milestone 2)

`scripts/m2-demo.sh` runs the loop from two empty databases, one for cs-mail and one for
the identity service, and prints the transcript:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/cs_mail_demo \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/identity_demo \
  scripts/m2-demo.sh
```

The member API needs the `members` section of the `cs-maild` configuration: the
provider's signing seed (its public key must be the bootstrapped provider key) and the
two secrets relationship references and request histories are derived with. Replacing a
derivation secret disconnects every existing relationship. The operator must publish a
protocol policy (`PublishProtocolPolicy`) before requests can be made.

| Route | What it takes |
|---|---|
| `GET /v1/deployment` | Nothing; returns the signing scope, product and current protocol policy |
| `POST /v1/accounts/commands` | A signed account command, for example registering a recipient key |
| `POST /v1/billing/commands` | A signed billing command, for example purchasing annual service |
| `POST /v1/request-classes` | A recipient's signed request classes |
| `GET /v1/addresses/{persona}/offer` | Nothing; returns `C` and the classes a sender may choose |
| `POST /v1/endpoint-keys`, `GET /v1/addresses/{persona}/endpoint-key` | An address's signed content key |
| `POST /v1/relationships/{sender}/{recipient}/commands` | A participant's signed relationship command |
| `POST /v1/relationships/{sender}/{recipient}/content` | The sender's ciphertext and content-key certificate |
| `POST /v1/lanes` | A recipient's signed express-lane grant |
| `POST /v1/members/reads` | A signed read of the member's account, a relationship, or its inbox |

`cs-mail-cli --home <dir>` keeps one member's keys and journal in `<dir>/member.json`
(mode 0600). Its commands are `init`, `enroll-begin`, `enroll-complete`,
`enroll-status`, `register-recipient-key`, `publish-content-key`, `account`,
`purchase`, `publish-classes`, `offer`, `request`, `followup`, `cancel`,
`permit-followups`, `decide`, `inbox`, `status`, `retry` and `whoami`. Every signed
request is journaled before it is sent; after a lost response, the same command or
`retry` sends exactly the same bytes again, and the service answers with the recorded
outcome. Message content is encrypted on the member's side to the recipient's
published content key and decrypted by the recipient's client.

## The sender request page (Milestone 3b)

`scripts/m3b-demo.sh` runs the page from two empty databases and drives it in headless
Chromium through `scripts/m3b-browser.py` (Python Playwright and `pg_dump` required):

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/cs_mail_demo \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/identity_demo \
  scripts/m3b-demo.sh
```

The page needs the `guests` section of the `cs-maild` configuration, beside `members`
and a payment provider (the simulation in development): the secret of the provider's
guest content key, the mail domain and DKIM selector with the domain's RSA-2048 DKIM
key (unencrypted PKCS#8 PEM), and, in development, the outbox directory where DKIM-signed notices are written
instead of sent. Publish the DKIM public key at `<selector>._domainkey.<domain>`.

`cs-mail-web` is static: `cs-maild` serves it at `/r/{persona}`, with its assets under
`/r/assets/`. It holds no rules; the page shows what the guest API answers.
Identifiers are 128-bit integers and travel as JSON numbers.

| Route | What it takes |
|---|---|
| `POST /v1/guest/verifications` | `{mailbox}`; emails a six-digit code, valid 15 minutes |
| `POST /v1/guest/verifications/{id}/confirm` | `{code}`; returns a two-hour session |
| `GET /v1/guest/recipients/{persona}/offer` | Nothing; returns `C`, the classes and the request windows |
| `POST /v1/guest/requests` | `{session, recipient, class, instrument, message, operation}`; authorizes the charge and stores the message encrypted |
| `POST /v1/guest/requests/submit` | `{session, recipient, operation}`, once the charge is funded |
| `POST /v1/guest/requests/cancel` | `{session, recipient, operation}`, before submission; the full charge is refunded |
| `POST /v1/guest/status` | `{session, recipient}`; the guest's own relationship and its money |

`operation` is the page's identifier for one request; retrying with the same one
replays every step. Refusals carry a stable code, for example `pending-request`,
`request-in-progress`, `backoff` or `payment-method-declined`. Message text, codes and
card numbers are never logged.

## Member-initiated external correspondence (first Milestone 3c slice)

Run `scripts/m3c-demo.sh` with `CS_MAIL_DEMO_DATABASE_URL` and
`CS_MAIL_DEMO_IDENTITY_DATABASE_URL` pointing at two empty local PostgreSQL databases.
It reuses the existing deployment/enrollment setup, then exercises the signed CLI
and HTTP flow: send, multiple pending replies, read, reply, reject, new invitation,
accept, and withdrawal before dispatch. It needs no browser. The local
`dev-external-reply` example supplies synthetic receiver evidence directly to the
application/storage boundary; it is not a network receiver or proof of SMTP checks.

| CLI command | Meaning |
|---|---|
| `email --conversation ID --to ADDRESS --message-file FILE --disclose-to-external-mail --operation NAME` | Explicitly open an invitation and authorize external plaintext disclosure |
| `email-reply --conversation ID --message-file FILE --operation NAME` | Queue another authored-text message under existing conversation consent |
| `external-read --conversation ID [--after SEQUENCE]` | Read and decrypt up to 100 records; use `next_after` for the next page |
| `external-reject --conversation ID` | Close a pending unpaid invitation, without blocking or settlement |
| `external-accept --conversation ID [--lane-lifetime-ms MS]` | Use existing standing acceptance or mailbox-bound express-lane permission |
| `grant-disclosure --conversation ID --operation NAME` | Explicitly authorize outbound email on an inbound/paid conversation, or restore withdrawn consent; cancelled mail stays cancelled |
| `withdraw-disclosure --conversation ID` | Cancel waiting messages atomically; dispatch already begun may be too late |

`POST /v1/external/prepare` accepts `{member, mailbox}` and returns public routing
and encryption keys. `POST /v1/external/commands` accepts a deployment-scoped member
signature over the full command. Mutating commands journal encrypted payloads and
stable operation identities before sending. Exact retries return recorded outcomes;
changed retries are refused. No public endpoint accepts claimed email authentication.

The member encrypts a local copy and relay copy. The relay key uses the configured
`external.content_secret_file` with its own key reference. Both database content and
CLI journal hold ciphertext. This slice accepts newly authored text up to 16 KiB;
native source/quotation exports are refused. Invitation replies require the exact
mailbox and conversation, current coverage, and trusted receiver authentication;
no interactive mailbox-control challenge is created. Blocks take precedence.

With `simulation`, the external worker writes mode-0600 `*.external.json` files to
`external.delivery.directory`. These contain deliberately disclosed plaintext and represent
local handoff, not RFC email or SMTP delivery. The worker commits the dispatch
boundary before transport I/O. Unknown handoffs remain `DispatchBegun`, require
reconciliation through `finish_dispatch`, and are never automatically retried.
`Accepted` means the relay accepted the handoff, not that the recipient read it.
The separate `cs-mail-mx` receiver accepts conversation replies and new mail to
member addresses, using plain text or paired plain-text/HTML alternatives. SMTP2GO transport and authenticated
feedback are locally implemented (below); live provider validation, operational
reconciliation UI and native-source export policy remain subsequent work.

## SMTP receiver

`cs-mail-mx --config FILE` offers STARTTLS, verifies incoming email automatically,
and acknowledges a message only after atomic admission. It opens an already
migrated database without DDL and takes no member signing or relay-decryption keys.
See the [receiver runbook](../docs/smtp-receiver.md) for configuration, supported
mail formats, stable retries, deployment requirements and validation limits.

## SMTP2GO external delivery (Milestone 3c)

External conversation endpoints now use an independent `external` configuration,
with exactly one delivery selection: `disabled`, `smtp2go`, or explicit development
file delivery in a simulation build. They no longer require `guests` or a payment
simulator. Existing development configurations must add `external` explicitly.
The CLI demo has been updated. API acceptance and downstream provider events are
visible separately through `external-read`.

`POST /v1/external/smtp2go/events` accepts authenticated JSON callbacks using a
separate bearer secret. The proxy terminates HTTPS. See the [outbound guide](../docs/smtp2go-outbound.md)
for complete configuration and the account-connection sequence. Guest verification,
request/outcome and refund notices now select their own explicit SMTP2GO or development
adapter under `guests.notices`. See [the completed local journey](../docs/external-email-journey.md)
for configuration, fresh-database requirements and validation limits.

The external-mail `feedback` choice is independent of `delivery`. Its SMTP2GO
receiver binds a dedicated loopback listener and exposes only the callback POST
route; the member/operator API no longer carries that route. Receive-only operation
needs no sending API key or DKIM key. See [the existing operating guide](../docs/smtp2go-outbound.md)
for the configuration cutover and public HTTPS requirements.
