> The current breaking update and validation scope are documented in [Rust domain update](rust-domain-update.md). Earlier stage descriptions below are historical.

# Shared identity integration: stages 1–4

The normative version 1 protocol is owned by identity-model:
[shared identity contract](../../identity-model/docs/shared-identity-contract.md).
The [product identity model](product-identity-model.md) is the current source for
login ownership, authentication methods and future Phoros subject adoption.
Both repositories consume its small `identity-contract` crate; cs-mail does not
import the identity domain implementation in production. Test-only dependencies
exercise both implementations against separate PostgreSQL schemas and over HTTP.

## Ownership and code

| Owner | Implementation |
| --- | --- |
| SubjectId, login mapping, product reference, binding reservation | identity-model/identity-enrollment |
| Signed request/decision protocol and private verified capability | identity-model/identity-contract |
| AccountId, PrincipalRef, validated account construction | crates/cs-mail-accounts |
| HTTP transport with time/size limits | crates/cs-mail-identity-client |
| Network orchestration and reconciliation | crates/cs-mail-application/src/accounts.rs |
| Enrollment transaction and confirmation outbox | crates/cs-mail-storage-postgres/src/accounts.rs |
| Background confirmation delivery | crates/cs-mail-worker::run_identity_confirmation_batch |
| Account-owned keys, receipt-time composition | cs-mail-security and PostgreSQL ingress |
| Funding sources, coverage, membership, obligations | Existing billing and finance crates |

The account domain has no HTTP or PostgreSQL dependency. Wire DTOs remain separate
from the private Account and VerifiedDecision values. ProductSubjectRef is opaque
and cannot be mistaken for the service's global SubjectId. Account/principal IDs
are generated with system randomness and are independent of bank identifiers.

## Enrollment API

1. The backend configures its immutable identity issuer, `cs-mail/<environment>`
   audience and pinned decision public key with `configure_identity_service`.
   The financial bank-signing authority is configured separately.
2. `AccountEnrollment::begin` persists an operation, fresh account/principal IDs,
   immutable local input, and random challenge, reserving local persona/key/billing/member/funding ownership before remote authorization. Local persona/key identifiers in
   EnrollmentInput are backend allocations, not claims to arbitrary existing email
   addresses. The method is a trusted backend API, not a public JSON endpoint.
3. The client authenticates to OIDC using that challenge as nonce and requests
   fresh authentication. The ID token must contain issuer, audience, issuance and
   expiry timestamps, nonce and auth_time. The proposed device signs the intent.
4. The authenticated bank integration signs a short-lived SignedBankOwnership
   assertion tied to the same login, challenge, operation and exact financial
   BankVerification digest. No raw bank credentials or medical facts cross the
   service boundary. The adapter must establish ownership before signing.
5. `enroll_with_identity` calls `/v1/enrollment` using a product-signed request and
   commits an eligible decision using the current backend clock. No local lock is
   held during remote I/O. Decisions cannot select local IDs or replace bank input.
6. A host worker periodically invokes `run_identity_confirmation_batch` with a bounded
   batch size and current timestamp. Its structured report distinguishes confirmations, retryable failures and interventions. Delivery is at least once; the service confirmation is idempotent.
   This worker must run independently of the original request's lifetime.

The application `AccountEnrollment` uses atomic reservation/activation ports implemented by PostgreSQL and memory. The host owns its product
signing secret and passes it through trusted configuration, never user request
fields. No signing secret or bearer token is persisted in enrollment tables.
The application-layer `reconcile_account_enrollment` uses a signed lookup after a lost response, keeping
all original IDs. Exact committed retries succeed after expiry. An expired, uncommitted attempt can be renewed through `AccountEnrollment::renew`. Renewal retains account/principal IDs and input, and requires fresh challenge-bound evidence. It never releases or replaces the identity/account binding.

## Account ownership and authority

This is an idea-stage application with disposable development databases. Initialize
fresh databases from the current schema; there is no upgrade or data-conversion path
for earlier development schemas.

Every product account has a required principal and shared identity binding. Account
creation uses verified enrollment in PostgreSQL and the in-memory domain adapter.
Pending enrollment is a separate state. There is no unbound account mode or billing-only
provisioning API. `cs_persona_owners` is the canonical persona ownership table, and
financial membership digests derive from the independent principal.

The pre-existing rule restricting a funding token to one account is retained as
an anti-abuse rule. Supporting shared/joint funding sources needs a separate rule.

User personas resolve only to account registries. Missing ownership and missing or
revoked keys reject authorization. Relationship registries accept provider/scheduler
keys only. Account-key revocation requires an explicit AccountId. Account registries keep their own transparency logs. Receipt snapshots retain
separate source registries, so revocation does not rewrite previously accepted
commands. Provider/scheduler authority remains local to the provider relationship.
Additional operational keys and explicit device-key recovery preserve the product
login. OIDC authentication methods and recovery to the same external subject are
provider-owned. Phoros subject adoption and subject corrections remain deferred.

## Builds, checks, and release order

For joint development, check out these repositories as siblings:

```
Documents/cs-mail
Documents/identity-model
```

The production dependencies are the narrow contract crate; the service/model crates
are development dependencies for the joint integration tests. Before merging or
publishing cs-mail, publish the corresponding identity-model changes and set the
GitHub repository variable `IDENTITY_MODEL_REF` to that reviewed full commit SHA.
CI checks out that exact revision next to cs-mail. The previously published commit
a6de0a8 does not contain the new crates. A standalone packaged release must publish
identity-contract or replace sibling paths with an immutable reviewed Git revision.

Run local checks from cs-mail:

```
cargo +stable fmt -- --check
cargo +stable clippy --workspace --all-targets -- -D warnings
CS_MAIL_TEST_DATABASE_URL=postgresql://USER@127.0.0.1:PORT/postgres \
  cargo +stable test --workspace -- --include-ignored
```

The PostgreSQL tests create isolated schemas. The shared_identity suite covers
signed context rejection, mandatory account bindings, subject binding uniqueness,
concurrent retries, local rollback, restart and lost acknowledgement, HTTP transport,
account-wide revocation, and frozen receipt authority. Existing payment, refund,
distribution, retention and protocol tests remain part of the same regression run.

## Deliberate scope boundaries

This implements bank-backed cs-mail enrollment, device-key recovery, bank rebinding
and their service boundary.
It does not launch a hosted service, configure a live bank provider or OIDC tenant,
provide an account UI, or enable Phoros enrollment or cross-product subject adoption. The identity
service accepts only the cs-mail bank-ownership policy. Historical assurance and
fixture recovery outcomes cannot upgrade that policy. Its signed eligibility is a
bounded-time capability, not instantaneous global revocation. Product security events
have an ordered reconciliation path; cross-product broadcasts and additional recovery
review remain separate work.

See [refactor decisions](identity-refactor-design.md) for the transition table, repository boundaries, runtime ownership and focused verification policy.
