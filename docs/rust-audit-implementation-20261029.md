# Rust audit implementation — 29 October 2026

This work follows `rust-domain-principles.md`: application code owns processing
decisions, adapters supply evidence and commit complete changes, and the installed
client has one supported cache reader. It does not complete the product build plan.

## Implemented boundaries

- **Processing:** typed participant/account owners determine conservative ordering
  scopes. A failed command blocks its owners while ready commands for disjoint owners
  can progress. Transactions still use a global serialization lock; this is failure
  isolation, not a claim of parallel write throughput. Program-wide financial fences
  remain conservative. Readiness exposes degraded processing separately from database
  availability.
- **Recovery:** signed operator inspection and resume bind a position to its exact
  command digest. Resume clears an unfinished command's failure; it cannot rewrite
  the obligation. Authentication, replay and transaction boundaries use the existing
  operator contract. Intervention failures require explicit resume; retryable failures
  use the worker interval, without a new persisted backoff policy.
- **External observations:** a transactional clock orders insertion and delivery
  changes. Reads carry typed observation bounds and clients merge by immutable message
  identity, committing each page with its cursor. Bounds describe change progress,
  not a historical snapshot of mutable delivery state. Existing signed mutation
  bytes retain their command signing domain. Expiration is interpreted explicitly.
- **Device storage:** cache v6 separates mailbox contents into authenticated encrypted
  records identified by opaque keyed digests. SQLite commits the head, referenced
  records and submission state atomically, and removes unreferenced records. Runtime
  opens only v6. A feature-gated offline conversion verifies the original identity and
  every old field before producing a replacement transaction. The client still
  hydrates the mailbox in memory; head metadata/reference limits and large-mailbox
  startup latency remain future work.
- **Presentation contracts:** Rust/FFI exposes initiation and conversation reply
  actions separately, with observation time, and explicitly names host-managed recovery
  custody. This does not redesign the current Swift screens or add isolated key custody.

The payment processor and bank-verification provider remain undecided by the owner.
The running environment therefore continues to use local simulation. No live provider
evidence contract, public delivery milestone or stronger custody claim is established.

## Validation and conjectures

Existing scenarios were extended rather than adding new test cases:

- A storage failure must roll back the blocked owner's effects while an independent
  pair progresses; signed resume must reject a foreign operator and preserve replay.
- Concurrent conflicting decisions must yield exactly one success and one version
  conflict. Fixture preparation precedes the race.
- A changed delivery observation must merge without losing immutable correspondence;
  page progress and saved records must move together.
- Independent encrypted mailbox records must exceed the former aggregate payload
  ceiling without weakening atomic submission rollback or exclusive device ownership.
  The native fixture used 1,050 records of 64 KiB, exceeding 64 MiB in aggregate.

All eight checks in `scripts/quality-gate.sh` passed, including PostgreSQL suites,
production linting, identity manifest and packaged search resources. Log:
`/tmp/cs-mail-audit-quality-pass.log`. Native storage/isolated Keychain checks passed:
`/tmp/cs-mail-audit-native.log`; browser enrollment was explicitly skipped. Canonical
Apple build/install passed. Running inspection opened preserved native and external
correspondence and refreshed the original mailbox. No messages or payments were sent.

Not covered: iOS, live payment/bank providers, public MX delivery, exhaustive delivery
callback interleavings, a production-scale mailbox benchmark, lazy mailbox loading,
or isolated/HSM custody. Existing UI edits present before this work were retained.
Identity-model source was unchanged. Tests challenge the listed boundaries; passing
them is not evidence for those uncovered claims.

## Installed development continuity

The canonical app remains `target/apple/CsMail-continuity-20261012.app`, with its
original signing requirement and original Keychain account association. The credential
was read through an anonymous pipe for offline verification, compared before/after,
and never replaced or written to an artifact. Account
`99093951882271847928833656445567929292` and persona
`331288778501094320672877597944731204898` remain in use. The protocol address
`mtallis62831@c-sqdmail.com` and the `marcus.thomas`, `mat7771` and `m11a33t`
aliases at the same domain were preserved.

The existing launch job `com.csqd.cs-mail.owner-readable-aliases` now uses
`target/rust-audit-owner-20261029/start.sh` and its pinned simulation-enabled server.
Its database is `cs_mail_rust_audit_owner_20261029`, schema 46. Conversion compared
109 original tables and nine sequences, and verified unchanged key-file hashes.
The previous database `cs_mail_unified_aliases_owner_b_20261021` remains untouched.
Evidence: `target/rust-audit-owner-20261029/verified.json` and `running.json`.
The latter confirms running executable inodes/checksums and healthy readiness.

Device conversion used `tools/development-continuity/mailbox-records-input.swift`,
the `mailbox-records-upgrade` binary and `apply-mailbox-records.py`. The helper holds
the client lock, checks the source head digest and preserves workspace, personal and
retained-copy tables. The actual device store remains at
`target/apple-continuity-state-20261012/device-v3/member.sqlite`.
`target/rust-audit-device-20261029/original.sqlite` and `plan.json` are offline
cutover evidence, not runtime fallback readers. Retain the original database/device
checkpoint until owner acceptance of this continuity upgrade; retire them together
after that acceptance. Do not independently restore an old device checkpoint against
a subsequently changed server. The historical continuity tools are not a supported
runtime or a general-purpose migration chain; only the new conversion target was built
for this upgrade.
