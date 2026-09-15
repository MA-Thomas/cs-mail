# cs-mail

cs-mail is a communication protocol that puts economic friction at the boundary
of a new relationship rather than on every message. Accepted senders communicate
without protocol message fees. An unaccepted sender submits a relationship request with one conditional charge;
the recipient's relationship-level decision determines both future permission
and settlement.

## Start here

Read the **[C-SQD domain model](DOMAIN_MODEL.md)** first. It records the confirmed
product rules and deferred decisions for future implementation and documentation.

The documents have distinct roles:

1. **[Whitepaper](cs_mail_whitepaper.pdf)** — the service, request experience,
   annual member return, incentives, and privacy.
2. **[Deployment and migration profile](cs_mail_deployment_profile.pdf)** —
   normative C-SQD billing and member-program rules, followed by operational
   guidance for payments, identity, and migration.
3. **[Protocol specification](cs_mail_protocol_spec.pdf)** — normative request
   states, commands, ordering, obligations, and security invariants.
4. **[Rust reference architecture](cs_mail_rust_reference_architecture.pdf)** —
   the implemented domain owners, pure transitions, durable transactions, and
   recoverable external work.
5. **[Desktop MVP build plan](cs_mail_desktop_mvp_build_plan.pdf)** — product
   milestones that build on the existing Rust libraries.
6. **[Express-lane memo](cs_mail_express_lanes.pdf)** — a worked introduction to
   bounded permission for organizations.
7. **[Express-lane implementation plan](cs_mail_express_lanes_implementation.pdf)** —
   lane authority, lifetime, domain authentication, and product integration.

The matching `.tex` files are the editable sources. `intro_doc.tex` is retained
only as a superseded design-history document and must not be used as a current
protocol reference.

## The model in brief

C-SQD collects the full annual utility price well before a specified year-long
service period begins. Failed or late collection retries preserve that period
and price. Collection, service coverage, pool year, and distribution date are
separate policy inputs. New service coverage requires finalized funding and the
current time to fall within the purchased interval. Accepted and lane messages
have no request charge.

For the current product, one identifiable person has one account, potentially
multiple email addresses, and an associated verified bank account. C-SQD uses
one configured payment-processing arrangement.

For an unaccepted sender, terms fix a processing component `C` and collateral
`S` for one relationship request. Submission requires authenticated evidence that
the request-specific funding of `C + S` has met the provider's finality policy.
The current pricing defaults are `C = $0.50` and `S = $5.00`, with recipients
selecting `S` from a versioned bounded menu. Cancellation before submission returns the full charge. Acceptance
records a full refund and grants directed permission. Rejection retains `C` as
processing revenue and sends `S` into pending forfeiture for the separate C-SQD
member program. Expiry retains `C` and refunds `S`. Refund obligations remain
outstanding until the payment processor confirms them.

Follow-up messages, when permitted, belong to the same request and create no
additional charge or decision deadline. Principal-recipient history coordinates
eligibility across sender aliases. Accepted communication and valid express lanes
require no request charge. A recipient block separately prohibits ordinary contact.

The network allocates eligible pooled forfeitures once per UTC calendar year,
with a once-only corporate assessment and equal member shares. A person's share
`D_i` of the previous year's company-wide pool is paid automatically as **one
lump sum** to the associated verified bank account. For annual utility charge
`U`, `R_i = min(D_i, U)` is the rebate classification and
`X_i = max(0, D_i - U)` is excess cash. These are reporting values within one
payment, not separate refund and payout operations. Distribution is independent
of renewal, ordinarily during an already-paid service year. Pending next-year
funding does not delay or reclassify it. Closure preserves the route for
outstanding distributions and refunds. The earlier utility charge and later
distribution remain separate transactions; distribution does not revoke coverage.

## Core vocabulary

- **Private principal**: the provider-local subject used for control, recovery,
  and request continuity across public aliases.
- **Protocol identity**: the public persona to which directed permission applies.
- **Relationship**: recipient-controlled permission, independent of any request.
- **Request**: one solicitation with immutable terms, an initial message and a
  lifecycle carrying its submission preparation or submission facts.
- **Request history**: shared eligibility and pending-submission coordination for one
  principal addressing one recipient.
