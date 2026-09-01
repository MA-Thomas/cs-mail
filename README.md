# cs-mail

cs-mail is a communication protocol that puts economic friction at the boundary
of a new relationship rather than on every message. Accepted senders communicate
without protocol message fees. An unaccepted sender temporarily reserves a bond;
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
   nonnormative choices for an initial C-SQD deployment, including identity
   enrollment, funding, SMTP migration, and provider clearing.

The matching `.tex` files are the editable sources. `intro_doc.tex` is retained
only as a superseded design-history document and must not be used as a current
protocol reference.

## The model in brief

For an unaccepted sender, the recipient provider quotes:

- `C`: a processing charge;
- `S`: recipient collateral; and
- `L_k`: a refundable persistence reserve for repeated attempts, when required.

The sender reserves `C + S + L_k`. If the message is never admitted, cancellation
or a short admission timeout returns the entire reservation; the recipient's
decision window starts only at admission. Acceptance refunds `C + S`, releases
the persistence reserves associated with that public identity, and grants
directed permission without resetting principal-wide anti-abuse history.
Rejection transfers `C` to the recipient provider and `S` to the recipient, but
it does not permanently bar later attempts: another attempt may become eligible
after the applicable backoff. Expiry transfers `C` to the recipient provider and
returns `S` to the sender. A separate recipient block refuses future ordinary
contact until it is removed. Accepted ordinary communication requires no
protocol bond.

## Core vocabulary

- **Private principal** - the provider-local subject used for control, recovery,
  and repeated-attempt continuity. It is not a public or globally comparable ID.
- **Protocol identity** - the visible persona that sends and receives, such as an
  address. Permission is scoped to this identity.
- **Directed relationship** - the recipient-controlled permission from one
  protocol identity to another.
- **Rejection** - a relationship-wide decision declining the active solicitation
  and settling its decision-open admitted bonds while fully cancelling unadmitted
  reservations; it preserves backoff and permits a later eligible bonded attempt.
- **Block** - a recipient-controlled prohibition on future ordinary contact,
  distinct from declining one solicitation and reversible only by the recipient.
- **Relationship solicitation episode** - the recipient-facing grouping opened
  by the first admitted attempt for an unaccepted relationship. Later attempts
  join the active episode without generating repeated relationship prompts.
- **Bond** - the fixed `C + S` reservation attached to an unaccepted attempt.
- **Admission window** - the short interval in which reserved value must become
  an admitted message; cancellation or timeout before admission returns all
  reserved value and produces no solicitation.
- **Persistence reserve** - refundable `L_k` liquidity held to discourage repeated
  unsuccessful attempts; it is never provider or recipient revenue.
- **Canonical journal** - the authoritative ordering of protocol and ledger
  events.
- **Express lane** - a recipient-signed, rate- and time-bounded capability for
  bond-free communication without unrestricted standing acceptance.

## Status

The repository contains an executable Rust reference implementation of the base
protocol and its centralized deployment foundations. It includes the pure
transition kernel, conserved ledger, authenticated wire format, durable
PostgreSQL shell, endpoint content encryption, retry-safe workers, native client
and ingress APIs, privacy/retention types, and bounded adapter state machines.
The protocol and architecture remain drafts dated August 2026.

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
  complete settlement manifests.
- `cs-mail-application` atomically applies manifests to in-memory protocol,
  ledger, journal, schedule, outbox, and idempotency state.
- `cs-mail-storage-postgres` applies the same manifests inside row-locked
  PostgreSQL transactions and persists projections, ledger batches, transfers,
  events, idempotency results, leased outbox work, and leased schedules.
- `cs-mail-wire` defines a strict deterministic CBOR command representation,
  including deployment and intended-provider signing scope.
- `cs-mail-security` provides Ed25519 command authentication, canonical receipt-
  time key rotation and revocation, a hash-chained transparency log, golden
  vectors, and threshold recovery ceremonies.
- `cs-mail-content` provides authenticated endpoint-only HPKE content encryption using X25519,
  HKDF-SHA-256, and ChaCha20-Poly1305, with authenticated binding to message,
  sender, recipient, content reference, and protocol version.
- `cs-mail-worker` materializes deadlines as ordinary commands and publishes
  leased outbox work without acknowledging failed deliveries.
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

The implemented command set covers terms, reservation, admission, unadmitted
cancellation, acceptance, rejection, blocking, unblocking, expiry, deferred
persistence release, and revocation. Tests cover relationship-wide settlement,
identity-scoped acceptance, solicitation coalescing, replay, value conservation,
deadline boundaries, conflicting concurrent decisions, signature mutation and
scope, ciphertext tampering and wrong-key access, content-before-admission,
worker retry after lease expiry, retention protection for undelivered content,
funding finality, SMTP downgrade labeling, and federation commitment matching.

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
CS_MAIL_TEST_DATABASE_URL='host=/path/to/socket port=5432 user=postgres dbname=postgres' \
  cargo test -p cs-mail-storage-postgres --test postgres
```

The four schema migrations are embedded in the storage crate and applied under
a database advisory lock. They cover protocol/ledger durability, encrypted
content, and retention-safe outbox linkage. The current connector deliberately uses `NoTls`; it is
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
