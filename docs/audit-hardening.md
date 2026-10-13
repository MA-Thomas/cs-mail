# Audit hardening

## Paired requests — first request-cutover slice, 12 October 2026

`cs-mail-application::requests::RequestDecision` now coordinates both protected
directions. The protocol decides cancellation through `supersede_preparation`;
the application produces one opaque forward/opposite result. Persistence checks
the affected relationship and revisions and commits the declared consequences.
It does not choose an extra cancellation or rewrite the forward manifest.

A submitted request supersedes opposite preparation, including finalized funding
and an uncertain capture. The existing cancellation/refund lifecycle preserves
the original capture operation and returns the full charge. An actionable opposite
request refuses terms, creation and submission before new payment work. Its issued
decision deadline is interpreted at the existing trusted canonical receipt time;
an unrecorded expiry cannot indefinitely prevent an opposite request. Sender-side
blocking and revocation produce distinct `SenderReplyRestriction` refusals at
submission without restoring permission or delivering content. Guest requests
retain their existing behavior.

Both direction records share the initiating command's causal journal position.
Migration 0033 changes the journal primary key to `(position, aggregate_key)`;
allocating a separate opposite position could overtake a queued receipt and break
cursor interpretation. The migration refuses populated older environments and
requires a fresh database. The personal schema-32 databases, state directory,
running application and Keychain credential were not reset or replaced.

Five new PostgreSQL experiments challenge named consequential claims: complete
paired rollback and exact replay; finalized/unknown funding reconciliation;
refusal before payment and expiry without worker recording; sender block/revocation;
two independently connected concurrent submissions producing one winner and one
fully refunded loser; and populated-schema refusal without data loss. Related
observations share experiments rather than duplicating type-system guarantees.
The signed gateway and existing payment workers perform the effects; test-only
failure injection challenges rollback after the forward writes.

The final full gate passed all eight checks, 113 ordinary tests/doctests and 99
PostgreSQL/local-integration cases (`/tmp/cs-mail-paired-final-gate.log`). The five
targeted experiments also passed. Identity-model and its 101 pinned sources are
unchanged. No live banking or email, native UI, iOS runtime, sustained-load or
power-loss experiment is claimed. The in-memory kernel harness is not evidence
for this protected paired persistence boundary.

The full request/conversation cutover remains unfinished. Request documents,
conversation-confined answers, automatic reverse lanes, custody publication and
portable compose/reply workflows still need their coherent implementation and
validation. The existing native request-content path remains current until that
cutover; no second native reader or compatibility implementation was introduced.

## Relationship revocation repair — 12 October 2026

The source baseline before this pass is preserved in
`target/audit-baselines/cs-mail-baseline-20261012-before-passes.tar.gz`, with a
SHA-256 manifest alongside it. It includes tracked and nonignored source files,
including the uncommitted Apple/directory work and the inactive `to_delete/`
files; it excludes Git metadata, ignored runtime state, databases and Keychain.
The baseline is local and ignored by Git. The sibling identity checkout is
unchanged and all 101 pinned sources match.

Revoking standing permission previously left a retained lane usable. Blocking
cancelled that lane through a storage-owned decision omitted from the protocol
manifest. Both controls now have an explicit `LaneRevocation` consequence owned
by capabilities and selected by the protocol. This checked type binds the original
lane revision to its revoked successor. The manifest includes its schedule
cancellation. Persistence checks the protected revision and commits the declared
lane record, capability event, relationship, schedules and outcome atomically;
the former `revoke_lane_locked` business decision is removed. Relationship
revocation also accepts lane-only relationships. No migration or wire-format
change is needed for this repair; the source change does not repair records of
earlier revocations retroactively.

The existing PostgreSQL lane experiment was extended, with no new test cases.
It challenges the claim that blocking or revocation ends current lane access
without losing transaction atomicity or retry identity. Scenarios include standing
permission with a retained lane, lane-only permission, an injected capability-event
write failure, unchanged protocol/ledger/lane state after rollback, recovery of the
original pending command, exact command replay, refusal under terminated authority,
unblocking and replay of the old grant without lane reactivation. A counterexample
would allow communication after termination or leave partial permission effects.

