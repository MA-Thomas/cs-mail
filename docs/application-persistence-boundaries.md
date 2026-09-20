# Application and persistence boundaries

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
- Key, persona, financial-account and funding ownership remain unique. Pending
  enrollment reservations also prevent another account from acquiring their persona
  or funding token. Constraints and protected lookups cover previously absent rows.
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
