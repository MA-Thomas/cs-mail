# Stage 4: persistence and lifecycle

Implemented September 14, 2026 after the [Stage 3 financial/work refactor](stage-3-financial-transitions.md).
The protocol specification and deployment profile remain normative.

## Stored owners and atomic commits

The relationship row contains current directed permission and follow-up policy.
Requests live in `cs_requests`; signed quote records own their terms and usage
marker. Shared histories, messages and request finances retain their separate
Stage 1 tables. Normal transitions assemble active requests plus their explicit
target; historical inspection is an explicit full snapshot. Financial/message
reads for transitions select those requests instead of every historical owner.

The member program is no longer one JSON document. Its metadata, members,
contribution lots, schedules, immutable quarter allocations, payables, ledger
accounts and journal entries have independent storage. Point operations select
the affected member, lot or payable. Quarter allocation assembles the program
records needed for the cross-owner decision. Its journal is append-only and is
loaded in full only for an audit snapshot. Identical owner/account values are
not rewritten by persistence.

A single transaction still commits all consequences: request/history changes,
financial postings, contribution exports, schedules, external work and outcome
evidence. Separation does not permit partial settlement. A regression injects a
work-insertion failure and verifies that every owner rolls back while the
already-authenticated command remains pending for retry.

## Replay results are not copies of live state

`CommittedTransition` is the common result for both the in-memory harness and
PostgreSQL: journal position, protocol events, issued terms and whether delivery
was admitted. Full `TransitionManifest` objects remain transient commit plans.
Neither adapter's replay result retains snapshots of all requests, messages,
financial records and balances. The in-memory harness retains a command digest
instead of copying the command body into its replay record.

PostgreSQL keeps the authenticated submission, policy and authority snapshot for
its declared replay lifetime and until signing/external work no longer needs it.
When that lifetime ends, deletion removes those bodies and detailed outcomes.
The replay fingerprint, command identity and original signed receipt remain.
An exact retry returns `ReceivedOutcome::Retired` with the original outcome digest;
it never executes again. Internal payment replays handle that outcome too.
Signed quote data remains available while a detailed replay still promises it.

## Executable lifecycle policy

Host administration installs an immutable versioned `LifecyclePolicy` containing
formation and detailed-replay lifetimes. `configure_lifecycle` registers existing
eligible records as well as installing the policy; subsequent commits register
new records. Each record receives its own pinned policy version and deadline.
A later policy does not silently shorten existing retention. No arbitrary
production retention periods are hardcoded into this refactor.

| Record | Earliest deletion basis | What prevents deletion |
| --- | --- | --- |
| Ciphertext | Its recorded content retention deadline | A scoped hold or unfinished delivery |
| Request formation | Closure plus formation lifetime | A scoped hold or an unresolved request |
| Message metadata | Validity end plus formation lifetime | A scoped hold, active request accounting or unfinished delivery |
| Signed quote | Quote expiry plus formation lifetime | A scoped hold, pending signing or detailed replay that still needs the quote |
| Submission/outcome bodies | Receipt time plus replay lifetime | A scoped hold, missing signed receipt or unfinished external work |

`update_retention` records a reasoned, record-specific hold/release or deadline
extension. It cannot shorten an existing deadline or resurrect deleted data.
`run_retention` respects both `delete_after` and `hold_until`, the pending-command
gate and live dependencies. It processes a bounded batch and defers temporarily
ineligible records so they cannot permanently monopolize that batch.

Request deletion preserves financial obligations and their verification evidence.
It does not alter relationship permission, shared eligibility/cooldown state,
member allocations or accounting balances. Minimal quote/message identity
records prevent identifier reuse after erasure. Financial owners and their
accounting journal remain settlement-audit records, not formation records; the
formation policy does not delete unpaid obligations or rewrite financial history.

Every deletion records a manifest in the same transaction. After restoring a
backup, `reconcile_restored_deletions` reapplies the manifests before serving
traffic and refuses restoration if pending work would resurrect deleted data.
Operators must preserve manifests outside the restored backup and import the
newer recovery evidence first. This implements physical deletion and restore
reconciliation; it does not claim that an offline backup has disappeared or
implement a KMS/backup-expiry service.

## Breaking schema and APIs

Migrations 10–12 replace the superseded work/storage paths. Current protocol
storage format is **6**, financial storage format **4**. Command/quote wire
format **5**, protocol semantics **2**, and receipt signing domain **v2** are
unchanged. Historical SQL migrations remain migration history; old runtime
representations and unused tables are removed.

Migration 10 explicitly refuses a populated earlier database before dropping or
reinterpreting any records. An existing installation requires an explicit offline
data conversion; this refactor does not provide a legacy decoder or silently
reset financial history. Fresh databases use the new schema directly.

`run_retention` replaces ciphertext-only purge. `claim_work` and fenced
acknowledgements replace the outbox APIs. Pending member operations are point
reads behind the common queue. Protocol execution results expose `transition`,
not the full commit manifest. The service and all tests use the replacement APIs.

## Verification

Validation uses the installed Rust 1.91 stable toolchain, with Cargo offline.
The repository-pinned Rust 1.88 toolchain was not available locally. PostgreSQL
17.6 was built into a temporary directory and run as an isolated local test
server, so the database tests were executed rather than merely compiled.

The complete suite covers financial transitions/evidence, concurrent shared
history and decisions, normalized program allocation/payout persistence,
restart and lost-response recovery, stale claims, independent worker failures,
transaction rollback, scoped holds and deadline extensions, late financial
confirmation after request erasure, detailed quote replay, receipt tombstones,
and deletion after restoring ciphertext from a backup.

Commands:

```sh
cargo +stable test --workspace --offline -- --include-ignored
cargo +stable clippy --workspace --all-targets --offline -- -D warnings
cargo +stable fmt --all -- --check
```

The test command requires `CS_MAIL_TEST_DATABASE_URL` pointing to an isolated
PostgreSQL database. Each integration test creates its own schema.

Final validation: **104 tests passed**, including **19 live PostgreSQL integration
tests**; none were skipped. Clippy with warnings denied, formatting and
`git diff --check` passed. The temporary PostgreSQL server was stopped after validation.