The final full quality gate passed all eight checks, including 113 ordinary
tests/doctests and 94 PostgreSQL/local-integration cases. Evidence is in
`/tmp/cs-mail-revocation-final-gate.log`. These experiments do not establish every
possible native send/revocation race, power-loss behavior or corrupted-history
recovery. The in-memory kernel harness does not host lanes and is not evidence
for lane persistence. SwiftUI interactions, iOS behavior and live providers were
not exercised in this Rust repair.

The next request/conversation cutover is specified in the domain model and
application/persistence boundary document, and remains unimplemented. Its confirmed
rules include answers after expiry, first-submitted precedence over opposite
preparation, full funding cancellation/reconciliation, and preservation of existing
custom reverse lanes without allowance refill. No personal database was reset,
no credential was replaced, and no messaging support is claimed by this repair.

This records the bounded implementation passes following the critical review of
Claude's 7 October 2026 audit. The [Rust-domain principles](rust-domain-principles.md)
remain binding. Architectural hazards, observable bugs, and public attack paths are
not interchangeable severity categories.

## Implemented changes

- **Client operation identity.** An HTTP error, including a 4xx on a retry, does not
  establish that an earlier attempt never committed. The CLI retains its saved body
  and operation identity through errors and restarts. A returned operation outcome
  resolves the journal. Transport errors are no longer named business refusals.
- **Guest preparation identity.** Before any request preparation effects, a durable
  reservation binds the mailbox's operation to its recipient, class, payment token,
  and text. The guest domain owns the comparison rule; PostgreSQL's unique key
  serializes competing reservations. Only a purpose-separated keyed fingerprint
  is stored, never the plaintext. It also binds deployment and operation, preventing
  correlation of equal messages across operations. Subsequent preparation stages
  retain their existing durable identities. An altered retry is a conflict even if
  the original attempt stopped before content storage; an exact retry can resume.
- **Key history.** Revoking a retired key preserves both its retirement cutoff and
  its revocation instant. Repeated revocation cannot move either boundary. Historical
  command verification and current authority remain distinct, using the same lifetime
  predicate. Version overflow is checked before changing rotation/revocation state.
- **Ordered command recovery.** Processing faults roll back attempted effects and
  leave the original command pending. A redacted diagnosis distinguishes an unsigned
  quote, persistence trouble, and an internal fault requiring intervention. The last
  diagnosis and failed-attempt count are stored separately from the business outcome;
  successful processing clears the active diagnosis. Failure recording is best effort
  in the precise sense that a lost database connection cannot commit diagnostics;
  the original durable command still remains pending. Readiness reports the active
  diagnosis without exposing message contents or participant identifiers. Key registry
  maintenance uses the receipt-order lock but need not drain pending commands: each
  received command already owns its frozen authority. Artifact work continues during
  processing failure, and recovery retries the original operation in order.
- **Admission policy.** Ingress initialization inserts a policy only when absent.
  Explicit versioned updates remain separate. Content upload checks the durable
  policy under the transaction's ordering protection; the service no longer caches
  a potentially stale policy. Delivery still rechecks policy. Missing policy data
  is an error, not a fallback to the default. The replaced service error and cached
  evaluation path were removed.
- **Quality gate.** Dependency inspection fails when Cargo fails or returns no
  package, rather than interpreting missing output as no dependencies. Inspection
  includes all features. Both the all-feature and no-default-feature configurations
  receive strict Clippy checks.

Full-gate validation also exposed a local carrier-fixture read failure. Its listener
polls nonblocking so it can shut down; accepted sockets now explicitly use blocking
reads with the existing timeout. This changes development support only, not the
carrier adapter or its delivery semantics.

The global ordering constraint remains. Persistent faults require diagnosis and
repair; there is no automatic skip, fabricated refusal, or endpoint for editing
queue records. `/health` remains process health; `/ready` is unavailable while the
oldest pending command has a recorded processing fault. Supervisor liveness should
use `/health`, allowing workers to recover while readiness is unavailable.

