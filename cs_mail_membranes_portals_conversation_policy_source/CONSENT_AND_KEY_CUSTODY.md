# Recoverable message access with scoped consent

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](../docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Product decision and implemented first slice, 2026-09-21. This supersedes the
assumption that CSQD must be cryptographically incapable of decrypting ordinary
cs-mail. The requirement is that decryption requires scoped user consent. These
product choices do not amend the general Rust-domain principles.

## Distinct authorities

Authentication establishes control of the particular cs-mail account/persona.
Entitlement establishes which retained mailbox copies that persona may access.
Consent authorizes a named device or registered CSQD processor to decrypt an exact
set of those copies, for one stated purpose, until an explicit expiry. None of
these facts substitutes for another. Shared SubjectId and Phoros consent convey
no cs-mail decryption authority. Reading a delivered quote does not authorize its
onward reuse; the current conversation policy remains the owner of that decision.

`cs-mail-consent` owns `ContentScope`, `DecryptionGrant`, `GrantUse`,
`DecryptionActor` and `DecryptionPurpose`. Private grant/scope fields, checked
construction and checked restoration enforce structural invariants. Grants have
active/revoked recorded state; expiry is derived at trusted current time.

The first scope is a snapshot of 1–100 explicit message IDs belonging to one
account/persona, with no wildcard or automatic access to future messages. Maximum
grant lifetime is 24 hours. `Display` targets the authorizing device's signing key
and encryption public key. `CsqdProcessing { operation }` targets one registered
processor, one exact operation identifier and its registered encryption key.
An operational key is verified against the account's active key registry; it is
not a second product login identity. A new device receives account authority
through the existing account key registration or identity recovery process.

## Recoverable content and custody

The sender encrypts the explicit document into an endpoint copy and a recovery
copy for each mailbox owner. All four ciphertexts, the immutable manifest and
delivery receipt commit together. Recovery ciphertext is authenticated by the
sender's content key and encrypted to the configured custody public key. Public
sender-key evidence and bindings are included in the signed delivery command.
Clients obtain the custody public key over their authenticated host connection;
this is a trusted deployment directory, not an independently witnessed key log.

A recovery copy is not an endpoint private-key backup. Each copy contains only
that message's document. Authorized recovery decrypts it locally in the custody
adapter, checks it against the committed manifest, and re-encrypts it solely to
the destination in the consent grant. No mailbox-wide or custody private key is
returned. A one-message grant consequently cannot unlock unrelated messages.

`cs-mail-key-custody::LocalCustodian` imports a host-provisioned HPKE secret. The
same secret must be restored from protected host secret storage across restarts;
it is never written into the message database, audit records or API responses.
Migration 0017 stores only the public custody configuration, encrypted recovery
copies, processor registrations, grants and audit receipts. Different root bytes
cannot silently replace a configured root. Root rotation/rewrapping, physical
secret-store deployment and an HSM adapter remain deployment work. Losing the
host's root without its managed backup loses the recovery route; users do not
need a separately memorized or saved secret.

The extra recovery envelopes intentionally trade storage for a simple auditable
first implementation. A later per-message wrapped-key representation may reduce
that cost while preserving the same consent, retention and release semantics.
Previously delivered endpoint copies remain local disclosures: consent expiry
cannot make already disclosed plaintext or endpoint keys disappear.

## Key-use and commit boundary

`cs-mail-application::consent::ConsentService` accepts signed `Authorize`, `Revoke`,
`Inspect` and `Read` commands. Account authentication and consent are separate
checks. An account grant issuer must still have active persona authority at key
use; recovery revokes old signing keys and therefore invalidates their grants.
Processor registration is operator configuration, never a grant. Disabling a
processor blocks its future requests even while a user's grant remains recorded.

The PostgreSQL adapter acquires the existing canonical receipt lock and protected
account, grant and retained-copy state. It authenticates before private lookup,
then the application rechecks authority and trusted time after protected reads.
Consent, exact purpose/recipient, entitlement, content expiry and custody-key
identity must all hold before it constructs a nontransferable `AuthorizedDecryption`.
Only local cryptographic work runs under these locks. The resulting ciphertext is
not returned until its receipt commits. A committed revocation before this key-use
boundary prevents release; one that follows a committed disclosure cannot retract
it. Account recovery, key revocation and copy deletion use the same ordering gate.

This adapter does not call an external KMS under database locks. A future remote
custodian needs a protocol for redeeming operation-bound authorization and
rechecking freshness at its own key-use boundary; a previously issued permit
must not become permanent authority during network delays.

Every successful mutation has an account-scoped stable operation ID, canonical
request digest, signature, verifying-key evidence, actor, trusted authorization
time and outcome. Release outcomes point to the retained grant's immutable scope,
purpose and destination. Receipts never contain plaintext or released ciphertext.
An altered retry conflicts. An exact retry of a completed read returns
`AlreadyReleased` with historical grant/message identity and performs no new key
use. If the caller lost that response, it may request a new read with a new ID,
subject to current authority and consent. This explicitly distinguishes a known
committed disclosure from a response that the device may not have received.

A receipt-write/commit failure returns no encrypted result to the caller. Local
cryptographic work may have happened before rollback; the deployment threat model
trusts the custody process not to disclose that intermediate plaintext. Durable
receipts describe committed releases, not every transient cryptographic attempt.

## Recovery and deletion

The existing shared-identity `RecoverDevice` event verifies recovery evidence and
installs replacement account authority. It suspends the account under the current
account lifecycle; the recovered manager resumes service explicitly. This pass
reuses that lifecycle rather than introducing a second recovery authority. The
new device then issues its own display grant and reads retained copies through
custody. It never needs the lost device's content private key. Device-loss detection,
identity-ceremony UI and live identity-provider operations are separate work.

Recovery requires both the owner's original retained copy and its matching
recovery row. Owner-specific deletion cascades to that recovery row in the same
transaction. It does not delete another participant's copies or independently
retained quotations. Expired content cannot be recovered. Neither old grants nor
retry receipts can resurrect deleted/expired content. Native scheduled physical
expiry cleanup and control-record retention remain deployment work.

## Trust boundary and limits

The local custody adapter is enforced by application checks, protected transactions,
host secret custody and operational controls. It is not independently enforced
against an operator controlling the service code and root secret. Explicit user
consent is required by the implemented API; stronger protection against operator
bypass needs a separately enforced key-use boundary. This is an operational claim,
not a cryptographic claim that CSQD lacks a decryption capability.

A processing purpose is an auditable authorization to disclose selected content
to a configured processor, not proof of how the processor behaves after receipt.
Its implementation, retention and operational controls must honor that purpose.
No processor implementation, training pipeline or Phoros PHI workflow is added.

This is a headless library slice, with real PostgreSQL persistence and HPKE crypto.
It does not add HTTP hosting, OS device key vaults, device synchronization or an
independent consent authority. Network adapters must authenticate their host and
bound incoming bodies before decoding. The global receipt lock and local crypto
under that lock are explicit throughput limits, not a scalability claim.

New correspondence commands use signing domain v3 and require both recovery
copies. Document commitments remain v2. Migration 0017 is additive and does not
convert existing ciphertext into recoverable ciphertext. Older endpoint-only
messages remain available to devices holding their original keys; managed recovery
returns unavailable for them. There is no compatibility writer or automatic key
escrow for old messages and no database reset.

## Concrete host flow

1. Restore `LocalCustodian` from the host's protected secret source and call
   `PostgresAccountRepository::configure_custody(custodian.public_key())`.
2. Supply that authenticated public key to `NativeClient::prepare_correspondence`.
   The existing signed send commits all owner-specific copies atomically.
3. Authenticate/recover account authority through the existing account APIs.
4. Sign an `Authorize` command with a `ContentScope` of retained message IDs and a
   `GrantUse` binding the device or processor, exact purpose, output key and expiry.
5. Sign `Read` with that grant, message and purpose. The custody service returns a
   sealed `ReleasedCopy`; `NativeClient::open_released` checks the expected custody
   key, decrypts locally and verifies the document manifest. A processor can use
   the same endpoint cryptography with its registered destination key.
6. Sign `Revoke` to stop future use, or let the bounded grant expire.

## Consequential validation

Four PostgreSQL experiments challenge: replacement-device recovery without old
secrets, separation of display and processing authority, receipt-failure/retry
behavior, and revocation racing a release. They also exercise wrong scope/purpose,
wrong-account access, revoked old device keys, content deletion, grant expiry and
sealed-response destination binding. Existing correspondence experiments continue
to challenge atomic delivery, mixed selections and policy inheritance.

The recovery test uses a development identity issuer to produce a security event;
it still executes the production signature verification and account lifecycle.
It does not claim a live biological or identity-proofing ceremony.

```sh
CS_MAIL_TEST_DATABASE_URL=postgres://USER@127.0.0.1:PORT/DATABASE \
  cargo +stable test -p cs-mail-storage-postgres --test correspondence \
  -- --ignored --test-threads=1
```
