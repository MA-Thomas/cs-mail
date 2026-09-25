# Rust domain update — 2026-09-20

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

This change replaces the superseded interfaces and assumes fresh databases. It does
not provide legacy provisioning, deserialization defaults for replaced records, or
an upgrade migration for an existing deployment. Both sibling repositories must be
reviewed and released together.

The subsequent [application/persistence cutover](application-persistence-boundaries.md)
defines the current service entry points, callback contracts and removed PostgreSQL
interfaces.

## Ownership and transaction boundaries

`cs-mail-accounts` owns product account control: active/suspended/closed lifecycle,
personas, management grants and revision checks. Signed account commands bind the
product, account, signing key, expected revision and idempotency key. Registering a
new operational key additionally requires a possession signature bound to that
account and product. A messaging key receives management powers only through an
explicit grant; enrollment grants the initial manager deliberately.

Billing owns service contracts, payment instructions and obligations. Its old
account-close command and account-open status are removed. Product suspension or
closure stops new product use and service purchases; existing collections,
refunds, member allocations and distributions remain payable. A verified bank
rebind changes future payment instructions and retains the destinations embedded
in existing payment operations.

`PostgresAccountRepository` receives a trusted `AccountClock`. The application activation callback samples it after the receipt-order, enrollment and canonical key-claim locks. The accepted
instant is recorded as `authorized_at`. No timestamp supplied with a signed
identity decision is accepted as the product's authorization clock. The in-memory
enrollment adapter samples its clock under its state mutex.

`cs_key_claims` is the only namespace for operational-key ownership. Enrollment
creates a reservation and activation changes that exact reservation to an assigned
account key. Provider, scheduler and signed account key registration all use the
same table. Expiry renews evidence, never ownership.

Receipt-time authority excludes inactive product accounts. Already received
commands retain the authority captured at receipt. The global receipt-order
barrier is retained where existing receipt semantics require it; this change does
not claim to eliminate every cross-account serialization point.

## Identity domain, adapters and disclosure

The sibling workspace now separates:

- `identity-model`: facts, policies, projections and pure workflow rules;
- `identity-application`: enrollment, account-change and encrypted-disclosure orchestration;
- `identity-adapters`: OIDC/JWKS, Apple assertion crypto, continuity signatures,
  hosted provider implementation and durable plaintext encoding;
- `identity-storage-postgres`: identity SQL, row codecs and durable audit writes;
- `identity-server`: mobile HTTP and host composition;
- `identity-contract`: signed service messages and verified proofs;
- `identity-enrollment`: shared-service HTTP transport and host composition.

The core has no HTTP, SQL or runtime feature switches. Its root exports are explicit.
Serde derives apply to fact/value records, not the opaque verified-decision proof.
The server's encrypted payloads use a versioned, stateless JSON codec. The old
production-encryptor constructor that silently selected a process-local plaintext
cache is removed; codecs must be selected explicitly.

`IdentityHistory` reports recorded history. `AuthorizationSnapshot` requires an
explicit time and retains its source fact IDs. Future occurrence/import times are
excluded, authority expiry is half-open, and permission checks require a current
policy reference. A historical projection is not an authorization capability.

The synthetic delegation flow and its demonstration service methods are removed.
`evaluate_delegation` checks the actual actor, target, authority type, action set,
duration, versioned policy and proposal-bound witness assessments. Witness names
are policy data. Missing evidence, review, denial and approval remain distinct.
Revocation is a separate target-authorized operation. Approved grants can be
converted to the existing fact representation for append-only persistence.

`authorize_disclosure` binds a principal, subject, exact fact IDs, purpose, consent,
policy and fresh authentication evidence. Every raw disclosure requires high-assurance
continuity within five minutes and credential freshness within fifteen minutes,
including disclosures for account inspection. Its opaque permit has a bounded lifetime.
`disclose_identifiers` checks that permit and uses the durable audit path before key
access. Onboarding returns a receipt summary from its newly verified and committed
facts; it no longer decrypts a subject's complete history or manufactures an
unconditional Allowed policy from deployment configuration.

## Account changes and security reconciliation