## Cutover

**Use a fresh development database.** Migration 0027 refuses a populated deployment
rather than inventing missing original guest inputs or repairing previously erased
key cutoffs. It does not delete or reset the old database. Existing development and
live-test profiles must not be restarted against their old database with this build.
Preserve those databases as evidence and bootstrap a fresh one for subsequent work.
No old-format reader or parallel implementation was added. No identity-model changes
were needed. The CLI change cannot reconstruct requests discarded by an older client.

The guest fingerprint key is the existing guest content secret, with explicit HMAC
purpose separation. Keep it stable for outstanding operations. Rotating it requires
an explicit cutover; this pass adds no fallback key ring. Fingerprints are retained
with operation identity rather than used as content, mailbox authority, or payment
proof.

## Validation scope

The user authorized this pass under the principles. Experiments challenge these
claims; passing is limited evidence, not proof for every possible execution:

| Claim | Counterexample that would matter | Experiment |
| --- | --- | --- |
| Uncertain replies preserve the operation | Restart produces different signed/input bytes and permits another email or charge | Actual loopback HTTP responses (503, 401, malformed success), disk journal reload, and exact-body comparison |
| Guest retries preserve original inputs | Changed text is accepted after a partial failure or both competing inputs acquire the operation | PostgreSQL content-insert failure, unchanged-state check, exact retry, two independent database connections |
| Revocation cannot expand validity | A post-retirement or post-revocation signature becomes historically valid after another revoke | Extended key rotation/revocation scenario, repeated revoke, serialization round trip, current-authority refusal |
| A processing fault preserves one complete consequence | Partial effects survive, fault becomes refusal, key maintenance deadlocks behind it, or recovery duplicates the obligation | Extended PostgreSQL worker scenario with unsigned quote and a trigger failing the final receipt update; retry after repair |
| Durable policy survives reopening | Service initialization overwrites/rejects an upgraded policy, or a stale gateway accepts a forbidden critical extension | Reopen gateway, admit supported content, upgrade policy again, refuse content without storing it |
| A populated database cannot silently bypass cutover | Migration accepts old data lacking the new guarantees | Simulate pre-0027 schema with a deployment, refuse migration and verify retained data |
| Failed dependency inspection fails the gate | Cargo tree fails but gate reports success | Temporary fault-injected Cargo executable; require nonzero gate exit and dependency FAIL |

The complete local quality gate passed on 7 October 2026: all eight checks, 107
ordinary tests and doctests, and all 75 PostgreSQL/local-integration tests. The latter
were explicitly run, not counted from the ordinary run's ignored cases. The gate now
has an additional production Clippy check. Existing guest plaintext-storage scans also
cover the new input table. No live carrier messages or real payment operations are
part of these experiments. Power-loss durability, arbitrary corrupt restored records,
all scheduler lease races, sustained load, and broad operational repair tooling remain
outside this pass.

## Subsequent work

### Guest verification follow-up

The next bounded assignment implements verification-backed guest sessions. The
domain owns private verification state (`AwaitingCode` or `Consumed`) and a checked
record format. Successful verification produces both consumption and session effects;
the storage adapter commits them together. A unique foreign key associates each
session with its verification, and the consumed record binds the exact token digest.
Session facts have one owner instead of being copied into independent session JSON.

Removed: `GuestSession::open`, writable session/verification fields, direct session
deserialization, the independent stored session format, and the old verification
`used` flag. There is no compatibility constructor, alias, old-format reader or
backfill. Migration 0028 refuses populated older databases and requires a fresh one.
No identity-model changes were needed.

Issuance and confirmation receive a clock and sample it after acquiring their
database protection. Session lookup samples it after the read. Domain functions
receive the resulting time explicitly. Restored verification records reject invalid
attempt counts, invalid mailbox syntax, overflow and inconsistent lifetimes; session
restoration additionally requires consumed state, the exact token and current time.
Raw database JSON remains evidence from a trusted repository, not an authenticated
capability. These checks cannot defend against a privileged actor fabricating a
consistent database history.

