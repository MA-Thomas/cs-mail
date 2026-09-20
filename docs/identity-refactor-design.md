> The current breaking update and validation scope are documented in [Rust domain update](rust-domain-update.md). Earlier stage descriptions below are historical.

# Identity refactor decisions

This is a breaking development update. Databases are disposable: create fresh schemas from the checked-in SQL. There is no compatibility layer, backfill, or data-conversion path.

## Ownership and boundaries

| Component | Responsibility |
| --- | --- |
| `identity-contract` | Signed DTOs, signature/context verification, opaque product subject references, sanitized wire errors |
| `identity-enrollment` | Authentication and bank-evidence policy, global SubjectId resolution, durable product bindings and attempt outcomes |
| `cs-mail-accounts` | Account IDs, principal ownership, billing association, binding and input invariants |
| `cs-mail-application::accounts` | Intent construction, shared activation rule, enrollment orchestration, renewal, confirmation delivery |
| `PostgresAccountRepository` | Account transactions independent of any relationship; pinned trust; unique local reservations; atomic activation and outbox |
| `MemoryAccountRepository` | The same application port and activation rule, with pinned trust and atomic in-memory persistence |
| `AuthoritySnapshot` | Validated account/persona/key ownership and receipt-time authorization |
| Development support crates | Synthetic evidence and scenario builders; never production dependencies |

The domain constructor creates domain state; it does not grant persistence authority. Both repositories accept signed decisions, load their own pending operation and trusted verifier, and invoke the same activation function with the backend's activation timestamp. A cached `VerifiedDecision` cannot bypass this boundary. Wire serialization and challenge generation belong to the application integration module, not the account domain.

## Enrollment transitions

The operation names a durable enrollment. Its authentication attempt is the expiring challenge inside that operation. Account, principal, bank input, persona, initial key and enrollment digest remain fixed across attempts.

| Current state | Action | Result |
| --- | --- | --- |
| No local operation | Begin with validated bank input | Reserve unique local ownership and generate account/principal/challenge |
| Pending operation | Repeat identical begin | Return the existing operation |
| Pending operation | Begin with altered input | Conflict |
| Live attempt | Renew | Conflict; do not replace evidence already in flight |
| Expired, uncommitted attempt | Renew | Fresh challenge and times; retain all ownership IDs and inputs |
| Authenticated attempt | Bank says denied/review | Durable attempt outcome; no account activation |
| Eligible attempt | Commit before expiry | Atomically create account, billing association, persona, key registry and confirmation item |
| Active operation | Replay exact committed decision, including after expiry | Return the existing account |
| Active operation | Renew or commit another decision | Conflict |

The identity service retains prior attempt results for exact retries and keeps the product binding separate. Renewing eligibility requires fresh OIDC, bank and device evidence, the same enrollment context and the same reserved subject reference. An expired reservation is never silently reassigned. A reviewed or denied attempt can be followed by fresh evidence after expiry; this is not a product-side override of the bank decision. Bank replacement and reassignment are separate, unimplemented ceremonies.

Local persona, key, billing, member and funding-token conflicts are reserved before contacting the shared service. Database constraints arbitrate competing operations. No local transaction spans an identity-service call.

## Authority and receipt ordering

Account repositories do not carry a relationship aggregate key. New account/key transactions serialize with receipt creation using its short database lock; they do not require queued commands to drain. Accepted receipts already contain immutable authority snapshots, so later revocation cannot retroactively invalidate them. New receipts see the changed authority.

Snapshot constructors and deserialization reject foreign persona keys, duplicate persona ownership, duplicate account entries and ambiguous key ownership. All lookup methods use the same ownership mapping. Relationship registries hold provider/scheduler authority only.

## Runtime and failure handling

The identity host uses asynchronous PostgreSQL. The executable owns and supervises its connection task; service destruction performs ordinary cleanup. Synchronous OIDC verification runs on the blocking pool without database locks. HTTP capacity is retained by the processing task if its caller disconnects. Expiry is checked again after verification and database waits.

One connection per service instance serializes its short database transactions. This is an explicit small-service choice, not a throughput claim; a pool can replace connection ownership when measured load warrants it. PostgreSQL constraints and advisory locks coordinate separate instances.

Internal database and client errors preserve their source errors. Only protocol error codes cross the HTTP response boundary. Confirmation batches retain per-item failure causes for the host, continue after delivery failures, delay transient failures by 30 seconds, and quarantine rejected items for intervention. The repository exposes an explicit retry after the underlying issue is resolved. Confirmation never changes account ownership or the recorded decision.

## Focused verification

One enrollment conformance scenario runs against memory and PostgreSQL: conflicting local ownership, expired authority, stable renewal IDs, wrong issuer, rejection of old attempts, exact committed replay and subject uniqueness. Additional integration checks cover real transaction rollback, concurrent retries, HTTP/lost responses, durable review outcomes and queued receipt authorization after revocation. A small confirmation test checks that a rejected item cannot starve a healthy one.

Removed checks asserted obsolete API names, demo golden text, synthetic onboarding outputs or source-file placement. Existing financial, authentication and protocol behavior tests remain: changing architectural boundaries is not a reason to discard independent behavioral evidence. Passing fixture tests does not establish live OIDC or bank-provider integration.
