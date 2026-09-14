# Stage 3: financial transitions and external work

Implemented September 14, 2026, on the Stage 1 owners and Stage 2 authenticated inbox.
See [Stage 4](stage-4-persistence-lifecycle.md) for persistence and retention.

## Financial behavior belongs to finance

`RequestFinancials` now owns its immutable financial contract, settlement decision,
capture status, refund obligation and evidence history. Its constructor validates
amounts, scope and authority. Its statuses cannot be changed directly by protocol
handlers. `settle` and `record_payment` are atomic pure operations returning
`RequestFinancialEffects`: accounting postings, provider operations to execute,
forfeiture exports and review holds.

The request kernel decides whether acceptance, rejection, expiry or cancellation
is permitted. It asks the financial owner to settle and incorporates its result
into the same atomic manifest. It no longer constructs refunds, changes payment
statuses or writes settlement accounting entries itself.

| Request decision | Financial consequence |
| --- | --- |
| Accepted | Full refund obligation |
| Rejected | Processing revenue and collateral export to the program |
| Expired | Processing revenue and collateral refund obligation |
| Cancelled before collection | Cancel/reconcile the existing capture operation |
| Cancelled after collection | Full refund obligation |

A cancelled request can be erased while capture reconciliation remains pending.
Late confirmation creates the refund using the financial contract alone. Provider
completion never reopens the conversational request. Capture reversals are
monotonic, preserve accounting conservation and identify affected contributions
for review. An old confirmation cannot undo a recorded reversal.

Both request finances and member payables remember verified event identities.
Exact duplicates have no new money effect; contradictory evidence for the same
event is refused. Unsupported transitions fail atomically. Member payout retries
continue to use the original operation; a separately reversed payout can be
prepared again under its next operation identifier.

## One lifecycle for external work

`cs_work` replaces the delivery/payment outbox and the separate member-payment
work list. `WorkPayload` preserves the distinct meanings of delivery effects,
request payments, member payments and artifact signing. A typed `WorkSource`
binds the work to a received command or payout operation.

The lifecycle is `ready -> running -> complete`. A transient or uncertain failure
returns work to `ready` with capped exponential backoff. Missing content, invalid
evidence, permanent delivery failures and unavailable signing authority can leave
an explicit `blocked` item. `resume_work` is a scoped host administration API for
resuming that item after its cause has been resolved. Failure codes contain no
provider tokens, content or raw dependency errors.

Every claim receives a fresh database sequence token. Completion, retry and block
updates require the owning queue and exact token; an earlier worker cannot
acknowledge or release a later worker's claim. Claims survive process death through
expiry and reclamation. Scheduled work also has a fencing token.

Payment workers look up the original provider operation before submitting it.
A lost response therefore leads to reconciliation, not another charge. Delivery
sinks must deduplicate by stable work ID or delivery-intent reference. Exactly-once
execution across an external system is not assumed.

All external workers claim bounded batches and isolate individual failures. Member
payments no longer scan every payable. Reports distinguish completed, retried,
blocked and lost-claim work. Schedule workers drain a bounded inbox prefix and
wait for another pass if earlier received commands remain; the Stage 2 ordering
gate still prevents deadlines from overtaking timely decisions.

Artifact workers load pinned signing evidence, release database locks, sign, and
persist stable quote/receipt artifacts. A crash after storing the artifact but
before acknowledging its work is recoverable without replacing the original
signature. A missing historical key blocks that item while other items proceed.

## Removed paths

The protocol-owned settlement/refund/payment implementations, bare-ID outbox
acknowledgements, unleased member-payment scan, old worker error-on-first-item
behavior and receipt-scanning signing loop have been replaced. There are no
compatibility aliases, parallel executors or alternate old decoders.

The public worker API uses `WorkQueue`, opaque `WorkItem` claims and `WorkReport`.
`sign_artifacts_batch` takes explicit work time, lease and batch limit. Callers
must use the appropriate signing key from the pinned authority snapshot.

Validation: the complete Stage 3/4 suite passed all 104 tests, including 19
PostgreSQL tests. See the Stage 4 guide for the commands and environment.