`ChangeIntent` separates device recovery and bank rebinding. It binds
the existing account, product-scoped subject, expected security version, challenge,
proposed key/bank digest and change kind. The backend signs the whole request.
Every change requires fresh OIDC authentication, device possession and authenticated
bank ownership. Bank rebinding requires possession of the current key. Device
recovery instead proves the replacement key and preserves the existing bank,
product-login and subject bindings. OIDC authentication-method management and
recovery belong to the configured provider and preserve its external subject.

The [product identity alignment](product-identity-model.md) replaces global login
aliases with one durable product login per subject. The second-login operation
is removed. Phoros subject adoption is a distinct future enrollment decision,
requiring its ceremony and proof of control of an existing cs-mail account.
Shared-enrollment schema version 2 requires a fresh database.

Successful changes create signed, ordered security events. Tokens and login IDs
are not copied into product security notifications. The product requires contiguous
versions and exact issuer/product/account/subject context, records application
atomically, and suspends product use. Recovery revokes previous keys and installs
an explicitly granted replacement manager. Resumption is a separate signed account
control action; a closed account cannot be reopened. Bank rebinding requires the
product's verified financial evidence to match the event's digest.

`IdentitySecurityRepository` and `synchronize_identity_security` expose resumable
notification reconciliation. The host supplies authenticated bank-change evidence;
missing evidence leaves that event unapplied and the cursor unchanged.

Identity decisions and security events identify the signing key. Product trust is
an explicit ring with activation/retirement windows. `rotate_identity_trust` uses an
expected configuration revision and the receipt-order lock. Removing a key revokes
future authorization through it; previously committed outcomes remain history.
Product-request and bank-witness keys remain separate deployment trust roots.

## Delivery and deployment

Identity confirmation uses `FOR UPDATE SKIP LOCKED`, an expiring lease and an
increasing generation. Completion checks the current generation and lease; retry
or intervention releases the claim. A stale worker cannot acknowledge a newer
claim. No network request runs while a product transaction holds locks.

Each SQL package owns its migrations. The identity adapter invokes the shared FEN
store's migrations through that package and owns identity migrations 0002–0005
and the separate shared-enrollment migration registry.
The health-economic package owns its own migration. Duplicate SQL copies are removed.

CI checks the core and adapter crates separately. Its live identity job provisions
PostgreSQL and a local Keycloak realm and enables required-live modes: absent
configuration fails rather than being reported as a successful optional skip.
The runtime harness still uses explicit development liveness/continuity/App Attest
fixtures; Keycloak success is not evidence of real Apple or bank-provider acceptance.

cs-mail CI requires a reviewed, exact identity repository commit. The accompanying
source checksum manifest additionally binds the sibling source content reviewed
with this update. A new commit SHA must be chosen after these uncommitted changes
are reviewed; an old SHA cannot stand in for this source tree.

## Validation limits and remaining work

No new test cases were created. Existing tests were moved with their code, adapted
to the new APIs, or removed when they asserted the deleted synthetic delegation
behavior. Existing workspace tests, fresh PostgreSQL integration checks, formatting,
cs-mail strict Clippy, and the identity application/contract/service Clippy gates are used.

These changes supply library/application boundaries, not a finished enrollment or
recovery UI. Hosts still own session authentication, abuse controls, policy/consent
repositories, collection of actual witness evidence, and scheduling reconciliation.
No real bank, Apple or liveness-provider credentials are available in this checkout.
The new workflows therefore do not have new dedicated regression cases, by request.

The broader Phoros account, employee-role, communications, immutable-original and
desktop-sync products are outside this implementation. Identity's older generic
workflow APIs still contain long argument lists and test doubles; workspace-wide
strict Clippy is not yet the identity workspace's established gate. The core/service
separation does not turn those development adapters into production integrations.


## Local execution record

On the final domain implementation, the existing cs-mail workspace suite completed
139 cases with PostgreSQL enabled and no ignored cases. The identity workspace's
all-features suite completed 204 harness cases with PostgreSQL required. Optional
OIDC/runtime provider cases still returned without live credentials locally; that
204 total is not a claim of real-provider execution. The final host error-handling
adjustment was compiled across all targets and the existing shared-identity HTTP
integration cases were rerun.

Formatting, `git diff --check`, sibling source checksums, cs-mail strict workspace
Clippy and the established identity application/contract/service strict Clippy gate passed.
The complete identity workspace's optional strict Clippy run still reports older
long-constructor and health-economic lint debt; it is not reported as passed.
