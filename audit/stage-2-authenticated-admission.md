# Stage 2: authenticated admission and receipt

Implemented September 14, 2026. The subsequent [Stage 3](stage-3-financial-transitions.md)
and [Stage 4](stage-4-persistence-lifecycle.md) guides describe the current worker,
result and storage APIs; this guide records the Stage 2 implementation.
 Stage 1's relationship, request, history, message
and financial owners remain authoritative. This stage changes the boundary that
receives commands and coordinates those owners.

## Receipt comes before execution

The durable path is now:

```text
signed submission / gateway proof
    -> verify stored scope and authority under the receipt lock
    -> commit canonical receipt time, position, command and authority snapshot
    -> process the oldest pending receipt
    -> commit outcome + state/effects + receipt/quote signing work
    -> materialize and retain the signed artifacts
```

`receive_signed`, `receive_message`, `receive_grant`, `receive_control` and
`receive_legacy` return an opaque `ReceivedCommand`. They do not perform admission.
`process_next_received` processes the oldest inbox entry across relationships;
`process_received` resolves a particular handle by draining its predecessors.
A fresh time is sampled inside the receipt lock. Retries reuse the original time.
Ciphertext availability is recorded at receipt, so a later upload cannot backdate
admission eligibility. Receipt positions and state-change journal positions use the same sequence and,
for a received command, the same position.

This reference deployment uses one database-wide receipt lock. That deliberately
serializes operations affecting shared history across aliases as well as those
on one relationship. It is a correctness boundary, not a throughput optimization.
Database locks never span external signing-service, payment-provider or DMARC I/O.

Registry changes, admission-policy replacements, financial administration, lane
horizons and content deletion cannot pass pending commands. They return
`PendingCommands` until earlier work is processed. Reinstalling an identical
admission policy is allowed during restart. Deadline workers drain received work
before claiming expiries. A decision received within its decision window is
therefore evaluated using that receipt time even if processing happens later.

## Authentication evidence is not ordinary command data

`KernelCommand<T>` replaces `Authorized<T>`. It is explicitly untrusted data for
the pure kernel; its constructor does not claim verification. The durable engine
has no entry point accepting it from a caller. `VerifiedCommand` and
`VerifiedLegacyAdmission` have private construction and cannot be deserialized.
The gateway proof binds DMARC evidence to the deployment's synthetic sender
identity and creates an honest legacy origin declaration. It does not require a
lane: an accepted relationship can authorize a legacy message.

Durable ingress verifies native signatures against the stored registry while
holding the receipt-order lock. Passing a previously fetched registry or a
previously verified command is no longer an ingress API. Payment evidence and
scheduler authority are checked before internal work enters the same inbox.

An exact replay fingerprint includes the signature, not just the command body.
Consequently the original signed submission retrieves its committed result even
after revocation, while an unsigned copy cannot act as that retrieval capability.
New submissions still undergo current authority checks.

Native uploads verify the content-key certificate and persist the ciphertext
under one authority lock. `store_trusted_content` is an explicitly named host/SMTP
storage facility with a required retention policy; it does not authorize delivery.

## Admission checks and consequences have separate owners

`cs_mail_protocol::admission` owns the shared content/declaration/policy checks.
Every route checks message, content, relationship, sender, recipient, protocol,
declaration digest, declaration authority, capability binding, content lifetime,
message validity, and supported critical schemas. Upload preflight does not
replace admission-time policy evaluation.

`plan_free_admission` selects accepted permission before an attached capability;
block takes precedence over both. It returns a message and, when needed, the next
lane state without performing I/O. Failed checks consume no allowance. Request
and follow-up commands carry explicit request intent and remain subject to their
request lifecycle and version checks. They never manufacture a charged request
as a fallback from a failed free admission.

For request deliveries, storage supplies an admission assessment to the pure
kernel. The request owner decides its consequence:

| Proposed delivery | Refusal consequence |
| --- | --- |
| Initial message of a preparing request | Cancel preparation, release its history reservation, and request capture cancellation or a full refund |
| Request follow-up | Refuse this message; keep request, deadline, history and finances intact |
| Accepted or lane message | Refuse this message; create no financial effect and consume no lane allowance |

The initial failure path works both before and after capture confirmation. Storage
does not synthesize a second provider cancellation command or implement refund
logic. The in-memory application remains a trusted kernel harness; it does not
host ciphertext and supplies successful admission assessments. The PostgreSQL
adapter supplies the real assessments.

## Explicit outcomes and recoverable artifacts

`ReceivedOutcome` distinguishes protocol outcomes, message admissions, lane
changes and typed refusals. An initial admission cancellation carries its
`admission_failure` alongside the committed settlement manifest. Infrastructure
failures leave the inbox entry pending; they are not converted into signed domain
refusals. No-change outcomes and refused commands also receive evidence.

`IngressService::submit` and its lane/message methods return `ServiceOutcome`,
containing the stored outcome, signed receipt and any signed terms. Receipt kind
is derived from the operation and result, not the first emitted event.

Receipt intent is stored atomically with the outcome and all state/effects.
`sign_pending_artifacts` materializes both quotes and receipts from durable work;
`run_received_batch` provides bounded restart/background processing. An exact
retry returns the same signed artifact. The transport's `replayed` flag is
excluded from outcome serialization and therefore cannot change its digest.

The receipt's authority snapshot survives later registry changes. Recovery needs
a signing key valid in that snapshot; deployments must retain appropriate keys
until their pending artifacts have been materialized. The signing pass checks
that key against the snapshot and stores the quote's verifying key with the
quote. Revocation does not silently rewrite historical quote evidence.

## Financial scope and breaking representations

`FinancialScope` binds deployment, operator, program, payment account and protocol
version. It is included in immutable financial terms, signed administration
commands, request operation IDs, payout/allocation/correction IDs and payment
operation digests. Provider evidence authenticates the scoped operation digest.
An unchanged, valid signature cannot be replayed into a different configured
program. The current store supports one configured program per settlement unit;
conflicting scope initialization is refused.

The superseded authorization wrapper, direct durable execution/admission methods,
manual quote attachment and manual receipt insertion are removed. There are no
legacy aliases or alternate decoders. Migration 9 adds the inbox and pinned
configuration. Current formats are protocol storage **5**, financial storage
**3**, command/quote wire **5**, and receipt signing domain **v2**. Protocol
semantics remain version **2**. Historical schema migrations remain migration
history; older stored representations require an explicit offline migration.

## Verification

Tests cover shared checks across all four message bases, accepted-permission
precedence, honest lane-free legacy proof, pending/confirmed capture refusal,
follow-up isolation, scope-separated IDs/evidence/admin commands and proof types
that cannot be deserialized. PostgreSQL regressions cover durable receipt/restart,
a queued timely decision versus expiry, revocation races, exact replay after
revocation, signed refused/no-change outcomes and policy changes after upload.

Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo fmt --all -- --check`. Live database tests additionally require an
isolated `CS_MAIL_TEST_DATABASE_URL` and `cargo test --workspace -- --ignored`.

Local verification for this change used the installed Rust 1.91 stable toolchain
with offline Cargo: **82 tests passed**. All **14 PostgreSQL integration tests**
compiled but were skipped because no PostgreSQL server or test database was
configured. Clippy with warnings denied, formatting and diff-whitespace checks
passed. The repository-pinned Rust 1.88 toolchain was unavailable locally.
