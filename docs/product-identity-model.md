# Product login identities and the shared subject

Canonical product model for cs-mail and Phoros, clarified 2026-09-20. Apply the
[Rust-domain principles](rust-domain-principles.md) when implementing it.

## Meaning and ownership

A person represented in both products has one shared `SubjectId`, one cs-mail
account with one durable cs-mail login identity, and one Phoros account with one
durable Phoros login identity. A subject identifier is a record identity, not a
claim of biological identity or a particular assurance level.

The identity service owns subject resolution and product login bindings. Product
accounts own their permissions and operational keys. Each product login can have
multiple authentication methods. Adding, removing or recovering a method must
preserve the login identity, product account and subject association.

`ProductLoginIdentity` represents the stable `(product, SubjectId)` identity and
its configured external provider binding. The current OIDC integration delegates
login authentication methods and login recovery to that provider, which must
preserve its issuer/subject pair. Session IDs, email addresses, passkeys and device
keys are not additional login identities. Replacing the external provider subject
is not an implemented recovery mechanism.

Product-scoped opaque references can differ while resolving to the same internal
`SubjectId`. Products do not need to exchange the global identifier. Shared subject
identity does not grant cross-product access or make one product's evidence satisfy
another product's enrollment policy.

## Enrollment

The implemented enrollment service accepts only the cs-mail bank-ownership policy.
It resolves a verified login within the configured product. If that product login
does not exist, it creates a subject, product reference and durable login binding
atomically with its enrollment reservation. It never discovers a shared subject
merely by matching a login in another product, an email address or a bank account.

Phoros enrollment remains deferred. Its ceremony must establish the required
biological identity evidence and biological continuity references. If the person
already has a cs-mail account, that ceremony must also establish control of that
account, with the proof bound to the Phoros enrollment. The identity service then
resolves and adopts the existing cs-mail `SubjectId`, establishing a separate Phoros
product login and account. It does not add a second cs-mail login or merge two
already-established subjects. Detailed ceremony and proof contracts are not yet
implemented; the existing cs-mail service must continue rejecting Phoros enrollment.

The mobile/FEN workflow host is development evidence-recording infrastructure. Its
caller-supplied subject IDs do not authorize product enrollment or establish the
association with a cs-mail account. Those handlers are not Phoros enrollment APIs.
A future production ceremony must receive its subject from the identity
application's protected, authorized resolution before attaching its facts.

## Recovery and authentication methods

The current `RecoverDevice` ceremony replaces product operational keys while
requiring the existing product login and fresh bank ownership. It preserves the
account, product login and subject. It is distinct from recovering access to the
OIDC login itself. Existing additional-key registration likewise changes operational
authority, not the product login binding.

The configured OIDC provider owns its authentication-method lifecycle and recovery
to the same external subject. A deployment must supply that recovery process; this
repository does not implement a fallback that creates a new login or subject when
authentication methods are lost. Recovery involving additional review remains
separate work. Product security changes remain product-scoped.

## Persistence and cutover

The shared identity schema stores product logins with a primary key on
`(product, subject_id)` and unique `(product, issuer, login_subject)` ownership.
Enrollment foreign keys require the product reference and product login to identify
the same subject. Locks include the product, including protection for absent rows.
Application decisions check the current product login under that protection.
Decision functions receive explicit time and candidate identifiers; orchestration
allocates identifiers before locking and samples trusted time after protected reads.

The second-login operation and its request field are removed, without compatibility
aliases or fallback readers. Shared-enrollment schema version 2 is a fresh baseline;
version 1 databases are rejected and require a fresh development schema. Existing
cs-mail schemas also follow the previously agreed fresh-database release policy.
Both repositories and the sibling source manifest must be released together.

Validation uses the existing suites, with no new test cases. Compiler guarantees,
database constraints, design review and operational checks provide complementary
evidence; passing suites do not establish the deferred Phoros ceremony or provider
recovery behavior.

Local verification completed 139 cs-mail cases (including PostgreSQL) and 204
identity-model all-features harness cases with PostgreSQL required. Optional live
provider cases had no credentials and did not exercise real providers. Formatting,
the established strict Clippy gates and all sibling checksums passed. Direct database
checks accepted two product bindings to one subject, rejected duplicate product
logins and mismatched subject references, and confirmed that the executable refuses
schema version 1. A parallel cs-mail run exposed the existing timestamp-based schema
name collision in the worker test fixture; the complete serial run passed. No new
test cases were added.