- **Message**: a committed admission with content and delivery references and the
  request, relationship version or lane version that authorized it.
- **Financial obligation**: an amount owed independently of external payment
  progress; capture, refund and payout evidence belong to financial records.
- **Member program**: company-owned forfeiture lots, eligibility, annual equal
  allocations, restricted funds and fixed member payables.
- **Billing account**: the person's verified bank association, service contracts,
  account status, and explicitly authorized account actions.
- **Service offer and contract**: published immutable annual price, service
  interval, and advance collection date; a purchase retains those terms across
  collection attempts.
- **Member payable**: one fixed allocation and payment lifecycle, with derived
  rebate/excess reporting values and published distribution terms.

See the current [domain implementation record](audit/domain-model-implementation-2026-09-15.md)
and [Rust architecture](cs_mail_rust_reference_architecture.tex) for the implemented
owners, lifecycles, and active formats.

### Request lifecycle names

`PreparingSubmission` means the request exists internally while funding and
initial-message requirements are completed. `SubmitRequestToRecipient` commits
initial delivery and starts the decision timer; the request then becomes
`AwaitingRecipientDecision`. `RequestSubmission` records that time and deadline.
Submission does not prove transport completion or reading. Cancellation is
available before submission; acceptance, rejection, or expiry settles a submitted
request. Individual-message permission checks continue to use "admission."

Related APIs use `submission_window`, `SubmissionTimeout`,
`CancelRequestSubmission`, `RequestSubmitted`, and `RequestSubmissionCancelled`.
Rust APIs and serialized records use the same submission terminology, including
state labels, command and event names, timeout tasks, and record fields.

## Status

The repository contains Rust libraries implementing the base
protocol and its centralized deployment foundations. It includes the pure
transition kernel, conserved ledger, authenticated wire format, durable
PostgreSQL shell, endpoint content encryption, retry-safe workers, native client
and ingress APIs, privacy/retention types, and bounded adapter state machines. Annual allocation, per-account billing, funded
coverage, automatic lump-sum distributions, and authorized scheduled payment work are
implemented; the running server and desktop application remain product work.
The protocol and architecture remain drafts dated September 2026.

This is not yet a production service. Network transports, TLS connection
pooling, durable administrative key custody, KMS/HSM integration, external
funding-rail connectors, the SMTP server/relay around the authentication
gateway, inter-provider authentication and
clearing, abuse operations, and jurisdiction-specific compliance policy are
deployment work. Express lanes are now a launch-feature implementation slice.
Inbound legacy authentication is integrated with Stalwart Labs' pinned
`mail-auth` 0.12.1 implementation using its minimal Ring/Hickory feature set;
production still needs resolver operations, SMTP reply policy, and monitoring.

## Rust reference implementation

The workspace is divided into focused libraries:

- `cs-mail-primitives` defines scoped identifiers, checked money, canonical time,
  and versions.
- `cs-mail-ledger` provides atomic value-conserving transfers and balance
  projections.
- `cs-mail-protocol` implements the pure deterministic transition kernel and its
  complete settlement manifests and shared admission checks.
- `cs-mail-finance` owns payment operations and evidence, funding-source controls,
  forfeiture lots, member eligibility, annual allocations, and payables.
- `cs-mail-billing` owns published service offers, fixed annual service contracts,
  collection evidence, coverage, and signed account commands.
- `cs-mail-application` atomically applies manifests to a shared in-memory store of protocol,
  ledger, journal, schedule, outbox, and idempotency state. Its pure billing
  coordinator prepares one bank payment for a payable using the account association;
  PostgreSQL uses that same coordinator.
- `cs-mail-storage-postgres` applies the same manifests inside row-locked
  PostgreSQL transactions and persists projections, ledger batches, transfers,
  events, an authenticated command inbox, recoverable signed outcomes, leased
  external work with fenced claims, independent owner records, retention manifests,
  and leased schedules.
- `cs-mail-wire` defines a strict deterministic CBOR command representation,
  including deployment and intended-provider signing scope.
- `cs-mail-security` provides Ed25519 command authentication, canonical receipt-
  time key rotation and revocation, a hash-chained transparency log, golden
  vectors, and threshold recovery ceremonies.
