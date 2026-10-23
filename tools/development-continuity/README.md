## Unified-alias preservation — 21 October 2026

`unified_aliases_upgrade.py`, `src/bin/unified-alias-device.rs` and
`unified-alias-input.swift` implement the explicit local schema-43 to schema-44
cutover. They are standalone maintenance tools, outside the app/server runtime.
The backend accepts only the guarded local source scenario, copies the stopped
source into a fresh database, archives original rows and signed evidence, rehearses
rollback and verifies unchanged unrelated facts. Alias policy and publication
mapping are supplied explicitly; historical public/private labels are not runtime
permission evidence. Only terminal request history is accepted. Historical
signatures are archived rather than invented or re-signed.

The device helper requires the original signing requirement and account association,
holds the device lock, backs up SQLite, decrypts and checks retained content, and
atomically writes the new cache representation. The Swift input helper reads the
existing credential without creating, replacing or deleting any Keychain entry.
Rollback and exact retry were checked. Diagnostic attempts can read the credential
again; no credential write is permitted. Current runtime readers accept only the
new representation.

The active owner copy is `target/unified-aliases-owner-b-20261021/`; original backend
and device checkpoints are `target/required-inboxes-owner-20261019/` and
`target/unified-aliases-device-20261021/`. All four requested addresses and the
original account are retained. A subsequent checked purpose-label correction is
archived in `cs_unified_alias_evidence`. Live activity and that correction mean the
completed conversion's baseline verification must not be treated as an instruction
to roll back or overwrite current records. Pair any rollback with the signed app
and device store, after accounting for all post-cutover activity.

Earlier converters and dated sections below document historical checkpoints.

## Required receiving assignments — 19 October 2026

The current cutover is `inbox_assignments_upgrade.py` with the standalone
`inbox-assignments-upgrade` binary. It accepts only the named local fixture copies
of schema 42, with ten native copies, no external conversations, no pending
submission and no legacy outgoing-work history. Marcus's received placeholders
are explicitly assigned to `marcus.thomas@c-sqdmail.com`. The original facts are
archived; the assignment operation and its supplied time are recorded separately
from historical admission. No permission or sender signature is invented.

The backend command copies the stopped source into a fresh database, rehearses
rollback, applies schema 43 and verifies exact retry, unchanged original facts,
keys and unrelated tables. The device command holds `client.lock`, backs up SQLite,
validates authenticated retained content and preserves the account profile, drafts,
personal state and index rows. One atomic transaction updates the required headers,
alias metadata and progress marker. It also rehearses rollback and verifies exact
retry. The read-only Swift helper must match the original app's signing requirement
before its single credential read; it never creates, changes or deletes a Keychain
entry. Runtime readers accept only the current assigned representation.

Build this tool explicitly with `cargo build --manifest-path
 tools/development-continuity/Cargo.toml --bin inbox-assignments-upgrade`.
Earlier sections and converters below describe preserved historical checkpoints;
the old whole-mailbox conversion probe is no longer part of the current client
experiment. Rollback must pair the preserved database, device store and signed app;
do not silently revert after new activity has committed.

## Current owner address scenario — 16 October 2026

Marcus explicitly requested replacing his displayed development address with
`mtallis62831@c-sqdmail.com` while retaining his original account and Keychain
identity. The public alias is `marcus.thomas@c-sqdmail.com`; private alias lanes are
`mat7771@c-sqdmail.com` (NYC-Hospital) and `m11a33t@c-sqdmail.com`
(Brooklyn-Lab-Co). These user-supplied private addresses are local fixtures only;
the member product still generates private addresses normally.

`alias_assessment.py` prepares a fresh schema-40 copy of the previous current
deployment. `src/bin/alias-assessment.rs` is a standalone, database-name-restricted
offline tool, never linked into the app/server. It constructs checked alias and
portfolio records, retains the prior directory entry in a durable prepared plan,
and atomically changes only the owner directory mailbox/revision and alias records.
It neither invents owner signatures nor rewrites historical receipts. The explicit
fixture correction is not a general mailbox-renaming API. No Keychain/cache access
or substitute owner identity is present. The wrapper checks source preservation,
all 112 unaffected table/sequence fingerprints, configured key files, rollback
after all writes and exact completed retry. Both rehearsal and owner runs passed.

The current server/config/verification artifacts are in
`target/alias-assessment-owner-20261016/`, launched by its `start.sh` at 9693.
Use this database as the source of subsequent continuity work. The prior
`target/alias-inboxes-owner-20261016/` remains an offline rollback checkpoint;
never silently switch back after new current-account activity. The original v3
encrypted device store was backed up unchanged under
`target/alias-assessment-owner-device-checkpoint-20261016/`. The unchanged canonical
signed app reopened with its original credentials. See the current Apple runbook
for running-app observations and limits.

