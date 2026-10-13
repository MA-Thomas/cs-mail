# Preserved development account conversion

This is a standalone maintenance tool for Marcus's preserved October 11 local
fixture snapshot. It is not a production migration, a client fallback reader, or
a reset command. It refuses any database URL other than the isolated
`cs_mail_continuity_20261012` candidate (or the named rollback-only test copy), an unsupported predecessor (conversion starts at 32; reply repair at 34 or 35),
unresolved commands, missing original keys, or unexpected fixture counts.

The original account and identity databases, signed application, Keychain item,
and encrypted SQLite store remain the rollback checkpoint. A candidate restored
from the account backup retains account/persona IDs, registered keys, address,
contacts, original decisions and financial history. Message IDs, request IDs,
admission times and plaintext remain unchanged. Original request terms explicitly
record `RequestReplyAgreement::NotOffered`: the original conversion incorrectly withheld conversation replies. The current
repair reconstructs every received conversation from original delivery evidence.
This permits replies without granting broader permission or reverse lanes. New
requests still require agreed terms. A new signed operator policy
version supplies terms for future requests; the old policy remains recorded.

The tool verifies the original recipient signature before reconstructing the
existing express lane through `VerifiedLaneGrant::activate`. Every previous lane
field must match exactly. `grant.declaration_authority = NativeSender` is retained;
the current `LaneAuthority::RecipientGrant` identifies who granted permission and
does not replace the message-origin constraint. Lanes with prior usage or other
state that cannot be reproduced exactly are refused rather than reset.

Old signed quotes are frozen in `cs_development_continuity_archive` and become
runtime replay tombstones. Original request and lane records are archived before
their representation changes. Runtime code only reads current records. Original
fixture plaintext is decrypted and checked in memory, then composed through the
current document/encryption API into participant and custody copies. PostgreSQL
commits the conversion in one transaction. The encrypted candidate cache is
validated by the current member client before the candidate is opened.

`keychain-input.swift` only reads the existing
`com.csqd.member.development/member-device-v1` credential. It passes it through an
anonymous pipe, never a file, and checks the credential bytes are unchanged after
the tool returns. There are no credential creation, update or deletion calls.
Build and sign the helper with the existing development identity and signing
identifier `com.csqd.member.dev`; Keychain access must remain fail-closed.

The helper takes the tool executable followed by database URL, original state
directory, output ciphertext path, mode, and custody-key path. Modes are
`rollback`, `convert`, `verify`, `lane-check`, `lane-convert`, `repair-replies`,
and `verify-replies`. The rollback mode
injects a database failure after preparing all fixture conversions. It must leave
schema 32, all original records, and no committed candidate cache. A durable encrypted prepared artifact records
commit uncertainty; reconciliation checks the protected schema before recovery. Verification
uses original device signatures to synchronize and decrypt all four original
contact conversations through the current service.

Observed checks are recorded in `target/account-continuity-20261012`: unchanged
account, directory, billing, ledger, endpoint-key and financial/history records;
unchanged original permission state; four exact plaintext comparisons in memory;
original Keychain and cache preservation; and injected transaction rollback.
The full Rust/PostgreSQL quality gate and standalone tool clippy passed. The
updated Swift app sent a pending-request answer which Maya's original fixture
endpoint decrypted. Device cache, fixture send journal and generated logs were
checked for that test plaintext. An injected loss of commit acknowledgement retained and reconciled the encrypted
cache artifact. Actual power loss, iOS, and replacement-device recovery were not
exercised.

Current app: `target/apple/CsMail-continuity-20261012.app`. It uses the original
credential service and signing identity, with its cache separately at
`target/apple-continuity-state-20261012/device-v3`.
The previous `device` store is retained unchanged. The explicit maintenance tool
reads v2 once and writes v3; the runtime has one v3 reader. Service config and restart script
are in that state directory. Do not start the old app against the current service
or switch the current app to the old database. Rollback pairs the original signed
app/cache with the original database and matching earlier service binary. Never
overwrite the checkpoint to make the current binary start.

The October 13 original-account repair restored five received-conversation records.
A fresh pre-upgrade backup compared unchanged for account, persona, endpoint-key,
request, request-financial, billing and ledger records; the original v2 cache hash
matched its checkpoint. The read-only helper verified unchanged Keychain bytes.
The updated app has the same designated signing requirement. All four original
fictional peers decrypted exact verification replies from the original account.
`verify-replies` holds a session advisory lock and retains its exact send operations
in a separate durable encrypted check cache. Runtime has no old-cache fallback.

## Accepted development samples

`align-acceptances` is the explicit correction for the original accepted samples.
It retains a durable prepared file containing fresh signed fixture consent and
stable operation identities, never private keys. The feature-gated Rust
application/storage maintenance contract protects original acceptance evidence,
current policy and both permission directions. It commits ordinary lane effects,
the received outcome and correction provenance atomically. Historical terms are
not changed; validity ends at original acceptance plus the current policy lifetime.
The original fictional recipient keys sign the new grant. No signature or consent
is attributed to the old formation record.

Later withdrawals, current stronger permission, later explicit grants, expired
acceptance windows and unresolved reverse requests prevent inappropriate repair.
`check-alignment-withdrawal` is restricted to its named rehearsal database and
challenges later signed block/unblock plus exact and altered retries. The lost
acknowledgement check applied Daniel once and replayed him before applying Elena.
The original environment now has both corrections, with unchanged identity, key,
request and financial records. Prepared files and backups are retained under
`target/acceptance-alignment-20261013`. The runtime needs no historical reader.
