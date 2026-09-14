# Stage 1: domain model and ownership

Implemented September 14, 2026. Stage 2 subsequently updated authentication,
admission and formats; see [the Stage 2 guide](stage-2-authenticated-admission.md).
This note describes the Stage 1 domain refactor;
the protocol specification and the financial-program section of the deployment
profile remain normative.

## One owner for each kind of fact

| Owner | Authoritative facts | References to other owners |
| --- | --- | --- |
| `Relationship` | Directed permission, its version, request generation counter | Shared `RequestHistoryRef` |
| `RelationshipRequest` | Immutable quoted terms, initial message ID, generation, lifecycle | Capture operation ID |
| `RequestHistory` | Principal-recipient request level, eligibility time, eligibility version, preparation reservation | Reservation names the exact relationship and request |
| `Message` | Admitted content, delivery reference, declaration digest, validity and admission time | An `AdmissionBasis`: initial request, request follow-up with policy version, accepted relationship version, or lane version |
| `RequestFinancials` | Capture/cancellation progress, refund obligation/progress, verified payment evidence | Exact provider operations |
| `FinancialProgram` | Per-unit transaction coordinator and ledger | Membership, forfeiture lots, immutable quarters and member payables |

`ProtocolState` contains relationship-local policy, quotes and requests. It does
not contain shared history, financial execution records or admitted messages.
`SettlementSnapshot` explicitly assembles those owners under the adapter's locks;
the manifest returns their new values for one atomic commit. Immutable quote
terms deliberately retain the history/version/prices at issuance. They are
contract evidence, not a second live history owner.

A request lifecycle carries the facts appropriate to its phase:

```rust
Preparing(Preparation)
Open(RequestAdmission)
Accepted { admission: RequestAdmission, event: EventRef }
Rejected { admission: RequestAdmission, event: EventRef }
Expired { admission: RequestAdmission, event: EventRef }
Cancelled { preparation: Preparation, event: EventRef }
```

There is no combination of `Open` plus a missing admission time or decision
deadline. Resolved requests preserve those facts. Capture and refund status live
in finance, so acceptance can coexist with a pending refund, and a late capture
can create a refund for a cancelled request without reopening it.

## Solicitation and message grouping

`AttemptId`, `EpisodeId`, `SolicitationEpisode`, `SolicitationStatus`, and the old
request state representation have been removed. Each request has one generation
within its relationship. `active_request()` derives the active solicitation;
`request_messages()` derives its message group. Initial admission emits one
`EstablishRequestSolicitation { request_id, generation }`. Follow-ups append
`Message` records and leave the request's charge, lifecycle and deadline alone.
The follow-up count and interval count derive from those same message records,
replacing separate message-digest and timestamp collections.

Both paid admission and existing accepted/lane admission paths persist the same
`Message` representation. Their authentication and admission execution paths are
still separate; their consolidation belongs to Stage 2.

## Shared history and transactional ownership

`InMemoryStore` owns all relationships, their shared histories, and one member
program per settlement unit. `InMemoryEngine` is a relationship handle into that
store. Alias tests now exercise actual shared state instead of copying a history
from one isolated engine to another.

PostgreSQL stores live history in `cs_request_histories`, request financial
records in `cs_request_financials`, and admitted messages in `cs_messages`.
Relationship transactions lock the relationship and then its shared history.
Messages, payment changes, ledger postings, schedules and outbox effects commit
with the request transition. Idempotency manifests remain historical results,
not authoritative live copies.

A preparation reservation names `(relationship, request)`, so equal request IDs
on different aliases cannot be confused. If another alias changes eligibility
while capture is in flight, initial admission cancels the stale preparation.
Confirmed capture is refunded; pending capture is cancelled/reconciled normally.
Cooldown updates take the maximum existing deadline. The history version tracks
eligibility changes; the reservation is protected by the shared transaction lock.

## Member program

Membership eligibility is evaluated by the member record. A forfeiture lot moves
from pending/held to cleared with evidence, then to assessed in one quarter.
Clearance evidence and assessment can no longer be independently toggled.
A member payable moves through `Due`, `Pending(operation)`, `Paid(operation)` and
`Reversed(operation)`. Its methods prepare and verify provider operations and
return postings for the program to commit. Prior reversed operations remain
financial evidence for late receipts; they are not legacy implementations.
Quarter schedules and allocation records have their own module. The program
still coordinates atomic allocation and ledger changes. Replacing whole-program
JSON persistence and revisiting retention remain Stage 4 work.

## Breaking representation changes

Command wire format is now **4** (`cs-mail/command/v4` and
`cs-mail/request-terms/v4`); protocol semantics remain version **2**. CreateRequest
no longer encodes an attempt ID, and terms no longer encode an obsolete sender
ledger-account reference. The corresponding unused identifier/deriver was
removed. Principal-recipient derivation uses `cs-mail/request-history/v1`.

Migration **0008** installs protocol storage format **4** and financial-program
storage format **2**. Older formats are explicitly refused. There is no legacy
Rust model, decoder branch, compatibility alias or dual-write path. Historical SQL
migrations and existing database records remain intact: starting this binary does
not delete or reinterpret financial history. Existing installations need an
explicit offline data migration before use; one is not supplied by this refactor.

## Validation

`cargo +stable test --workspace --offline`,
`cargo +stable clippy --workspace --all-targets --offline -- -D warnings`, and
`cargo +stable fmt --all -- --check` validate the change. The installed stable
toolchain is Rust 1.91; the repository pins 1.88, which is unavailable here.

New executable tests cover real concurrent aliases, capture during an alias
cooldown change, three messages/one solicitation/one refund, and serialization
through preparing, open, accepted and refunded phases. Existing cancellation,
late-capture, provider reversal, quarter allocation and payout tests use the new
owners. A PostgreSQL integration test checks separate storage, shared alias
history, reconnect, and rejection of the previous format. PostgreSQL tests are
compiled but ignored here because no test database or PostgreSQL runtime is
available; database execution remains unverified in this environment.