Four PostgreSQL experiments challenge duplicate confirmation, exhausted attempts,
partial commits, expiry across real database waits, and invalid restored evidence.
The existing cutover experiment now covers migration 0028 as well. A failed session
insert leaves the original code usable; concurrent successful code submissions
produce exactly one session. A lost response after a successful confirmation still
requires a new verification; this pass does not add session-token recovery.

The complete local quality gate passed for this follow-up: all eight checks, 107
ordinary tests/doctests and all 79 PostgreSQL/local-integration tests. No suites
were skipped. The four new PostgreSQL experiments and the extended cutover
experiment also passed in a targeted run.

The scope is session issuance and restoration. SMTP evidence, express-lane authority,
other restored records, freshness after subsequent waits in downstream guest request
processing, real mailbox delivery, crash/power-loss injection and live financial
activity remain outside this pass.

### Inbound SMTP and express-lane follow-up

The durable inbound boundary now requires a gateway-produced `AuthenticatedReply`.
The gateway binds the verifier's exact text, mailbox, conversation, replay identity
and encrypted content in one immutable value. Its fields are private and it has no
public raw constructor or deserializer. The storage adapter converts it into plain
application facts and runs deterministic admission against protected current state.
The former freely constructible application `AuthenticatedReply` is removed; `Reply`
explicitly represents facts, not authentication authority. Encryption and replay
fingerprinting have one implementation in the gateway. Development fixtures now use
that same verification/encryption path with an explicit synthetic verifier.

Express-lane activation requires `VerifiedLaneGrant`, produced by checking the
recipient signature. The replaced `ValidatedLaneGrant` and its raw-grant conversion
are removed. Receipt checks current authority and stores the original signed grant.
Processing restores proof against the receipt's frozen authority and time, preserving
ordered receipt semantics after subsequent key revocation. Damaged stored signatures
leave the command pending with a processing diagnosis, rather than creating a lane or
recording an invented business refusal. Relationship permission and settlement still
commit together through the existing application decision and storage transaction.

`Lane` fields are private, and restoration checks grant structure, lifetime bounds,
lifecycle consistency, rate-window counts, admitted message identities and version.
Legacy domain names also use checked deserialization. Lane transitions calculate
fallible consequences before changing state, so overflow cannot leave a partial
mutation. Repeated in-memory grant operations compare the complete grant, preventing
an altered grant from masquerading as an exact retry. Validated restored records
remain facts from trusted storage, not cryptographic proof of an unaltered history;
a privileged actor able to fabricate a consistent database history is outside this
boundary's guarantee.

Migration 0029 requires a fresh development database because earlier queued grants
omitted the signature needed for recovery. It refuses populated deployments without
deleting them. No compatibility alias, fallback reader or parallel implementation
was retained, and identity-model and its source pin are unchanged. The receiver's
existing-schema opener now checks the complete embedded migration list instead of
its stale hard-coded version 26; fresh-schema reopening and missing-migration
refusal are covered by the extended cutover experiment.

The focused experiments challenge these consequential claims:

| Claim | Falsifying observation | Experiment |
| --- | --- | --- |
| Authentication does not preserve expired permission | Prepared mail commits after a block or expiry across a lock wait | Block after preparation; real PostgreSQL lane-row wait with explicit clock advance |
| Message and allowance commit together | Failed lane update leaves a message, or racing retries consume twice | Injected lane-update failure, full rollback checks, two independent connections retrying the same email |
| Restoration preserves consistent lane state | Reset counters or extended horizons permit new delivery | Malformed counter, horizon, identity, version and domain records; used/revoked lane round trips; database error followed by repair on the same connection |
| Failed transitions leave state unchanged | Arithmetic failure changes any part of the lane | Overflow during consume, revoke, horizon and reconfirm transitions |
| Recovery preserves receipt-time authority | Corrupted signed contents activate, or subsequent key revocation erases authenticated work | Alter stored grant, require pending fault with no lane, repair fixture, revoke current key, process original receipt |