## Receiving-inbox upgrade checkpoint — 16 October 2026

`alias_inboxes_upgrade.py` creates a fresh local schema-40 copy of the original
schema-39 owner database. It refuses a live source or populated experimental
receiving evidence, preserves original rows/sequences and migration history, and
checks configured key-file fingerprints. It never opens or changes the owner's
Keychain credential or device cache. The guarded additive migration introduces
transport-scoped receiving evidence; there is no runtime fallback reader.

The current checked owner copy and pinned server are in
`target/alias-inboxes-owner-20261016/`, started by its `start.sh` at the existing
9693 endpoint. All 114 original table/sequence fingerprints and configured keys
were preserved. The previous schema-39 copy and
`target/alias-creation-owner-20261014/start.sh` are retained as the rollback
checkpoint. Use the current copy as the source for subsequent work; do not silently
revert to the checkpoint after new activity has committed.

See the current Apple runbook for app installation/signing status and validation
evidence. A server upgrade does not establish that the updated app is running.

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

## Schema 37 external-receipt copy — 13 October 2026

`external_receipts_upgrade.py` is an offline copy tool, not a runtime migration or
credential reader. Stop the source's app/backend first. Supply its existing
configuration, a distinct freshly created local database, a new artifact directory,
and a pinned simulation-enabled cs-maild binary. The quality gate builds production
binaries too, so do not rely on a shared build-output path during a cutover.

The tool backs up schema and data, migrates the empty destination, preserves the
explicit retained maintenance archive, and restores data in one transaction. Only
fresh migration seed rows are replaced. It compares every table and sequence with
the source, rechecks configured key files, and writes `verified.json` only after
all comparisons pass. Original identity/account databases and Keychain credentials
are never rewritten. Check Keychain item/credential continuity separately with the
existing signed read-only helper; copying database rows does not establish it.

If the final verification acknowledgement was lost, rerunning the same arguments
can verify an exact complete copy. An incomplete or subsequently changed copy is
refused and retained; use another fresh database, never reset or start the uncertain
copy. A verification record establishes preservation before startup, not that later
legitimate application activity leaves every row unchanged. Fresh-copy preservation and exact-rerun recovery were challenged on an inactive
copy, matching 101 tables and nine sequences. Power-loss durability of the
maintenance artifacts was not challenged.

The verified owner copy is `cs_mail_principles_owner_ready_20261013`; original source
`cs_mail_continuity_20261012` remains unchanged. Its preservation artifacts and current
launcher are under `target/principles-alignment-20261013`. The original encrypted
cache was backed up and reopened using the original Keychain item and account.

## Alias creation format cutover (38 to 39)

`alias_creation_upgrade.py` follows the offline preservation workflow for a fresh
local copy. It requires a schema-38 source with empty legacy alias routes and
commands, retains the untouched source/backup and configured keys, and verifies
all original table/sequence fingerprints and migration history. It installs the
new creation-record constraint and single-current-public-alias index in schema 39.
It refuses populated old alias metadata rather than inventing owner descriptions
or rewriting signed commands. There is no runtime compatibility reader and this
is not a general populated-alias migration. The owner cutover evidence and current
launcher are recorded in the Apple runbook.

## Independent receiving permission (schema 41)

`alias_permission_upgrade.py` requires a stopped schema-40 source and a distinct
empty local database. It snapshots and dumps the source, rehearses complete rollback,
then applies the explicit schema-41 conversion to the copy. Original aliases,
signed management commands/receipts and delivery evidence are preserved verbatim in
`cs_alias_permission_cutover_evidence`, which runtime code never reads as authority.
Existing private grants become explicitly revisioned permission; new addresses do
not receive a lane. The current v3 replay table starts empty without rewriting old
signatures. Pending predecessor operations must be resolved before cutover.

The script checks exact address and permission conversion, route-history conversion,
all unrelated tables/sequences, original migration history and deployment key-file
fingerprints. `validate-alias-permission` reads the copy through checked Rust types.
Repeat execution verifies the same prepared result without duplicate writes. It
never accesses Keychain or edits the source or encrypted device cache. Current owner
configuration and evidence are under `target/decoupled-owner-20261016/`.


## Independent retained records — 19 October 2026

`retained-records-upgrade` authenticates the previous local container and prepares
independently encrypted copy/index rows. `retained-records-input.swift` supplies the
original credential once through an anonymous pipe; sign it with the original app
requirement. `retained_records_upgrade.py` requires the app to be stopped, holds
its store lock, backs up the database, rehearses full rollback and applies the
verified records atomically. Exact retries verify ciphertext without another
credential read. Neither account state nor Keychain is written. The owner evidence
and previous app are in `target/incremental-search-owner-20261019/`.
