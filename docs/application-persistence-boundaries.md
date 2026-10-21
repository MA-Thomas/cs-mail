# Application and persistence boundaries

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

The subsequent [product identity alignment](product-identity-model.md) introduces
product-scoped durable login bindings and shared-enrollment schema version 2.
The schema/version statements below describe the earlier application cutover.

The September follow-up moves the account, billing, shared-identity enrollment,
identity-change and encrypted-disclosure workflows into application-owned Rust.
It replaces their earlier PostgreSQL business entry points. There are no forwarding
methods, legacy façade aliases, or alternate implementations of the replaced workflows.

## Ownership

| Owner | Responsibility |
| --- | --- |
| Domain crates | Validated state, signatures/proofs, business invariants and transitions |
| `cs-mail-application::accounts::AccountEnrollment` | Reserve, renew and activate enrollment; construct all initial account/financial records |
| `cs-mail-application::accounts::operations::AccountService` | Authenticate management commands and apply identity security changes |
| `cs-mail-application::billing::operations::BillingService` | Service purchases, collection retries, dispatch authorization, payment confirmation, funding reverification and distribution preparation |
| `identity-application::enrollment::EnrollmentService` | Evidence verification, subject/login resolution decisions, enrollment/change policy and signing |
| `identity-application::workflows::EncryptedWorkflowService` | Build/encrypt workflow records and coordinate authorized disclosure, audit and key access |
| PostgreSQL adapters | Consistent context, atomic writes, uniqueness, serialization, durable audit and queue fencing |
| HTTP/worker hosts | Assemble adapters, trusted clocks, verifiers and signing dependencies; execute external I/O |

`identity-enrollment` now contains HTTP transport and executable composition. Its
application types are imported from `identity-application`; its database client is
owned by `identity-storage-postgres::enrollment::PostgresEnrollmentStore`.
Signing secrets are held by the application signing dependency and never passed to
storage. The application crate has no PostgreSQL, SQLx or HTTP dependency.

## How an atomic operation works

The application invokes an operation-specific persistence port with a local decision
callback. The adapter opens a transaction, establishes the required protection, and
loads typed context. The callback authenticates the request and coordinates domain
transitions against that context. It returns an opaque decision containing either a
historical replay or the complete persistence effects. The adapter writes those effects
and releases the result only after commit.

Decision constructors are confined to the application. Adapters receive readable
persistence records, not permission to choose which business transition to apply.
The callbacks receive no SQL connections or transactions. They execute once and do
local computation only; the application decides whether a later conflict merits a
new attempt. Adapters do not silently retry callbacks.

The cs-mail in-memory implementations execute the same callbacks under their shared
mutex. Multi-record management, security and billing changes use staged state and
publish it only after all ownership and persistence checks succeed.

This design deliberately retains transaction semantics in the port. It does not
replace one atomic operation with unrelated repository `load` and `save` calls.

## PostgreSQL guarantees retained

- The existing cs-mail receipt-order barrier orders authorization with key revocation,
  trust rotation, account changes and financial writes. Billing operations retain the
  requirement that earlier received commands have been processed.
- Key, persona and financial-account ownership remain unique, and each billing
  account owns its funding sources. Pending enrollment reservations also prevent
  another account from acquiring their persona, key, billing account or member. A
  bank account is shared between accounts only through approved enrollment review;
  rebinding to a bank account another account enrolled with is refused. Each capture
  records the funding source it was reserved on. Constraints and protected lookups
  cover previously absent rows.
- Account changes and command outcomes commit together. Identity security effects,
  the accepted event and the new cursor commit together. Billing state, funding state,
  ledger postings, evidence receipts and payment work commit together.
- Identity enrollment locks the verified login and enrollment operation, then protects
  the subject row before resolving a product reference. Changes lock involved logins
  in a stable order. Subject-row protection also covers reference creation through
  different logins for the same subject.
- Identity confirmation takes the same enrollment-operation lock as renewal, preventing
  confirmation of an older attempt from racing a new attempt.
- Workflow sequence allocation and all encrypted workflow rows remain one atomic
  append. Audit acknowledgements represent completed durable writes.
- Worker leases and generation checks remain atomic database operations. Confirmation
  failure recording now samples fresh trusted time after locking the claim, just as
  completion does; a batch-start timestamp cannot extend an expired claim. Retry
  scheduling policy is defined in the application.

The ports specify these guarantees. SQL, row codecs, driver errors, constraints and
lock implementations remain adapter-owned. Existing database privilege restrictions
are retained. No triggers or stored procedures were introduced to duplicate policy.

## Authorization time and external effects

Account and billing decisions sample the trusted clock after the adapter has loaded
protected context. That is the authorization instant, and new command/enrollment
receipts retain it. Already committed enrollment replay creates no new authority.
Payment worker entry points now accept a clock, allowing authorization and evidence
recording to sample time separately around provider I/O.

