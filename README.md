# cs-mail

cs-mail is a communication protocol that puts economic friction at the boundary
of a new relationship rather than on every message. Accepted senders communicate
without protocol message fees. An unaccepted sender submits a relationship request with one conditional charge;
the recipient's relationship-level decision determines both future permission
and settlement.

## Start here

The documents have distinct roles:

1. **[Whitepaper](cs_mail_whitepaper.pdf)** - why the protocol exists, how the
   mechanism works, and what incentives and privacy properties it is intended to
   create.
2. **[Protocol specification](cs_mail_protocol_spec.pdf)** - the normative state,
   commands, ordering rules, settlement effects, errors, and invariants. Where
   explanatory documents differ from it, the specification controls.
3. **[Rust reference architecture](cs_mail_rust_reference_architecture.pdf)** -
   how a deterministic Rust kernel, transactional persistence, ledger, outbox,
   and privacy boundaries can implement the specification.
4. **[Deployment and migration profile](cs_mail_deployment_profile.pdf)** -
   normative requirements for the C-SQD financial program, plus deployment
   guidance for identity, payments, SMTP migration, and provider clearing.

The matching `.tex` files are the editable sources. `intro_doc.tex` is retained
only as a superseded design-history document and must not be used as a current
protocol reference.

## The model in brief

For an unaccepted sender, terms fix a processing component `C` and collateral
`S` for one relationship request. Admission requires a confirmed request-specific
capture of `C + S`. Pre-admission cancellation returns the full charge. Acceptance
records a full refund and grants directed permission. Rejection retains `C` as
processing revenue and sends `S` into pending forfeiture for the separate C-SQD
member program. Expiry retains `C` and refunds `S`. Refund obligations remain
outstanding until the payment provider confirms them.

Follow-up messages, when permitted, belong to the same request and create no
additional charge or decision deadline. Principal-recipient history coordinates
eligibility across sender aliases. Accepted communication and valid express lanes
require no request charge. A recipient block separately prohibits ordinary contact.

## Core vocabulary

- **Private principal**: the provider-local subject used for control, recovery,
  and request continuity across public aliases.
- **Protocol identity**: the public persona to which directed permission applies.
- **Relationship**: recipient-controlled permission, independent of any request.
- **Request**: one solicitation with immutable terms, an initial message and a
  lifecycle carrying its preparation or admission facts.
- **Request history**: shared eligibility and preparation coordination for one
  principal addressing one recipient.
- **Message**: a committed admission with content and delivery references and the
  request, relationship version or lane version that authorized it.
- **Financial obligation**: an amount owed independently of external payment
  progress; capture, refund and payout evidence belong to financial records.
- **Member program**: company-owned forfeiture lots, eligibility, quarterly equal
  allocations, restricted funds and fixed member payables.

See [Stage 1: domain model and ownership](audit/stage-1-domain-ownership.md) for
the implemented owners and Rust lifecycles, and
[Stage 2: authenticated admission and receipt](audit/stage-2-authenticated-admission.md)
for durable receipt ordering and shared admission checks.
[Stage 3: financial transitions and external work](audit/stage-3-financial-transitions.md)
and [Stage 4: persistence and lifecycle](audit/stage-4-persistence-lifecycle.md)
describe financial ownership, fenced work claims, independent stored owners, retention
and the current breaking formats.

## Status

The repository contains an executable Rust reference implementation of the base
protocol and its centralized deployment foundations. It includes the pure
transition kernel, conserved ledger, authenticated wire format, durable
PostgreSQL shell, endpoint content encryption, retry-safe workers, native client
and ingress APIs, privacy/retention types, and bounded adapter state machines.
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
- `cs-mail-finance` owns payment execution, forfeiture lots, member eligibility,
  quarter allocations and payables.
- `cs-mail-application` atomically applies manifests to a shared in-memory store of protocol,
  ledger, journal, schedule, outbox, and idempotency state.
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
  payment and member payment batches with failure isolation.
- `cs-mail-client` keeps plaintext and content private keys at the endpoint.
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
admission, follow-ups, cancellation, acceptance, rejection, blocking, unblocking,
expiry and revocation. Tests cover these lifecycles, shared alias history, replay,
value conservation, provider reconciliation, financial allocation, authentication,
content binding and worker retries.

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

The twelve schema migrations are embedded in the storage crate and applied under
a database advisory lock. They cover protocol/ledger durability, encrypted
content, retention, capabilities, financial programs, independent domain owners
authenticated receipt ordering, unified external work and owner-specific lifecycle rules.
Current protocol storage format is 6, financial storage format is 4, and command
wire format is 5. Older stored formats require an explicit offline migration. The current connector deliberately uses `NoTls`; it is
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
```

Generated auxiliary files can be removed with `latexmk -c`. Root-level PDFs are
the canonical review artifacts; `output/` is ignored to avoid duplicate generated
copies.