- `cs-mail-content` provides authenticated endpoint-only HPKE content encryption using X25519,
  HKDF-SHA-256, and ChaCha20-Poly1305, with authenticated binding to message,
  sender, recipient, content reference, and protocol version.
- `cs-mail-worker` recovers received commands and pending signed artifacts,
  materializes deadlines as ordinary commands, and runs bounded delivery, request
  payment, utility payment, annual allocation, and member payment batches with
  failure isolation.
- `cs-mail-client` keeps plaintext and content private keys at the endpoint and
  supplies separately scoped billing command signing.
- `cs-mail-service` supplies trusted-time signed-command ingress and bounded
  encrypted-content upload around the durable engine.
- `cs-mail-privacy` supplies directional pairwise handles, recipient-scoped
  principal assertions, minimized audit and telemetry types, and executable
  retention classes.
- `cs-mail-adapters` supplies testable funding-finality, explicit SMTP privacy-
  downgrade, the async SMTP/DMARC boundary, canonical versioned evidence types,
  and federation prepare/commit foundations without changing the centralized
  transition meaning.
- `cs-mail-smtp-gateway` supplies bounded SPF, DKIM, and RFC 9989 DMARC
  verification through Stalwart's `mail-auth`, with typed transient/permanent
  failures and canonical-receipt-time skew protection.
- `cs-mail-capabilities` supplies recipient-signed native/domain lane grants,
  enforceable volume and lifetime bounds, replay-safe admission consumption,
  and lane horizon behavior.

The implemented command set covers terms, request creation, capture evidence,
submission to the recipient, follow-ups, cancellation, acceptance, rejection, blocking, unblocking,
expiry and revocation. Tests cover these lifecycles, shared alias history, replay,
value conservation, provider reconciliation, financial allocation, authentication,
content binding and worker retries. Domain acceptance tests cover advance
collection with fixed-period retries, bank-linked identity and explicit authority,
dispatch-time funding restrictions, automatic lump-sum payment after closure,
lost-response recovery, and reporting independent of future renewal funding.

Wire determinism follows the deterministic-encoding requirements of RFC 8949.
Command signatures use Ed25519 as specified by RFC 8032. Native content uses the
RFC 9180 HPKE construction. These choices are versioned protocol inputs, not an
invitation to silently substitute another suite.

To verify the Rust workspace:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

PostgreSQL integration tests run when `CS_MAIL_TEST_DATABASE_URL` names an
isolated test database:

```sh
CS_MAIL_TEST_DATABASE_URL='postgresql://postgres@localhost:5432/postgres' \
  cargo test -p cs-mail-storage-postgres --test postgres -- --ignored
```

The fourteen schema migrations are embedded in the storage crate and applied under
a database advisory lock. They cover protocol/ledger durability, encrypted
content, retention, capabilities, financial programs, independent domain owners,
authenticated receipt ordering, unified external work, annual allocation, utility
billing, and owner-specific lifecycle rules. Current protocol storage format is 8,
financial storage format is 6, and command wire format is 6. This clean domain
update requires a freshly initialized schema. Unsupported stored formats are
rejected; the migrations do not convert populated older records.
There are no compatibility aliases or fallback readers for deprecated labels. The current connector deliberately uses `NoTls`; it is
appropriate for a local Unix socket or a separately secured development
connection. A TLS-configurable connection pool belongs to the service/security
deployment layer.

## Building the PDFs

With a TeX distribution containing `latexmk` and `pdflatex`:

```sh
latexmk -pdf cs_mail_whitepaper.tex
latexmk -pdf cs_mail_protocol_spec.tex
latexmk -pdf cs_mail_rust_reference_architecture.tex
latexmk -pdf cs_mail_deployment_profile.tex
latexmk -pdf cs_mail_express_lanes.tex
latexmk -pdf cs_mail_express_lanes_implementation.tex
latexmk -pdf cs_mail_desktop_mvp_build_plan.tex
```

Generated auxiliary files can be removed with `latexmk -c`. Root-level PDFs are
the canonical review artifacts; `output/` is ignored to avoid duplicate generated
copies.