Identity verification may perform external OIDC I/O before entering the transaction.
The application rechecks intent and evidence freshness after the protected context
has been loaded, then signs locally. A signed result is not returned before commit.
No database lock spans an external provider request.

Disclosure requires `AuthorizedDisclosure`: principal, subject, exact fact IDs,
purpose, consent, policy and fresh evidence are bound into its short-lived permit.
The former PostgreSQL façade and its generic policy-evaluation/default-audit replay
methods are removed. The application records audit events and rechecks the permit
immediately before key resolution and again before decryption, including after the
intervening audit awaits. Onboarding continues to return its committed receipt facts
without decrypting historical identity data.

## Cutover and validation

### Native request/conversation cutover — planned boundary, 12 October 2026

The confirmed product rules are in [the domain model](../DOMAIN_MODEL.md#native-requests-and-replies--agreed-next-implementation-12-october-2026).
This section specifies the next implementation's boundary, not a claim that the
current native request reader already implements it.

The first implemented boundary is the paired request decision in
`cs-mail-application::requests`. It coordinates a forward kernel transition with
an optional opposite cancellation from `cs-mail-protocol::supersede_preparation`.
The opaque opposite effect identifies its exact relationship, expected revisions,
financial batch, schedules, work and causal position. The storage adapter loads
the complete opposite snapshot, protects its absence with receipt-order locking,
and persists the declared effect after ownership/revision checks. It makes no
cancellation decision and invokes no extra kernel transition during persistence.
Requests retain the existing canonical receipt-time interpretation; an issued
decision deadline, rather than an expiry-worker flag, determines whether an
opposite request remains actionable at that receipt.

Migration 0033 makes the journal key `(position, aggregate_key)`. Both direction
records use the initiating command's causal position and commit together. There
is no separately allocated opposite event that could overtake a queued receipt.
The migration refuses populated older environments. Native document publication,
request-answer authority and automatic reverse lanes are still planned below.

The request application use case must receive the two directed relationships,
their revisions and lanes, affected requests and financial state, shared request
histories, account/key authority, issued terms and the relevant conversation
records as one protected context. The existing receipt-order protection covers
both directions and absent records; taking independent snapshots and composing
their results later is insufficient. Trusted time is sampled after that context
is protected. No external network or custody-secret lookup runs under the lock.

| Operation | Domain/application-declared consequence |
| --- | --- |
| Prepare a native request | Refuse an actionable opposite request before allocating payment work; otherwise preserve one operation and its frozen reply agreement |
| Submit | Recheck both directions, funding and the opposite decision deadline; cancel opposite preparation with its complete financial consequences; publish the initial restricted conversation/document and both owners' endpoint/recovery copies with request submission |
| Follow up | Enforce the existing request windows and follow-up policy; append a document to that request's conversation without another charge |
| Answer | Establish current conversation-confined answer authority from submitted-request provenance and the sender-owned direction; commit the document and mailbox copies without a request decision or settlement |
| Accept | Coordinate forward permission and request settlement with reverse permission derived from the signed agreement; preserve existing standing/custom access without refilling allowance; commit all effects together |
| Revoke or block | Terminate current routes in that direction, including request-answer authority; retain historical evidence, deliveries and terminal settlement |
| Reject, expire or cancel | Apply the request's existing financial rules and preserve delivered copies; do not turn retained content into permission or resurrect an opposite cancelled preparation |

Request settlement, lane lifecycle, directional permission and conversation reuse
remain domain-owned. The application returns one opaque decision containing
their effects, expected revisions, explicit mailbox deliveries, audit provenance,
ledger effects and recoverable payment work. The adapter commits it without
changing any declared result. In particular, it cannot issue an extra reverse
grant after acceptance or invoke a second protocol transition during persistence.
Original sender agreement is evidence for automatic reverse permission; the host
must not create a signature on the sender's behalf.

Pending preparation, submitted-request provenance and current send authority need
distinct checked types. Restored request/conversation links must bind the exact
pair, request and initial document. Already delivered content survives financial
termination. Both-party reuse relaxation remains conversation-owned, including
restrictions inherited through known source edges.

The client journals the exact signed operation and encrypted payload before I/O.
Uncertain outcomes retain that operation across restart. Exact replay returns the
original committed result without fresh delivery, allowance or financial effects.
Neither a replay receipt nor cached request status authorizes a new send.
Transport JSON field order is separate from the defined signing representation;
normal object-field reordering cannot change a typed command's meaning.

The existing content custody contract requires four owner-bound ciphertext copies
per native document: two endpoint and two recovery copies, committed together.
The public custody key must be configured and published by the host, while its
private key remains behind the custody port. Recovery decryption still requires
entitlement and scoped consent independently of send/answer permission.

### Earlier account/billing cutover

Callers use the application services directly. The storage methods
`execute_account_command`, `apply_identity_security_event`, `execute_billing_command`,
`authorize_utility_dispatch`, `confirm_utility_payment`, `reverify_funding_source`,
`prepare_due_distribution`, `begin_account_enrollment` and `commit_account_enrollment`
are removed. The concrete account-service forwarding wrapper is also removed.
`SqlxPostgresEncryptionAwareWorkflowRepository` and its PostgreSQL-specific workflow
error types are replaced by a generic application service and generic errors.

The enrollment migration now belongs to `identity-storage-postgres`, using its
separate shared-enrollment migration registry. The adapter continues to own identity
migrations 0002–0005 and invokes FEN migrations through the FEN storage package.
Existing schema numbers and wire formats did not change in this follow-up. The earlier
fresh-database and coordinated-release requirements still apply.

No new test cases were added. Existing callers and tests were adapted to the new
owners. The existing PostgreSQL workflow replay case now obtains a valid disclosure
permit instead of supplying a fabricated `Allowed` result. CI includes the identity
application in its strict Clippy gate and checks it independently of host crates.
The sibling source manifest includes the new application crate and moved SQL.

The existing suites pass: 139 cs-mail cases with PostgreSQL and no ignored cases;
204 identity harness cases with PostgreSQL required. Optional real-provider cases
still return early without local credentials. These suites do not constitute new,
dedicated coverage of every security-event, management or disclosure branch.

This cutover does not claim to reorganize every older protocol-ingress or financial-
program adapter, or complete product authentication, consent ownership, witness
collection and deployment integration. Trusted operator configuration and maintenance
APIs remain infrastructure entry points. The existing global receipt serialization
and full billing snapshots remain explicit implementation constraints. Broader identity
core API/constructor cleanup and real-provider acceptance remain separate work.

## External outcome and restoration boundary — 13 October 2026

An authenticated external mutation declares either conversation/delivery changes
plus its receipt, or a receipt-only semantic refusal. Storage rolls back preparatory
writes for a refusal and commits the declared result under the receipt-order lock.
A first invitation may be refused without creating a conversation. Missing facts,
unresolved policy, failed authentication and storage faults do not establish a
terminal command result. Exact replay retains the historical receipt and still
requires current caller authority.

The host signs the recorded external acknowledgement; the client verifies its
provider signature and exact command digest/key/signature before resolving its
journal. Transport status alone cannot resolve it. Signature creation after commit
is recoverable: replay signs the same recorded result without new work. An empty
visible page is distinct from exhausted cursor traversal. Native and external
permission presentation share explicit-time interpretation of matching lane facts.

ProtocolState owns the single unresolved-request and request/pair consistency
checks. Its checked deserialization and protected PostgreSQL restoration use that
same rule. Invalid stored state or decoding failure stops processing without a
permanent refusal; repaired records permit recovery of the original received work.
This bounded repair does not move all older transactional orchestration or claim
that every storage-owned domain call is itself a behavioral defect.

## Alias foundation — 14 October 2026

Rust separates address lifecycle from independently revisioned receiving permission.
Private creation grants no lane. Explicit signed actions open, restrict or remove
an alias-scoped lane, or atomically replace access with selected signed sender
grants. The existing alias transaction commits permission and its exact-operation
receipt together. Native and SMTP admission recheck current permission; domain
matching uses authenticated evidence while correspondence and blocks retain the
actual sender identity. Swift groups lanes by permission and exposes pairing
controls. The schema-41 cutover preserves original evidence and the owner account
in an offline checked copy, with no runtime predecessor reader.

`AliasId` identifies a receiving route independently of `ProtocolIdentity`.
The sender capability types are now `SenderScopedGrant` and `SenderScopedLane`;
the old Rust names are removed without aliases, while their signed/storage field
representations remain unchanged. `AliasScopedGrant` has no sender, charge or
sender-lane renewal interval. `ReceivingScope` contains explicitly selected alias
IDs; `ExpressLanePermission` distinguishes the two forms without sharing their
lifecycle operations.

`cs-mail-accounts::aliases::Alias` owns route lifecycle and checked restoration.
Restrict freezes exact admitted conversations. A signed conversation approval adds
one named sender/conversation authority separately from that historical snapshot.
Replace creates a new route and keeps the old route's confined permissions through
an explicit transition interval; it does not copy scopes into the new route.
Retire preserves the address reservation. Participant blocks remain relationship
facts and override every alias permission; alias operations do not mutate accounts,
credentials, consent or financial records.

`cs-mail-application::aliases::AliasService` authenticates the original enrolled
participant's recipient key against protected account authority. Additional legacy
personas are not a route-creation mechanism. One decision declares aliases,
reviewed receiving-scope revisions and an exact operation receipt. PostgreSQL
serializes these through the existing receipt-order barrier and commits state,
original signed command and receipt atomically. Alias IDs and addresses remain
unique, including retired rows; Directory publication respects the same protected
reservation. No alias management command invokes request
settlement. The host signs the committed acknowledgement; the portable client
verifies command/result binding before resolving its existing encrypted journal.

Migration 0038 adds alias metadata without rewriting any existing account,
identity, key, protocol, financial or encrypted-copy representation. Owner-only
inspection does not publish aliases or associate them through Directory. The
delivery-evidence table is reserved for the coordinated delivery integration;
management cannot turn an authorization or a held item into admitted history.

This is a foundation pass, not the native/SMTP alias cutover. The deterministic
receiving decision returns a non-deserializable `ReceivingAuthorization`, Hold,
Request or refusal, never a delivery receipt. Transport integration must authenticate
the exact receiving alias and commit the complete message/ciphertext, admission
basis, allowance changes, route evidence and receipt together. Public per-alias
profiles/classes, default receiving scope on request acceptance, sender/reply alias
selection and held-mail persistence/release are not
implemented by this pass. The subsequent native management slice below exposes
address metadata and lifecycle operations without claiming mail delivery. Neither existing sender-lane tests nor a management
receipt establish those behaviors.

Validation extends the existing domain snapshot, directory/PostgreSQL replay and
portable client recovery experiments without adding test cases. The claims concern
conversation confinement, explicit approval, cross-route block precedence, address
reservation, replacement rollback, competing claims, exact replay, unchanged
canonical account/financial tables and original encrypted retry/credential bytes.
Concurrent alias delivery/blocking, actual power loss, public-provider delivery and
iOS remain outside these experiments.


## Native address management — 14 October 2026

The portable member client projects checked alias records into `ManagedAddress`,
`AddressState`, available lifecycle actions, retained conversation facts and an
observation time. These are presentation projections, not current authority.
`AddressMutation` parses inputs, creates checked random alias identifiers and
uses the original signed, encrypted operation journal. `PendingAddressAction`
identifies the original intent separately from an editable draft. A typed
`AddressUpdateResult` distinguishes a verified refusal from transport uncertainty.
UniFFI translates these owned values without deciding permission or lifecycle.

Swift's My addresses workspace lives under Relationships. Its list and detail
render Rust projections; confirmations capture the selected revision and user
intent. Rust and the protected backend recheck every command. Reload failure
retains a visibly stale snapshot and disables new updates. Resume sends the
original signed operation; closing a view or locking does not cancel that work.
Session generations prevent late results from revealing content after Lock.
Creation, restriction, explicit replacement transition and retirement are supported.
Private records have no publication or paid-request controls. The existing primary
address/publication remains intact until its separate alias-aware publication
cutover. No new account, credential or disclosure permission is introduced.

The workspace explicitly states that additional addresses cannot yet send or
receive mail. Receiving-route badges, sender-scope acceptance controls, alias-aware
publications, outbound alias selection and held-message release still depend on
those missing contracts and integration. They are not inferred in Swift.

Validation extends the existing client recovery experiment through the native
adapter API and the existing native enrollment journey against an isolated real
backend. It challenges signed-byte retention through response loss, altered
acknowledgement and local save failure; typed terminal refusal; canonical lifecycle
projections; Directory isolation; and original account/credential preservation.
No new automated test cases were added. The owner cutover uses the offline
`alias_metadata_upgrade.py` helper to restore a fresh copy, add schema 38, and
verify every original table/sequence fingerprint, migration history and configured
key file. The original source is retained. See the Apple runbook for installation
and interactive verification status.

## Native express lane groups — 14 October 2026

`AddressWorkspace.alias_lanes` contains broad/restricted `ReceivingLaneView` values;
`sender_alias_lanes` contains sender-restricted receiving permissions.
The portable Rust client classifies the owner-authenticated checked records by
their explicit receiving permission (`Alias::permission`), then projects lifecycle with the injected
observation time. Private addresses without lane authority are excluded; retired and replaced routes
retain explicit closed/transition state. UniFFI translates this projection without
reclassifying it. These snapshots do not establish current delivery authority.

Swift uses separate alias-lane and sender-lane selection cases. Only sender
selection opens sender correspondence; alias selection opens lifecycle details
and the existing address manager. Incoming/Outgoing filters only sender lanes.
Alias metadata does not acquire sender-lane expiry, renewal or rate semantics.
The existing native enrollment experiment now challenges public-address exclusion
and preservation of closed alias state; no automated test cases were added.
This presentation slice does not implement alias mail delivery.