The complete local quality gate passed: all eight checks, 109 ordinary tests and
doctests, and all 82 PostgreSQL/local-integration tests, with no suites skipped.
The three new PostgreSQL experiments and two new domain experiments also passed.
These experiments do not cover arbitrary consistent database tampering, power loss, sustained load, live
Internet mail, real payment operations, or every other persisted domain record.

### Guest-request authority across waits

The paid guest request path now carries its verified session through preparation,
command receipt and encrypted-content storage. A session establishes mailbox control;
it does not establish payment or permission to communicate. The guest domain still
owns the lifetime rule, and each protected action passes a newly sampled time to it.

Input reservation checks session expiry after its potentially blocking insert/read
and rolls back an expired attempt. Relationship preparation checks the session and
sender binding after loading its protected history context; its deterministic plan
uses that same time. Command receipt acquires authority and relationship protection
before sampling time, then rechecks the command's session. Guest content storage
checks session ownership and its certificate using time sampled after protection.
Private status and request answers receive a final session check after intervening
reads or work before being returned to the guest.

Durable acceptance remains the dividing point. Processing an already received command
and fulfilling its refund obligation do not depend on a still-live browser session.
An exact internal receipt replay recovers historical work and creates no new authority.
The public guest API still requires a current session to obtain private results.
Re-verification of the same mailbox does not change the original request's operation
identity: transient session evidence is excluded from the command's replay bytes.
Changed request inputs remain a conflict, and existing request deadlines remain fixed.
Previously accepted preparation stages can remain when a later stage is refused;
expiry does not erase their evidence or promise an all-or-nothing multi-step request.

Removed: preparation from mailbox facts alone, guest content storage without session
authority, and caller-sampled timestamps for those storage boundaries. No compatibility
entry point, fallback or parallel implementation was kept. Stored command bytes and
schema are unchanged, so this pass adds no migration; the preceding fresh-database
requirement through migration 0029 still applies. identity-model is unchanged.

The four new PostgreSQL experiments and extended retry scenario challenge these claims:

| Claim | Falsifying observation | Experiment |
| --- | --- | --- |
| External waits cannot preserve expired authority | Tokenization completes after session expiry and creates request preparation effects | Paused tokenizer, explicit clock advance, absent durable effects, fresh verification and retry |
| Storage waits cannot preserve expired authority | Reservation, history preparation, command, content or private status succeeds after expiry | Independent connections, observed PostgreSQL lock waits, clock advance, refused operation and unchanged state |
| Re-verification cannot duplicate an interrupted request | A new session changes operation identity or permits altered content | Content-insert failure, expiry, re-verification, altered-input rejection, exact retry and concurrent conflicts |
| Accepted work outlives the session | A queued cancellation stalls or refunds twice after expiry | Receive before expiry, process afterward, run payment work, renew and replay, require one refund to the original funding token |

The complete local quality gate passed: all eight checks, 109 ordinary tests and
doctests, and all 86 PostgreSQL/local-integration tests. No suites were skipped.
The targeted guest-request run also passed all nine scenarios. These are controlled
local experiments, not real payment tokenization or Internet mail. They do not cover
all timing interleavings, power loss, privileged database tampering, session revocation
(which is not introduced here), or unrelated member authorization paths.

### Remaining sequence

Before broader member/guest use, review remaining consequential restored records.
Complete public SMTP and provider-callback deployment checks before treating local
external-email experiments as evidence of Internet operation. The macOS member
application remains the next major interface milestone. Keep historical evidence
separate from current permission.

Before real financial activity, independently anchor bank/payment trust, complete
refund attempt recovery, distinguish restriction causes, and route bank replacement
through overlap review without prohibiting approved shared bank accounts.

Move older storage-owned business decisions by complete use case, keeping locks and
atomic writes in storage. Improve meaningful history and identifier types as those
owners change. Do not collapse carrier acceptance into delivery, mechanically replace
independent flags with enums, or discard ordered commands on unexpected errors.
