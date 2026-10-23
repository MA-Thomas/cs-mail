## Superseded build state removed — 22 October 2026

Principle 11 now requires removal of superseded build state. Obsolete compiler
caches, copied executables, app bundles, generated bindings and source snapshots
were removed without creating another build archive. `target` decreased from
approximately 92 GiB to approximately 173 MiB. The next build will regenerate
its compiler outputs. Audit: `target/build-cleanup-20261022.json`.

The canonical app remains `target/apple/CsMail-continuity-20261012.app`, UUID
`C89FEC96-BC24-3B06-BDBB-A835671F33C9`. Its executable and Info.plist, the running
backend at `target/unified-aliases-owner-b-20261021/cs-maild`, and protected account
configuration/key files are unchanged by checksum. The existing app and backend
processes remained running; refreshing Communications completed successfully and
the existing aliases remained visible. The current device database passed a
read-only integrity check. No Keychain entry was written or replaced.

Durable development state still resides under `target/apple-local-state/` and
`target/apple-continuity-state-20261012/`; signing material remains under
`target/apple-signing/`. Preserve these paths and the current deployment during
future cleanup. Database/conversion evidence, validation records and the small
encrypted removed-design writing store were retained. Do not treat all of
`target` as disposable. Earlier dated sections record historical deployments;
references to superseded binaries, app bundles and source backups are not an
inventory of retained files.

No runtime source or build settings changed in this cleanup. No rebuild or
unrelated Rust/PostgreSQL suites were run; validation covered file preservation,
device-database integrity and a live app refresh, not every application workflow.

## Native column divider line removed — 21 October 2026

The native macOS split-view divider is covered with the canvas before the custom
perimeter and spill are drawn. This changes its visible line only: the native
resize hit region, measured column bounds and eight-point bottom gutter remain.

The canonical signed app is installed and running on the original account,
UUID `C89FEC96-BC24-3B06-BDBB-A835671F33C9`. Strict signature verification passed
and the designated signing requirement matches the original. Running-app
inspection confirmed the extra line is absent at the top, alongside the column
and below its rounded bottom edge, while the custom rim and eight-point gutter
remain. Jules’s conversation was reopened afterward. Build and diff checks passed;
no new tests or unrelated Rust/PostgreSQL suites were run. Dark appearance,
other areas/window sizes, resizing interaction and iOS were not rechecked.
Build log: `target/column-b-divider-20261021.log`.

## Eight-point bottom gap restored — 21 October 2026

At Marcus’s explicit direction, column B’s bottom gap is restored to exactly
8 points. The canvas beneath that gutter remains; the renderer’s lighting falloff
settings are unchanged. This supersedes the 24-point clearance described below.

The canonical signed app is installed and reopened on the original account,
UUID `74338BA3-E653-342F-8C59-313784AC357B`. Strict signature verification and
original designated signing requirement checks passed. Running-app visual
inspection confirmed the smaller gap in Communications, and Jules’s previously
open conversation was restored. Build and diff checks passed. No new tests or
unrelated Rust/PostgreSQL suites were run for this presentation change. Dark
appearance, other areas/window sizes and iOS were not rechecked.
Build log: `target/column-b-eight-point-20261021.log`.

## Column B bottom gutter — 21 October 2026

The initial eight-point gutter below did not preserve the full bottom lighting
falloff. The correction uses `EditorialCoveLight.spillExtent` (24 points), shared
with the renderer, for bottom clearance. The gutter uses the app canvas beneath
the glow, replacing the exposed native sidebar background strip.

The final canonical signed app is installed and running with the original account
and designated signing requirement, UUID `CFEF10C0-65A1-3D70-9DC7-7DCC441A625F`.
Strict signature verification passed. Running-app visual inspection confirmed
bottom spill for Relationships in light and dark appearance and Communications
in light appearance. Light appearance was restored. The build and diff checks
passed; no new automated tests or unrelated Rust/PostgreSQL suites were run.
iOS, other window sizes and the remaining area/appearance combinations were not
inspected. Evidence: `target/column-b-bottom-spill-20261021.log` and
`target/column-b-bottom-spill-20261021.json`.

The following records the superseded first gutter attempt.

Column B now reserves eight points below its content, glass surface and measured
perimeter. Content is clipped inside the column before its glass background is
applied; the top toolbar cap and existing perimeter lighting remain in place.

The canonical signed app is installed and reopened on the original account with
UUID `1CCCED80-6BDD-39F8-8F0B-4F57BEB1F0E2`. Strict signature verification passed
and the designated signing requirement matches the original. Running-app visual
inspection confirmed the bottom gutter in Relationships and Communications,
scrolling to the end of the inbox list within the perimeter, the retained Manage
aliases action, and successful navigation to My aliases. The five active aliases,
including Aetna 2026, remain present; the deleted old Aetna alias remains absent.
Build evidence: `target/column-b-bottom-gutter-20261021.log`.
Validation record: `target/column-b-bottom-gutter-20261021.json`.
No automated tests were added and unrelated Rust/PostgreSQL suites were not run
for this presentation change. iOS, other window sizes and dark appearance were
not inspected in this pass.

## Readable alias previews — 21 October 2026

The final signed app is installed and running with UUID
`37AEFFA6-5A4F-399E-BEEC-8B9DFF328D1F`. Signature verification passed and the
designated signing requirement matches the original. It reopened the original
account, displayed the final shorter name prompt, and generated a fresh preview.
Build log: `target/readable-alias-apple-final.log`; validation record:
`target/readable-alias-validation-20261021.json`.

Generated addresses now combine a member-approved name fragment (2–8 lowercase
Latin letters) with six random digits, including leading zeroes. The local part is
8–14 characters. Name input is explicit and independent of purpose, identity
records and receiving policy. The member can regenerate or choose a preferred
fragment. The same controls support alias replacement.

Rust owns generation in `crates/cs-mail-application/src/aliases/generation.rs`.
Reserved addresses inform prefix occupancy and candidate rejection; the soft
crowding threshold is 10,000 per prefix/domain and selection is bounded at 32
attempts. Preview is authenticated but reserves nothing. Creation signs the exact
reviewed address; atomic uniqueness resolves competing claims. A known collision
requires another reviewed preview, while an uncertain result retains the original
operation for recovery. Existing aliases are never regenerated or renamed.

The owner backend remains on port 9693 with the same database, keys and account.
Its pinned binary and checksum are in `target/unified-aliases-owner-b-20261021/`;
the previous binary is `cs-maild.before-readable-generation`. The current Aqua
session launchd job is `com.csqd.cs-mail.owner-readable-aliases`, invoking the
existing `start.sh`; logs are `server-readable-aliases.log` and
`server-readable-aliases.err`. Check that job before starting another server.
At Marcus’s subsequent explicit request, the unused old Aetna Insurance alias
was completely deleted, including its address reservation and two command
receipts, without creating a backup. A transaction rechecked every other public
table for dependencies before deleting that exact retired development fixture.
This is an explicitly authorized development-data exception, not a change to
normal retirement semantics. The four requested addresses and the newer
`thoma973837@c-sqdmail.com` (Aetna 2026) remain active. Running-app inspection
confirmed that the old entry is absent and the newer Aetna alias remains.
No Keychain credential was written or replaced.

Validation adapted existing experiments without adding automated test entry cases:
required name fragments, bounded collision exhaustion, crowded-prefix selection,
non-reserving previews, concurrent claims, and exact retry/reopen recovery.
Workspace all-target checking and strict all-feature lint passed, as did four
PostgreSQL directory tests, six portable-client runtime tests, six affected SMTP
integration checks, and the full native SQLite/FFI journey. Native checks create
the exact preview and exercise replacement. Evidence is in
`target/readable-alias-{check-final,clippy-next,directory,client-runtime,smtp,native-check}.log`.
Running-owner inspection verified regeneration, fragment selection, initials,
preview invalidation, creation readiness under both policies, and draft retention
across closing/reopening; no owner alias was created during inspection.
The full quality gate, iOS and public/live-provider delivery were not run for this
change. These local checks do not establish public email delivery.

## Main-space return navigation — 21 October 2026

The return button now occupies a fixed layout row above the detail viewport.
Scrolling content is clipped to that viewport, so titles and controls cannot draw
over the return action or enter the window-toolbar region. This replaces the
transparent safe-area inset; navigation history and return actions are unchanged.

The canonical signed app is installed and running with its original signing
requirement and account association; build/installed UUID
`EB8995BF-B4C3-3E03-A271-23B33B1A91B5`. Running-app inspection reproduced the overlap,
then verified scrolling in both directions and successful return navigation after
the fix. All five current owner aliases, including the newly created Aetna Insurance
alias, remain present. Evidence: `target/navigation-scroll-build-20261021.log` and
`target/navigation-scroll-verified-20261021.json`. No automated tests were added;
Rust/PostgreSQL suites were not rerun for this presentation change. iOS was not
inspected. Generated address length and readable format remain under discussion;
the generator and all existing addresses are unchanged.

## Unified aliases — 21 October 2026

The Rust model, signed commands, PostgreSQL schema 44, portable client and unified
Apple application now use one alias type. Address choice, owner-visible purpose,
receiving policy and directory visibility are independent. Creation requires an
explicit choice between Bonded and an alias-scoped express lane. Each bonded alias
owns its profile and request classes. First publication requires an explicit
visibility choice. Multiple bonded aliases are supported.

Sender lanes and standing permission apply to named receiving aliases. Explicit
renewal retains that selection; creating another alias never expands it. Request
terms freeze the recipient alias and the sender's selected return alias. Choosing
a correspondence address supplies no receiving authority. Runtime readers accept
only the current signed and encrypted representations; there is no legacy fallback.
Policy switching, alias fees, privacy-preserving correspondent references and held
mail remain deferred. Local simulation does not establish public email delivery.

The preservation-checked owner deployment is
`target/unified-aliases-owner-b-20261021/`, started by its `start.sh` on port 9693.
The previous `target/required-inboxes-owner-20261019/` database is an untouched
checkpoint. Backend and device conversion rehearsed rollback and exact retry,
archived original evidence, and retained the original account association and
Keychain entry without credential writes. Device/app checkpoints are under
`target/unified-aliases-device-20261021/`. Rollback must account for activity since
cutover and pair the backend, device store and signed app.

All four requested addresses are retained: `mtallis62831@c-sqdmail.com`,
`marcus.thomas@c-sqdmail.com`, `mat7771@c-sqdmail.com` (NYC-Hospital), and
`m11a33t@c-sqdmail.com` (Brooklyn-Lab-Co). The existing publication remains attached
to its exact `mtallis62831` receiving address; it was not copied onto the distinct
`marcus.thomas` alias. Existing sender permissions retain their explicit scopes.

The final corrected signed build (UUID `90D3C886-B1CB-3BCD-B85D-7B749B4AAE2B`)
is installed at the canonical path and has reopened with the original account.
Signature verification passed and its designated signing requirement matches the
original app. Running-app inspection verified all four retained aliases, successful
live refresh, both receiving-policy choices, independent generated/chosen address
selection, the preserved original profile/class, and the separate bonded alias's
empty profile/classes with an explicit first visibility choice. No extra owner
alias was created and no publication was changed during inspection. The alias list
is left open. Evidence: `target/alias-build-install-status.json`,
`target/alias-validation-final.json` and `target/alias-apple-build-installed.log`.

The full native SQLite/FFI journey passed in `target/alias-native-check-h.log`,
including creation's atomic lane grant, per-alias publication, explicit quote/payment
review, request/reply routes, source selection, lane renewal, multiple receiving
inboxes, scoped unread/search, retirement, blocking and same-device reopening.
Strict all-feature and production lint rechecks passed in
`target/alias-clippy-verified.log` and `target/alias-production-clippy-verified.log`.
The final owner backend is pinned and running (`server-final.log`); its predecessor
binary is retained alongside it. No new automated test cases were added. PostgreSQL
suites and workspace tests passed in `target/alias-quality-gate-verified.log`.
That gate run recorded the earlier lint failure for the expanded publication
experiment; both subsequent strict lint rechecks passed after the correction.
It must not be presented as a single clean gate exit.
iOS, public/live-provider email delivery, and the optional native external-email
journey were not validated in this run; Rust external/SMTP integration is covered
by the local database suites. Do not treat the historical results below as evidence
for this cutover.
See the [domain model](../../DOMAIN_MODEL.md#aliases-and-express-lane-scope).

## Conversation find — 19 October 2026

Native and external readers keep messages visible, highlight literal
case-insensitive text matches, distinguish the current occurrence and provide
previous/next controls with a count. Navigation wraps. Matching includes displayed
quotations and shared copies, respects the receiving-inbox scope and never follows
references. Clear restores the unhighlighted text. Find state is local presentation
state; encrypted storage and mailbox indexing are unchanged.

The canonical build passed and is installed and running with the original signing
requirement; build/installed UUID `69C62FD7-A6D6-3FB7-9060-EE719A948F1B`. Focused experiments
using the actual matcher and find model passed Unicode range fidelity, formatted
excerpts, scope removal, occurrence counts, wrapping and clear/restart. Evidence is
in `target/conversation-find-20261019/`. Rust/database suites were not rerun for this
presentation change. iOS and live external-reader inspection are not established.
Running-app inspection verified all-match highlighting, the distinct current match,
counts, navigation within/across messages, scrolling, wrapping in both directions,
no-match controls and clearing. Inspection exposed and corrected native scrolling
that initially changed the current match without bringing it into view. Daniel's
conversation is left open with find cleared.

## Required receiving inboxes — 19 October 2026

Every received native or external message now carries a checked receiving-inbox
assignment. Rust delivery effects keep that assignment with the message and its
content; checked restoration and database constraints preserve the binding.
Receiving history is derived from those records. The aggregate mailbox derives
received correspondence from alias inboxes, with sent copies represented separately.
The Inbox screen excludes sent-only conversations.

Ordinary native sends and initial requests sign the destination's public alias.
Explicit alias sends retain their selected destination. External conversations
record their receiving alias at creation, including for later token replies.
Alias selection establishes a destination; existing permission rules still decide
whether delivery is allowed. Development assignments cannot grant alias permission.

Marcus approved assigning the existing received placeholders to
`marcus.thomas@c-sqdmail.com`. The preservation-checked cutover records that decision
explicitly, archives original evidence and verifies authenticated retained content,
rollback and exact retry. The original account, Keychain entry, addresses, message
content and admission history are preserved. The signed helper read the existing
credential once and made no Keychain writes.

The current owner service/config is `target/required-inboxes-owner-20261019/`,
started by its `start.sh` at the existing 9693 endpoint. The previous
`target/subject-search-owner-20261019/` database remains an untouched checkpoint.
The previous signed app and encrypted device backup are under
`target/required-inboxes-device-owner-20261019/`. Use the new owner copy for subsequent
work; rollback must pair the app, device store and backend, and must account for
any activity committed since cutover.

The canonical signed app is installed and running on the original account;
built/installed UUID `82036424-B0FD-3AE1-95A9-479FF3904405` and the original signing
requirement match (`target/required-inboxes-20261019/apple-final.log`,
`running-app.json`). Running-app checks verified all five received placeholders in
the public inbox, an empty private inbox, reading and device-scoped body search
with the service stopped, and successful refresh after restoring the owner service.
The final presentation also loads current receiving metadata when connected,
without requiring a successful cached projection first. The public inbox is left open.

The complete native SQLite/FFI journey passed, including native and external
delivery, mixed-alias membership, aggregate/alias message-ID agreement, scoped
search, read state, retirement history and reopening (`native-complete.log`).
All eight local quality-gate checks passed, including PostgreSQL integration
suites (`quality-complete.log`); logs are in `target/required-inboxes-20261019/`.
Existing experiments were extended; no automated test cases were added. Earlier
attempts exposed missing fixture inboxes, a fixture-domain mismatch and the cached
projection ordering issue; their logs remain separate. This work does not establish
iOS behavior, real-provider delivery, real power-loss recovery or new performance
measurements. Cloud indexing and WASM remain deferred.

The earlier checkpoints below retain their original validation and deployment
evidence; they do not identify the current server or app build.

## Incremental encrypted search storage — 19 October 2026

Retained copies and per-message index records are independently encrypted SQLite
rows. One transaction commits the affected copy, index record and progress marker;
only committed changes enter the session index. A failed or uncertain commit
invalidates that cached index so the next operation reloads durable state. Rebuilding
updates index records without rewriting copies. The replaced whole-mailbox reader
and writer are removed; the 64 MiB limit now applies per record.

Unlock builds the in-memory word lookup from index records without opening message
copies. Queries select candidate IDs directly; snippets and readers retrieve selected
copies. Result pages load as they come into view. Known expiry/removal, query/scope
binding and reader-return context remain enforced. Downloads release the index lock
before network waits, and cannot repopulate a session cleared while they were waiting.

Validation extended existing experiments; no automated test cases were added.
Seven local quality-gate checks passed; PostgreSQL suites were explicitly skipped
because no server contract changed (`target/incremental-search-20261019/quality-ready.log`).
Final focused client and lint checks passed (`client-installed.log`, `lint-installed.log`).
The native SQLite/FFI journey passed transaction-failure rollback, stale revisions,
aggregate storage beyond 64 MiB, native/external indexing, alias search and session
clearing (`native-ready.log`). Earlier native attempts exposed assumptions about
scan cursors and cleared alias sessions; those logs are retained separately.

The optimized synthetic experiment reached 100,000 records: selective warm search
about 0.04 ms, no-match search about 0.004 ms and cold index opening about 0.9 s,
with no message-body reads during cold opening. The largest checkpoint in that
fixture stayed 2,438 bytes (`scale-release.log`). These are component measurements
using a memory-backed store and simple 1 KiB text records, not SQLite or full-app
latencies. Cold opening and memory use still grow with the index; phrase checks
may retrieve word-matching candidates. Images and realistic mixed-mailbox workloads
were not benchmarked. Other account/receiving-metadata snapshots retain their bounds.

The canonical signed app is installed and running on the original account; the
built/installed UUID is `8F765FCA-0B91-3C09-94CB-BA9B25CAA346`, with the original
signing requirement preserved (`apple-complete.log`). Running-app checks with the
service offline verified subject/body search over the ten retained copies, reading,
return to results, lock/unlock and saved receiving-inbox scope. Search layout was
visually inspected. The existing owner service was restored at 9693 and All inboxes
left open. SwiftPM products now explicitly relink the external Rust archive.

The one-time conversion verified authenticated copies, exact text, account binding,
rollback and exact retry. Account cache, drafts and personal-state ciphertext were
unchanged by conversion; Keychain was never written. The helper made one credential
read using the original app signing requirement. The former app and device backup
are in `target/incremental-search-owner-20261019/`; conversion evidence is
`owner-upgrade.log`. Backend schema, keys and addresses were unchanged.

Not covered: iOS, real power loss, a full appearance/accessibility matrix or public
provider delivery. Cloud indexing and WASM remain separate design discussions.

## Earlier subjects and offline-search checkpoint — 19 October 2026

New native conversations and initial requests accept a shared subject, fixed for
both participants. Blank input receives `Default subject: October-19-2026, 3:42 PM`
using the sender's local date/time. Replies inherit it. Subjects are encrypted;
conversation lists, readers and device-local search display them alongside the
correspondent's name. Existing development placeholders received date/time subjects.

Rust retains authenticated copies and a rebuildable word index in one encrypted
SQLite record. After unlock, search and readers use local copies without the
service. Connected background downloads extend coverage in bounded batches.
Results distinguish not-downloaded, unavailable and remaining content; known expiry
and removal apply before matches or snippets appear. Saved receiving evidence
restores alias inboxes offline. Lock clears transient results.

Validation: all eight local quality-gate checks passed, including PostgreSQL
(`target/search-subjects-20261019/quality.log`). Extended existing experiments
challenged subject substitution, retry, offline reopening, expiry/removal, failed
index checkpoints and plaintext leakage; no automated test cases were added.
The complete native SQLite/FFI journey passed, including shared subjects through
replies and reopening, subject search, caller-only removal and retained-store
transaction failure (`native-final.log` in the same directory).

The canonical signed app is installed and running on the preserved owner account.
Built/installed executable UUIDs match `F0004EA1-A501-3F53-85E4-D1767F3B0288`;
the original designated signing requirement is retained. Running-app checks with
the service stopped verified quit/reopen, lock/unlock, saved alias inboxes, subject
search over all ten retained messages, local reading and return to search results.
Screenshots checked subject/correspondent rows and the reader heading. The service
was restored and All inboxes left open (`apple-ready.log` records the final build).

The pinned owner service is `target/subject-search-owner-20261019/start.sh`, using
`cs_mail_subject_search_owner_20261019` at the unchanged 9693 endpoint. The verified
rehearsal copy was adopted without a second credential conversion. The schema-41
source remains offline at `cs_mail_decoupled_owner_20261016`; original device backup
and encrypted upgrade artifacts remain in
`target/subject-search-preservation-check-20261019/`. Rollback, exact retry, original
evidence archival and unrelated table/key fingerprints passed. The original
Keychain entry, account association and four requested addresses were preserved.

Limits: the initial word/phrase index has a 64 MiB encrypted-record bound. Large
mailboxes and the proposed FTS5/SQLCipher comparison remain unevaluated; this does
not settle the long-term engine choice. iOS, graphical subject creation, real power
loss, a full appearance/accessibility matrix and public/live-provider delivery
were not validated. External mail does not gain native shared subjects.

## Communications heading — 16 October 2026

The aggregate Inbox main-space heading is “All inboxes”, without the redundant
“· Inbox”. Other mailbox/address headings keep their scope. The canonical signed
build was installed and reopened on the preserved account, and the running
accessibility tree confirmed the heading. Build and diff checks passed; no new
tests, unrelated Rust/database checks or screenshots were used. Build log:
`/tmp/cs-mail-all-inboxes-heading-build.log`.

## Navigation glass surfaces — 16 October 2026

Column B and the Column A hover box use native regular-material frosting beneath
their original background colors at 82% opacity. Column B retains its canvas
color; the hover box retains its original area-specific background and inverted
appearance for text and controls. No extra area wash is applied to Column B.
Reduce Transparency restores each original opaque background. Interior gradients
remain absent; perimeter palette values and lighting rendering are unchanged.
The canonical signed app was built, installed and reopened on the preserved
account with Relationships restored for owner assessment. Build and diff checks
passed. No screenshot capture was used, so rendered glass/contrast, Reduce
Transparency, dark and compact/iOS layouts were not visually checked.
No automated tests or unrelated Rust/database checks were run. Build log:
`/tmp/cs-mail-original-glass-colors-build.log`.

## Relationships sidebar — 16 October 2026

Relationships' Column B contains a Your relationships link above the Manage links,
returning to the main relationships page through the existing guarded area
navigation. This resets return history, so selecting the root does not add a
redundant Back button there. Management pages retain their return controls.
It omits the people search,
permission groups or contact list. Their unused grouping preference and search
checkpoint state were removed. Selecting Relationships in Column A still opens
the main overview, whose Map/List layout control remains available. The canonical
signed build was installed and reopened on the preserved account, and the running
accessibility tree confirmed the sidebar links and return from My aliases to the
main relationships page without a Back button, including repeated root selection.
No screenshot capture or unrelated Rust/database tests were used. Build log:
`/tmp/cs-mail-relationships-root-navigation-build.log`.

## Column A label — 16 October 2026

The “cs” label and its header spacing were removed from Column A. The canonical
signed app was built, installed and reopened on the preserved account; its
accessibility tree confirms the rail starts with Home. No screenshot was taken,
to avoid reintroducing the system capture indicator. Build log:
`/tmp/cs-mail-remove-rail-mark-build.log`. No automated tests or unrelated
Rust/database checks were run for this presentation change.

## Column A divider spacing — 16 October 2026

Column B and the Column A layout preview retain the reduced gap from
the Column A divider, with a four-point rightward adjustment to clear the Mac
title-bar control area. The shared spacer is 10.75 points, alongside the native
sidebar's existing eight-point gutter. The signed canonical build was installed
and reopened on the preserved account; screenshot inspection confirmed both edges
remain aligned and clear the visible purple system sharing indicator. The ordinary
traffic-light buttons were not visually verified: macOS displayed its sharing
indicator in their place during inspection. Strict signature verification passed and
built/installed UUIDs match `28B5F797-62DF-3419-BD0E-831EC76BD1E6`.
Build log: `/tmp/cs-mail-window-clearance-build.log`. Validation was the native build,
changed-screen inspection and diff checks; no new tests or unrelated Rust/database
suites were run. Compact/iOS and dark layouts were not separately exercised.

## Column A transient layout preview — 16 October 2026

Selecting a Column A icon navigates without pinning its layout preview. Pointer
previews remain only while the pointer is over the icon or popup; interacting with
the popup does not pin it. The redundant Close button and pinned session state
were removed. Right Arrow still opens keyboard controls and Escape dismisses them.
Initial pointer dwell is now 750 milliseconds.
The signed canonical app was rebuilt, installed and reopened on the preserved
account for this timing change. Build log:
`/tmp/cs-mail-hover-750ms-build.log`. Build and diff checks passed; exact pointer
timing was not measured, and no screenshot capture or new tests were used.

The canonical signed app was built, installed and reopened on the preserved owner
account. Strict signature verification passed, the original signing requirement
was retained, and built/installed UUIDs match
`E0446B58-9A03-318F-89A7-3966C29A7029`. Running-app checks confirmed navigation
without a pinned box, clicking an icon dismisses its keyboard-opened preview,
Right Arrow opens controls without an X, and Escape dismisses them. Screenshot
inspection checked the simplified popup. Build log:
`/tmp/cs-mail-hover-unpin-build.log`. No automated tests were added or Rust/database
suites run for this presentation change. Pointer transit timing, iOS and compact/dark
layouts were not separately exercised.

## Decoupled receiving permission — 16 October 2026

Private alias creation now establishes an address without lane authority. The
separately validated `ReceivingPermission` has a distinct `PermissionRevision`.
Signed v3 management supports opening, restricting and removing a broad lane and
atomic replacement with selected signed sender grants. Restrictions freeze exact
admitted conversations; removal and sender replacement do not grandfather them.
Address lifecycle remains separate. Ordinary relationship permission remains an
independently checked authority for its explicitly named receiving scope.

Sender-restricted receiving uses existing `SenderScopedLane` validity and allowance
rules. Native evidence binds the known sender; SMTP binds verified author-domain
evidence to selected legacy-domain scopes while preserving the actual mailbox
identity and its blocks. Chosen grant state, receiving evidence, message copies,
allowance and receipts commit under the existing receipt-order barrier. Historical
retry never reactivates a superseded permission, and private refusal has no bonded
fallback. Swift separates permission from address status and groups lanes by their
actual scope. New aliases start unpaired; the existing two private owner routes keep
their recorded broad authority through explicit preservation conversion.

Validation: the full eight-check local quality gate passed, including all 106
PostgreSQL checks (`/tmp/cs-mail-decouple-quality-final.log`). The focused real-SMTP
experiment also passed preparation/removal fencing, authenticated domain mismatch,
allowance and exact redelivery (`/tmp/cs-mail-decouple-smtp-final.log`); targeted strict
lint passed (`/tmp/cs-mail-decouple-stale-preparation-lint.log`). The complete native
SQLite/FFI/isolated-Keychain journey passed, including unpaired creation, restricted
existing versus new conversations, removal and native sender replacement, read/search
scope, retained-copy removal, request/reuse/recovery and external consent
(`/tmp/cs-mail-decouple-native-complete.log`). Existing experiments were extended;
no automated test cases were added. Earlier attempts retain their logs: one had a
superseded replacement-lane expectation; another launched a production-only helper
after a shared build overwrote it. Final native validation pins both helper binaries
under `target/decoupled-native-tools-20261016/` and uses separate databases on
10073–10075. The isolated test services were stopped after the checks.

The current owner server is pinned at `target/decoupled-owner-20261016/start.sh`,
serving the unchanged 9693 endpoint against `cs_mail_decoupled_owner_20261016`.
The schema-40 source `cs_mail_alias_assessment_owner_20261016` remains offline and
untouched as the rollback checkpoint. Rehearsal, complete rollback and exact rerun
checks passed. All 111 unrelated table/sequence fingerprints and configured key
files matched. Original address records, command receipts and route evidence were
archived verbatim before explicit conversion; no runtime predecessor reader was
added. Owner account/persona/registered keys and all four requested addresses were
preserved. The original encrypted device files were backed up and verified unchanged
before reopening; the maintenance tool never accesses Keychain.

The final signed build is installed at the canonical app path and running on the
preserved owner account. Built and installed executable UUIDs match
`F2C3D959-DCB9-3F18-9716-49DD22F081B2`; strict signature verification passed and the
original designated requirement is retained. Running-app inspection verified both
private alias lanes, separate address and permission controls, restrict/remove
confirmation meanings, sender replacement contacts/domains/expiry/daily limit,
and the new-alias explanation that creation grants no lane. Changed controls were
visually inspected; owner permission confirmations were cancelled. Brooklyn-Lab-Co
inbox was restored after inspection. Build log:
`/tmp/cs-mail-decouple-apple-ready.log`.

Not covered: permission mutations on the actual owner account (covered in isolated
native fixtures), iOS, a full accessibility or
appearance/layout matrix, public/live-provider delivery, held-mail release,
outbound alias selection, real power loss or large-mailbox performance.

## Address and lane management boundary

Rust separates address lifecycle from independently revisioned receiving permission.
Private creation grants no lane. Explicit signed actions open, restrict or remove
an alias-scoped lane, or atomically replace access with selected signed sender
grants. The existing alias transaction commits permission and its exact-operation
receipt together. Native and SMTP admission recheck current permission; domain
matching uses authenticated evidence while correspondence and blocks retain the
actual sender identity. Swift groups lanes by permission and exposes pairing
controls. The schema-41 cutover preserves original evidence and the owner account
in an offline checked copy, with no runtime predecessor reader.

## Popup close focus appearance — 16 October 2026

The shared Close button disables its system focus effect, removing the blue
perimeter around its enlarged target while retaining keyboard focus, accessibility
and Escape. The canonical signed build installed and reopened on the original
account; strict verification passed and built/installed UUIDs match
`58B6EA72-CF46-33CF-B4DA-D855BC48CFA6`. In the running Comms popup, keyboard traversal
focused Close and screenshot inspection confirmed no blue outline. An offset click
inside the 44-point target still dismissed it. Brooklyn-Lab-Co/Inbox was restored.
Build log: `/tmp/cs-mail-popup-close-focus-build.log`. Diff checks pass. Validation
was limited to the native build and changed macOS control; no new tests or
unrelated Rust/PostgreSQL suites were run, and iOS/dark/compact layouts were not
separately exercised.

## Popup close target — 16 October 2026

The shared layout popup Close button now places its small xmark inside a
44 × 44-point label with an explicit rectangular content shape. The clickable
space belongs to the button, rather than padding outside it. Header insets retain
the icon's position and compact header. All six area popups use this one control;
the accessible Close name and Escape shortcut remain.

The canonical build installed with its original signing requirement, passed strict
signature verification, and matches the built executable UUID
`4F0BCB6C-3FF1-3817-8D1A-3414EDBAAB6B`. After quit/reopen through the original
account, the Comms popup dismissed when clicked 15 points right and below the
glyph center, inside the enlarged target and outside the former target. Screenshot
inspection confirmed the click location and retained inbox. Build log:
`/tmp/cs-mail-popup-close-target-build.log`. Diff checks pass. No new tests or
unrelated Rust/PostgreSQL suites were run; iOS, compact/dark layouts and every
popup instance were not separately exercised for this shared presentation change.

## All-area layout controls — 16 October 2026

Every column-A popup now contains presentation controls: Home cards/list within
the existing permission sections; Comms receiving-inbox/mailbox section order;
Relationships grouping and map/list; Express lanes private-alias/sender section
order; Economics newest/oldest/name record order; Notes reading width for its
current informational page. Notes creation/saving remains unimplemented. Choices
are non-sensitive device-local AppStorage values; no domain facts or authorities
are changed. All icons now share configuration pinning, Escape/Close/outside-click
dismissal and preservation of current detail when clicking the selected area.

After owner signing approval, the canonical build installed with the original
designated signing requirement. Strict signature verification passed; built and
installed executable UUIDs match `D35BC58E-4365-335A-9786-2620C874098D`.
The prior process was quit and the canonical app reopened through its existing
account/Keychain association, without enrollment or credential replacement.

Running macOS inspection verified all six popup control sets. Home changed to a
single-column list while retaining permission sections; Comms reordered sections
while retaining Brooklyn-Lab-Co/Inbox; Express lanes reordered while keeping the
Brooklyn alias detail open; Economics changed newest/oldest/name order while
retaining Daniel's financial record. Notes focused width visibly changed the
current page. Relationships retained Name/List. Escape dismissal and the accessible
preview action were also exercised. A second quit/reopen verified Home List,
Comms Mailboxes first, lanes People first, Economics Name and Notes Focused.
The app is left in Comms/All inboxes with its layout popup open for assessment.

Build log: `/tmp/cs-mail-all-layout-controls-build.log`; pre-pass source snapshots:
`target/all-layout-controls-baseline-20261016/`. Diff checks pass. No new tests or
unrelated Rust/PostgreSQL suites were run. This presentation pass covered the
Apple build, signature/executable checks and affected running screens; iOS,
compact/dark layouts, large datasets, exact hover timing, every keyboard traversal
and every filtered navigation permutation remain unexercised.

## Relationships layout controls — 16 October 2026

The Relationships column-A popup now changes organization rather than repeating
the workspace description. People in column B can be grouped by Permission,
ordered by Name, or ordered by Recent correspondence. The main overview switches
between Map and List using the same existing relationship summaries. Clicking
Relationships opens its workspace and pins the controls; clicking it while already
there preserves the current detail and search. Escape, Close and outside content
click dismiss the controls. Other area popups remain orientation aids in this pass.

The two non-sensitive layout preferences persist locally in AppStorage. They do
not alter permissions or member records and are not reverted by history Back.
Recent ordering uses available message/request timestamps with stable name and
identity tie-breakers. No message content or search query is added to preferences.

Validation: canonical Apple build, strict signature verification, matching built
and installed executable UUID `ACF9C70A-47A4-3337-9913-155C000F2DDC`, diff checks
and running macOS inspection. All three organizer choices and Map/List changed
immediately. With Daniel Reed open and a Daniel search active, changing both
preferences retained the detail and query. Escape dismissed the popup. Quit/reopen
through the original account retained Name/List; the app is left there with the
popup open and the assessment search cleared. The original signing requirement
was preserved; no enrollment or credential replacement occurred.

Evidence: `/tmp/cs-mail-relationship-layout-build.log`; pre-pass source snapshots:
`target/relationship-layout-baseline-20261016/`. No new tests or unrelated
Rust/PostgreSQL suites were run. iOS, compact/dark layouts, keyboard traversal of
every control, large datasets and all navigation permutations were not exercised
in this presentation-only pass.

## Workspace-linked interior top tint — 16 October 2026

Every actual column-B organizer now uses one background wrapper, including Comms.
A neutral, opaque canvas prevents the native sidebar surface from supplying an
unrelated hue beneath the subtle tint. The existing EditorialColumnTopTint uses
the same EditorialAreaPalette as the perimeter: turquoise Comms, dark-gold
Relationships, silver-blue Express lanes and olive Economics. Its 120-point
height, gradient stops and appearance-dependent opacity are unchanged. The middle
and lower background are neutral. Hover previews keep their separate contrasting
surface; navigation and domain behavior are unchanged.

The canonical build installed with its original signing requirement; strict
signature verification passed. Installed/built executable UUIDs match
`C6B6F143-2F8F-3692-B80A-39A135DC78A3`. After quit/reopen on the original account,
running macOS screenshots verified all four top tints in light appearance and
Comms in dark appearance. The gradient follows the workspace palette and fades to
the neutral column interior. Light appearance was restored, and the app was left
in NYC-Hospital/Inbox, matching its pre-install receiving selection. No enrollment,
credential, permission, message or financial changes were performed.

Validation: canonical Apple build, signature/executable checks, diff checks and
changed-screen inspection. Evidence: `/tmp/cs-mail-column-area-tint-build.log`;
pre-pass source snapshots: `target/column-area-tint-baseline-20261016/`. No new
tests or unrelated Rust/PostgreSQL suites were run. iOS, compact layouts and every
dark workspace were not visually exercised in this presentation-only pass.

## Shared return navigation and captured context — 16 October 2026

The Brooklyn-Lab-Co inbox → Manage permissions route recorded an origin but the
Express lanes detail exposed no history return action. The main-space owner now
supplies a labeled Back action whenever history exists and the rendered detail
has no existing equivalent. This covers lane/financial details, Directory, the
approach editor, alias management, nested mailbox views and unavailable native
details. Native correspondence and external-email readers retain their established
return controls. Alias management retains its separate collection-return action;
the origin-return control now comes from the shared main-space owner.

Navigation checkpoints capture a MailboxNavigation context (folder, query, sort,
bulk selection and stable list/search anchors) plus relationship/financial/lane
filters and alias-management selection. Back restores the captured values, not
whatever happened to remain in the mutable workspace after an intervening route.
Existing receiving scope, selected person/thread/message and the protected-edit
Save/Discard/Stay guard remain part of the return path. Explicit Inbox/mailbox
selection continues to start the intended fresh context. No Rust domain, cache,
credential, message, permission or financial contract changed.

Validation challenged the actual reported route before the repair, then the
installed fix: Brooklyn-Lab-Co/Inbox → Manage permissions → Back restored that
inbox. A stronger nested journey began in Brooklyn-Lab-Co/Unread with A–Z enabled
and a search query, opened permissions, used View inbox (which intentionally reset
the mailbox), then returned twice. Unread, query and sort were restored exactly.
Permissions → Manage alias → Back restored the selected Brooklyn lane and its
receiving filter; the next Back restored Brooklyn-Lab-Co/Inbox. No alias mutation,
message send, charge or permanent deletion was performed. The app was left in
Brooklyn-Lab-Co/Inbox with the assessment query cleared.

The canonical build installed with the unchanged original signing requirement,
passed strict signature verification, and matches the built executable UUID
`91A769EC-BC4D-358D-9F81-A00C4E831C5B`. The app was quit/reopened through its original
account/Keychain association without enrollment or credential replacement.
Evidence: `/tmp/cs-mail-navigation-return-build.log`; source snapshots are under
`target/navigation-return-baseline-20261016/`.

Validation scope: native build, signature/executable checks, diff checks and the
running macOS journeys above. No new tests or unrelated Rust/PostgreSQL suites
were run. iOS, dark/compact layouts, long-list scroll/selection restoration,
unavailable-reader states and every possible navigation permutation were not
manually exercised. These checks establish the local navigation repair, not public
mail delivery.

## Stable workspace organizers — 16 October 2026

Column A selects a workspace; its contrasting hover/focus previews now explain
that workspace briefly, without duplicating its lists or management controls.
Column B remains persistent and owns organization within each workspace. The
approved colors, continuous spill and absent sidebar hide control remain.

Comms separates receiving inbox scope from mailbox selection. Inbox, Unread,
Flagged, Archive, Trash, Sent and Outbox are selected in column B; the main-space
mailbox picker is removed. Sent/Outbox remain All-inboxes views, as before. Drafts
is available in the organizer. Mailbox selection uses the existing navigation
owner and scoped Rust projections. Explicit mailbox changes clear search/selection;
opening correspondence preserves the originating mailbox/search return context.
No unread counts or route membership are inferred from sender addresses.

Relationships has a searchable, unique correspondent list, with pending incoming
decisions and existing Rust permission projections grouped separately. Permission
subtitles explicitly describe contact from that person, not outgoing authority.
Outgoing-only/unclassified correspondents have their own section. Directory,
My aliases and My approach page form a separate management section. That organizer
stays present while aliases are listed and managed in the main space.

Express lanes retains distinct private-alias and sender-lane groups, adds search,
and keeps direction/receiving-address filters without resetting the opened detail.
Economics organizes existing request records by direction and issuance month,
with name/class search and a record count. It displays Rust-supplied financial
outcomes and does not derive charges, refunds or financial totals from text.
The superseded multi-area sidebar-list renderer and projection are removed.

This is a Swift presentation change. Saved searches, additional financial
categories/totals/distributions and Notes organization are not implemented by this
pass. Existing backend permissions, recovery contracts, encrypted caches and
original account/Keychain association are unchanged.

The canonical build installed after owner signing-key approval, retaining the
original designated signing requirement. Strict signature verification passed;
installed and built executable UUIDs match
`F9C8ADBE-4E49-390F-AE2E-DFB1726EB797`. The prior app was quit and the canonical
app reopened through the original account/Keychain association, without enrollment
or credential replacement. All three requested receiving aliases and the existing
correspondence remained available.

Running macOS inspection verified the concise Relationships preview, Comms Sent
selection and correspondence round trip, a Daniel relationship search retained
through correspondence/back and alias management, and the main-space alias list
while the same relationship organizer remained visible. Economics direction and
month filters changed the record list while preserving the opened Jules record;
Express lanes direction switching likewise preserved Elena's incoming detail.
Dark appearance verified the lane organizer and contrasting light Comms preview.
A clean reopen reset assessment-only navigation filters. NYC-Hospital/Unread
retained its receiving scope and excluded Sent/Outbox; Drafts opened from B and
showed all three existing unsent drafts. Light appearance was restored and the app
was left in All inboxes/Inbox. No messages, permissions or financial mutations were
submitted during this inspection.

Validation: canonical Apple build, strict signature and executable checks, diff
checks and the affected running macOS journeys above. Build log:
`/tmp/cs-mail-stable-organizer-build.log`. Pre-pass source snapshots are under
`target/stable-organizer-baseline-20261016/` (the superseded multi-area renderer
and projection remain recoverable through Git). No new tests or unrelated Rust or
PostgreSQL suites were run. iOS, compact layouts, full VoiceOver, very large lists,
all dark-area combinations, unseen pending/outgoing-only relationship groups and
pointer dwell/dismiss timing were not exercised. This is local UI evidence, not
public mail delivery validation.

## Column B restored after layout assessment — 16 October 2026

At Marcus's request, the column-free navigation assessment was reverted. The four
changed Swift files exactly match their pre-removal snapshots in
`target/navigation-popup-baseline-20261016/`. Persistent column B, ordinary rail
click navigation, transient previews and the alias-management sidebar are restored.
The approved contrasting previews, turquoise Comms palette, continuous spill and
removed sidebar hide button remain. New utility for column B is undecided.

The canonical build installed using the original signing identity and unchanged
designated requirement. Strict signature verification passed; installed and built
executable UUIDs match `7D98BFE9-3389-3CB8-BCCC-C1B17FA7EE3B`. The earlier process
was quit and the canonical app reopened on the original account without enrollment
or credential changes. Running macOS inspection confirmed Comms/All inboxes with
persistent column B, existing correspondence, all three requested alias inboxes,
turquoise perimeter lighting and no sidebar hide control. The app was left there.
Evidence: `/tmp/cs-mail-column-b-restored-build.log`.

Validation: canonical Apple build, strict signature and executable checks, exact
source-restoration comparison, diff checks and running macOS inspection. No new
tests or unrelated Rust/PostgreSQL suites were run. iOS, compact layouts and a fresh
full dark-mode/hover matrix were not checked for this restoration.

## Contrasting navigation previews and continuous spill — 16 October 2026

The native column-B Hide Sidebar toolbar item is removed at its sidebar owner.
Column A previews now use a darker area-tinted surface in light appearance and a
lighter surface in dark appearance, with matching text/control appearance and an
explicit contrasting Close control. The preview and receiving column share one
palette: Comms uses inky turquoise, Relationships dark gold, Express lanes a
visible silver-blue and Economics olive. Main-space reading surfaces stay neutral;
non-Comms column interiors retain their top-only tint.

The twelve discrete spill bands are replaced by one blurred perimeter stroke,
clipped outside the column, using continuous shadow-style falloff. Initial spill
opacity compensates for each color's relative luminance against an approximate
appearance background. This provides a consistent starting-contrast target and
shared falloff, not a claim of perceptually exact equality on every display.
The old separate surface parameter and ring implementation are removed.

The canonical build used the installed Command Line Tools toolchain because the
selected Xcode toolchain currently reports an unaccepted license; no license or
system developer selection was changed. The original signing identity was used,
the build script verified unchanged designated requirements, and final strict
signature verification passed with access to the normal signing/trust services.
Installed and built executable UUIDs match
`0A7C6BC3-183B-3C13-9DCF-11EA93F6D0EE`. The app was quit/reopened at the canonical
path on its existing account without enrollment or Keychain changes. Existing
correspondence and the requested public/private alias inboxes remained visible.

Running macOS inspection covered turquoise, gold, silver-blue and olive lighting,
dark appearance for silver-blue/olive, dark previews in light appearance and a
light Relationships preview in dark appearance. Native shadow and earlier ring
rendering were visually compared before replacement; the new falloff is smoother,
without claiming an exact reproduction of macOS's window-shadow kernel. Final
inspection confirmed no sidebar hide item, readable preview Close, successful
Close dismissal and the original alias inboxes. Light appearance was restored and
the final app was left in All inboxes. Build evidence:
`/tmp/cs-mail-color-pass-build.log`; source snapshots before this pass are under
`target/color-pass-baseline-20261016/`.

Validation scope: canonical Apple build, strict signature verification, matching
executable UUIDs, diff checks and changed-screen inspection. No new tests or
unrelated Rust/PostgreSQL suites were run. iOS, compact layouts, full VoiceOver,
all color/appearance combinations and pointer dwell/dismiss timing were not
validated in this pass. Search and rich message formatting are separate work.

## Current top-only column tint — 16 October 2026

Visual comparison confirmed the earlier non-Comms gradient incorrectly tinted the
full column. It has been replaced by `EditorialColumnTopTint`: a vertical fade
confined to the top 120 points, ending fully transparent with no painted base or
bottom color stop. The middle and bottom retain the native sidebar background.
Relationships uses dark gold, Economics olive and Express lanes blue-grey. Comms
and column-A navigation previews remain on their original rendering paths.

The canonical build installed after Marcus approved the existing signing key.
Strict signature verification passed, the original signing requirement is retained,
and installed/built executable UUIDs match
(`38397DFF-6C72-36F4-8B91-E9D0485580EB`). After quit/reopen on the original account,
macOS screenshots were inspected for Economics, Relationships and Express lanes:
the tint is restricted to the top, with neutral middle/lower interiors. The app was
left on Economics. Evidence: `/tmp/cs-mail-column-top-tint-build.log`. Diff checks
passed. No new tests or unrelated Rust/PostgreSQL suites were run for this
presentation-only correction. Dark mode and iOS were not inspected in this pass.

## Earlier column-interior scope correction — 16 October 2026

Marcus clarified that Comms column B must remain as it was and that column-A
navigation hover boxes were outside this change. The navigation wrapper now leaves
Comms and every preview on their original rendering path, without the added
gradient, foreground override or expansion frame. Only actual non-Comms column-B
interiors receive the shared color wash. The requested dark-gold Relationships
palette is retained. This supersedes the broader preview/Comms treatment below.

The canonical build installed, retained the original designated signing requirement,
passed strict signature verification and matches the final built executable
(`72BB0374-7ED4-38E8-9668-AB9026FFBCB0`). After quit/reopen on the original account,
running visual inspection confirmed the Relationships hover preview's restored
neutral interior and Comms' restored native background with the alias scenario
intact. The app was left in Comms/All inboxes. Evidence:
`/tmp/cs-mail-column-scope-restored-build.log`. Diff checks passed. No new tests or
unrelated Rust/PostgreSQL suites were run; iOS and all dark/preview combinations
were not inspected in this correction.

## Column interior implementation, before scope correction — 16 October 2026

All column-B content now uses one shared interior gradient treatment, with its
color drawn from the same palette as perimeter lighting: Comms blue, Relationships
dark gold (`0.68, 0.46, 0.10`), Express lanes blue-grey, and Economics olive.
Relationships' former cream lighting is replaced by dark gold at Marcus's request.
Tint fades across the interior over the existing canvas, with reduced strength in
dark mode. It no longer depends on wallpaper/native-sidebar blending. The shared
navigation-column wrapper covers the inbox selector, other area lists, My aliases
and navigation previews; the old opaque list background was removed.

The canonical build installed successfully with the original designated signing
requirement. Strict signature verification passed and installed/built executable
UUIDs match (`EFE0F3BE-83EF-39E3-9DD2-67152CD912C1`). The app was quit/reopened on the
preserved owner account and alias scenario. Running macOS visual inspection covered
all four area interiors in light mode, My aliases, the gold Relationships preview,
and a Relationships dark-mode readability check. Light appearance was restored and
the app was left on Relationships. No account, permission or correspondence records
were intentionally changed. Evidence: `/tmp/cs-mail-column-interior-final-build.log`.

Validation scope was the native build, signature checks, diff checks and changed
screen inspection. No new tests or unrelated Rust/PostgreSQL suites were run for
this presentation-only change. iOS, compact layouts, every preview and every dark
area combination were not manually inspected.

## Current owner alias scenario — 16 October 2026

At Marcus's explicit request, the existing owner account now displays protocol
address `mtallis62831@c-sqdmail.com`, public alias `marcus.thomas@c-sqdmail.com`,
and two active private aliases: `mat7771@c-sqdmail.com` (NYC-Hospital) and
`m11a33t@c-sqdmail.com` (Brooklyn-Lab-Co). Addresses use the existing lowercase
canonicalization. The private routes were explicitly supplied local development
fixtures, constructed through the checked AliasPortfolio domain contract; ordinary
private-alias creation continues generating addresses. No alternate member app,
owner account, client adapter or runtime mode was introduced.

The current launcher is `target/alias-assessment-owner-20261016/start.sh`, at the
existing 9693 endpoint, against `cs_mail_alias_assessment_owner_20261016`. It uses
the pinned existing server binary, original deployment keys and unchanged account,
persona and registered operational/content keys. The prior schema-40 source and
`target/alias-inboxes-owner-20261016/start.sh` are retained offline as a checkpoint;
do not revert to them after new activity on the current copy. The directory/SMTP
namespace in the current configuration is `c-sqdmail.com`; this is local configuration,
not DNS/MX ownership or live-provider delivery.

The explicit offline tool and wrapper in `tools/development-continuity/` preserved
the source database, recorded the old directory entry and scenario plan, and changed
only the owner directory mailbox/revision and three checked alias records in one
transaction. No signed historical command or receipt was rewritten or fabricated.
112 other table/sequence fingerprints and every configured key file were identical.
Both rehearsal and owner-copy checks passed rollback after all writes and exact
retry without duplicates. The original encrypted device store was backed up and
confirmed unchanged before reopening. Neither tool accesses or writes Keychain.

The existing signed canonical app reopened through its original Keychain identity
without enrollment. Running inspection verified all three inboxes, each private
lane's exact address and Active status, inbox-to-permissions and lane-to-inbox links,
the public-facing alias in My aliases, and the requested replacement address in
My approach page. Existing correspondence remains in All inboxes; the new inboxes
are empty, with no invented historical routing or sample messages. The app was left
in All inboxes with all three alias selections visible.

Validation: maintenance binary build, targeted clippy with warnings denied,
PostgreSQL preservation/rollback/retry experiments and running macOS inspection.
Evidence: `/tmp/cs-mail-alias-assessment-check.log`,
`/tmp/cs-mail-alias-assessment-owner.log`, and the current deployment's
`verified.json`/`scenario-plan.json`. No product implementation, schema or Swift
presentation code changed, so no new app build or unrelated full quality gate was
needed. Populated alias correspondence, iOS and live mail delivery were not checked
in this fixture pass. Alias-aware public approach-page publication remains unfinished:
My approach page still uses the existing protocol-address directory publication.

## Alias inboxes and receiving evidence — 16 October 2026

Comms column B now selects All inboxes or an alias inbox, using owner-only purpose
labels, unread counts and a historical-inbox section. The main space browses and
reads that selection; search binds to the same scope. A conversation can appear
in several inboxes while retaining one identity. Alias readers show and acknowledge
only matching received messages, with an explicit full-conversation action. All
inboxes retains earlier mail with no recorded alias; no sender/address heuristic
backfills membership. Conversation-wide organization controls remain in All
inboxes; custom unions and independent per-alias organization are later work.
Express lanes separates People and organizations from Private aliases, exposes
explicit receiving scopes and connects alias permission details with its inbox.

Rust's application layer owns historical DeliveredRoute facts and coordinates
explicit signed native alias targets or authenticated SMTP envelope addresses
with the existing deterministic alias admission owner. Storage commits receiving
evidence with the message, copies, conversation, allowance effects and terminal
receipt. Identity-addressed delivery stays distinct and gains no inferred alias.
Recipient account activity and both billing records are protected inputs; service
coverage is evaluated at the application's decision timestamp. The portable client
checks receiving evidence against incoming headers, derives inbox/count projections
without fetching message content, and binds retained-text search and read/unread
actions to an explicit scope. There is no inbox aggregate in the request kernel.

The existing native journey passed receiving through two private aliases in one
conversation, separate unread counts, rejection of a cross-scope read request,
scoped search and cursor binding, retirement history, refused retired delivery,
and cross-alias blocking. Existing request, recovery, organization, deletion,
source-reuse and external journeys also passed. A pre-existing funding experiment
now accepts an immediate definite refusal from a fast simulator while still
requiring exactly voided funding and no delivery. No test cases were added.
The existing real SMTP experiment passed misleading To-header routing, interrupted
message/evidence rollback, a new authenticated guest, canonical address case,
exact redelivery after retirement and refusal of fresh mail after retirement.
Evidence: `/tmp/cs-mail-alias-inboxes-native-checks-complete.log` and
`/tmp/cs-mail-alias-inboxes-smtp-test.log`. The full local quality gate passed: identity pins, dependency direction, formatting,
workspace and production lint, workspace tests, PostgreSQL suites and application
binaries. Evidence: `/tmp/cs-mail-alias-inboxes-quality-complete.log`.

The schema-39 owner service was offline when checked. The explicit upgrade created
a fresh schema-40 copy and preserved all 114 original table/sequence fingerprints,
original migration history and configured key files. No owner Keychain credential,
account association, address or local v3 cache was replaced. The current pinned
owner launcher is `target/alias-inboxes-owner-20261016/start.sh`, with
`owner-ready/verified.json` and `server.sha256`, serving the original 9693 endpoint.
The original schema-39 database and launcher remain the rollback checkpoint.
The existing app reconnected and refreshed successfully through its original
credential. Evidence: `/tmp/cs-mail-alias-inboxes-preservation.log`.

The final signed Apple build installed at the canonical path with the original
designated signing requirement. Strict signature verification passed and the
installed executable's UUID matches the final build. The app was quit and reopened
on the original account. Running-app inspection verified Inboxes/All inboxes,
Jules correspondence and its return path, Manage aliases and its return path,
Express lanes' separate sections, Elena's sender-lane details with explicitly
unrecorded alias scope, and lane-to-correspondence-to-lane navigation. The owner
profile still shows marcus@csqd-mail.com. No owner profile was published, alias
created, message sent, permission changed or retained copy removed. The owner has
no managed aliases; populated alias rows/readers were exercised through native
contracts rather than invented in the owner account. The app was left in All
inboxes. Evidence: `/tmp/cs-mail-alias-inboxes-apple-final.log`.

Current limits: this establishes local receiving contracts, not public MX or
SMTP2GO delivery. Native alias-send contracts require an explicit known participant
and alias target; normal address resolution/composer selection and outbound/reply
alias addressing remain separate work. Public alias approach-page publication and
held-mail release are not implemented here. A standalone MX host needs the explicit
optional registration capability to open previously unknown guest pairs. Inbox
routing observations are session-local; durable offline alias metadata, very large
mailbox performance, iOS, dark/compact layouts and full VoiceOver assessment are
not established by these checks.

## Navigation context and protected edits — 15 October 2026

Correspondence now returns to its actual origin: retained search results, the
selected mailbox, Relationships map or Directory profile. Mailbox query, sorting,
selection and stable scroll anchors live in one presentation navigation owner;
request and lane direction also survive a correspondence round trip. The toolbar
Inbox and Communications root still explicitly return to a fresh Inbox. Alias
management and lane review use the same navigation history.

My approach page uses one session-local editor draft. Leaving dirty fields offers
Save changes and continue, Discard changes and continue, or Stay. Saving continues
only after confirmed publication; a failed save keeps the editor open. Checking a
saved publication does not overwrite newer dirty fields. Explicit Lock uses the
same guard. This draft is not durable across quit or automatic session locking.

Inactive readers are unmounted and the model requires the matching visible reader
context before recording read identities. Contact selection records the latest
intent during an operation and loads it after the operation completes; stale loads
cannot replace the selected contact. Late Directory lookups respect newer
navigation intent. These changes remain in the Swift adapter; protocol and Rust
domain ownership are unchanged.

The final canonical signed build was installed, quit and reopened on the existing
owner account, retaining marcus@csqd-mail.com and the original designated signing
requirement. Strict signature verification passed. Running-app inspection verified
search → Daniel → Search results with the query/results retained, Sent → Jules →
Sent, Relationships → Jules → Relationships map, and Directory → Daniel profile →
correspondence → the same profile. A temporary local display-name edit triggered
the exit dialog; Stay retained it. The original text was restored without
publishing, and the app was left in Inbox.

The existing native integration journey was extended to challenge two conjectures:
an inactive reader must not mark messages read, and the latest contact selection
must survive an overlapping refresh. Both passed, as did the existing isolated
local request, native and external journeys. No test cases were added. Evidence:
`/tmp/cs-mail-navigation-native-checks.log` and
`/tmp/cs-mail-navigation-final-build.log`. Diff checks passed. No unrelated Rust
or PostgreSQL suites were run. iOS, compact/dark layouts, long-list scroll precision,
and Save/Discard against the owner profile were not manually exercised. These
checks establish local navigation behavior, not public email delivery.

## Explicit Inbox return — 15 October 2026

Opened native correspondence now has an explicit Inbox back link in Communications;
the toolbar shows a labeled Inbox action. Selecting Communications clears the
selected contact and returns to Inbox instead of keeping that contact open.
External correspondence has the same explicit return action. Returning clears
the open thread/message focus and resets the mailbox view to Inbox. Other areas
retain their existing Home return behavior. This changes navigation only.

The canonical signed build installed with its original signing requirement and
was quit/reopened on the existing account. Native inspection opened Jules Morgan
and verified all three return paths: the conversation's Inbox back link, the
Communications navigation item and the toolbar Inbox action. Each returned to
Your mail with Inbox selected; the app was left there. Evidence:
`/tmp/cs-mail-inbox-return-build.log`. Diff checks passed. No new tests or unrelated
Rust/PostgreSQL suites were run for this presentation-only change. External and
iOS navigation were not manually exercised.

## Reading, search, organization and outgoing work — 15 October 2026

Communications now opens Your mail, with Inbox, Unread, Flagged, Archive, Trash,
Sent and Outbox. The list uses message activity with stable identity tie-breaking;
A–Z is available. Personal read identities, flags and folder placement are encrypted
on this device. Reading marks only the represented message snapshot; synchronization
does not mark mail read and no sender-visible receipt is produced. Manual unread,
archive/restore and reversible Trash have no permission or retention effects.
Selections support bulk archive, flag, Trash and restore. Native permanent copy
removal is separately confirmed and journaled through DeleteCopies. External
permanent deletion is not offered.

Readers separate messages by sender and service commit time and render structured
text, retained inline images, quotations, shared copies and inert references.
Unavailable and access-denied native copies have distinct projections. Find within
a conversation, Latest and per-message Reply sit alongside existing request and
relationship controls. Search retained message text opens authenticated accessible
copies in bounded pages, reports searched/unavailable/remaining coverage and opens
the matching conversation/message. It never follows references and has no durable
plaintext index. Session locking clears transient results and the Rust search scope.
The sidebar's separate field is explicitly labeled Filter contacts.

Refresh observes mail without resuming mutations, reports incomplete native/external
observations, and rolls back a failed native page's cursor and headers together.
Swift refreshes on foreground activation and periodically while open. The last
complete refresh time remains visible. Existing explicit recovery controls retain
their original operation identities. Sent shows conversations containing committed
outgoing messages. Outbox exposes submission history recorded from this version
onward: unknown, native mailbox committed, external queued or refused. The encrypted
history retains the exact command, originating draft revision and terminal evidence
in the same atomic transaction as the cache and draft workspace. Relay handoff and
provider-reported observations cross FFI as distinct typed facts. Older history is
not fabricated; previous committed mail remains in Sent.

Validation extended existing experiments without adding test cases. Six portable
client checks passed, including later-arrival read races, malformed personal-state
restoration, failed organization writes, no implicit submission retry on refresh,
failed later mailbox pages, search-scope clearing, refusal/unknown outcome history
and exact-command replay. Strict client/FFI lint, identity pins, formatting and
diff checks passed. The native SQLite/FFI journey against isolated local PostgreSQL
services passed third-record transaction rollback, request/native/external journeys,
personal organization and retained-text search. Fixture-only permanent removal
made the caller's copy unavailable while the peer retained its readable copy.
Evidence: `/tmp/cs-mail-mailbox-tests-final.log`,
`/tmp/cs-mail-mailbox-lint-final.log`, `/tmp/cs-mail-mailbox-native-checks.log`,
`/tmp/cs-mail-mailbox-identity.log` and `/tmp/cs-mail-mailbox-apple-final.log`.

The first canonical build installed with the original signing requirement and
was quit/reopened on the preserved owner account. Native light-appearance inspection
found the previously missed older text through search, opened the matching message,
checked individual timestamps, marked a snapshot unread, archived/restored a
conversation to Inbox, and inspected Sent and initial empty Outbox. No owner
message was sent, charge authorized or retained copy permanently removed. Locking
cleared search results and open mail; reopening retained the original account and
`marcus@csqd-mail.com`. The mailbox workspace was left open. The isolated test
services were stopped after validation. Following the second macOS signing
authorization, the final build containing the native-page rollback refinement
installed at the canonical path with the original signing requirement. The app
was quit/reopened; its installed executable's build UUID matches the final build,
and its signature verifies strictly. Native
inspection confirmed the mailbox workspace, persisted unread/folder state and all
three earlier unsent drafts. The final mailbox workspace is running and left open.

The full quality gate and unrelated server suites were not run. iOS, dark/compact
layouts, full VoiceOver assessment, actual power loss, very large mailboxes,
cross-device personal state and live/public-provider delivery were not exercised.
Populated outgoing history and external rendering were exercised through native
contracts; the owner account's Outbox is empty and its external inbox unpopulated.
No new provider-event ordering experiment was run; existing server delivery rules
are unchanged. Subjects, general attachments and complete offline content search
remain separate work.

## Durable drafts and submission handoff — 15 October 2026

The member application now autosaves native messages, external emails, approach
requests and follow-ups in an encrypted device-local workspace. Communications
offers a Drafts list that reopens the selected writing by its exact identity.
New emails and new native conversations create independent drafts; replies resume
their existing context. Composers show saving status, retain writing after refusal,
offer explicit save/close and discard choices, and keep disclosure consent and
charge approval separate from saved writing. Local saving does not wait for the
network operation queue. Drafts are local to this device; they are not synced.

Rust's client workspace owns the deterministic draft lifecycle, checked identities,
revisions and restoration. The protocol kernel does not own drafts. Submission
atomically moves an exact draft revision into the existing recovery journal;
uncertain results retain that operation for exact retry, while a confirmed refusal
restores editable writing. SQLite commits workspace and journal together. Existing
account encryption and the original Keychain credential association are retained.

Validation extended existing experiments, without adding test cases: six portable
client recovery checks passed, including stale revisions, independent drafts,
local saves during network work, invalid restoration, atomic handoff failure,
refusal restoration and exact retry after reopen. The native SQLite/FFI journey
passed against isolated local PostgreSQL services, including second-write rollback,
request-draft reopen before charge approval, native and external submission, reuse
policy, consent, refusal and recovery. Strict client/FFI lint, formatting, identity
source pins and diff checks passed. Evidence:
`/tmp/cs-mail-drafts-tests-final.log`, `/tmp/cs-mail-drafts-lint-final.log`,
`/tmp/cs-mail-drafts-native-checks.log`, `/tmp/cs-mail-drafts-identity.log` and
`/tmp/cs-mail-drafts-apple-build.log`.

After macOS signing authorization, the canonical build installed successfully with
the original signing requirement. The app was quit and reopened at
`target/apple/CsMail-continuity-20261012.app`; native inspection confirmed the
existing owner address `marcus@csqd-mail.com` and existing relationships. Two
independent external drafts retained their distinct recipients and bodies, and a
native reply retained its writing across a complete app restart. Native inspection
also checked save status, the close/save choices and discard confirmation/cancel;
it did not execute permanent discard. The three clearly labeled usability-check
drafts remain unsent in Drafts for assessment. No owner message was sent or charge
authorized. This canonical build also includes the preceding relationship-map
spacing refinement, superseding its historical signing wait below.

The full quality gate and unrelated suites were not run. iOS, dark/compact draft
layouts, actual power loss and cross-device recovery were not exercised. Native
contract checks covered request/follow-up submission; those composer screens were
not manually inspected on the owner account. Local simulation establishes no
SMTP2Go/public delivery. Unread flags and broader mailbox organization remain
subsequent slices.

## Relationships overview map — 14 October 2026

The Relationships overview replaces Choose a request with a static incoming
relationship map centered on You. It reuses the Rust-supplied permission summaries
already used by Home: standing permission has a pale gold solid line, express
lanes a cool silver dashed line, and history without standing permission a faint
dotted line. Each person retains their supplied permission subtitle and opens the
existing correspondence view. Closed-route history is not an active permission;
there are no inferred connections between contacts or invented alias branches.
Swift owns only arrangement, line styling and navigation. Nodes use generous
click regions and perimeter-only hover feedback, with no continuous animation.
The map scrolls as needed and uses four fixed positions per ring to make room for
labels. Receiving-route grouping and an outgoing map remain subsequent work.

The first canonical signed build and diff checks passed. Installation retained
the original signing requirement; the app was quit/reopened on the existing owner
account. Native inspection checked the four-contact map in light appearance and
confirmed that Daniel's node opens Daniel's correspondence. The map was left open.
No account or relationship data was changed. Evidence:
`/tmp/cs-mail-relationship-map-build.log`. A subsequent refinement aligns the
node perimeter with its hover outline and limits each ring to four fixed positions.
It compiled successfully (`/tmp/cs-mail-relationship-map-layout-build.log`), but
its final canonical installation is waiting at macOS signing authorization
(`/tmp/cs-mail-relationship-map-final-build.log`). The running first build uses
the same four-contact arrangement; the refinement is not yet installed.
No tests or unrelated Rust/PostgreSQL suites were run for this presentation-only
change. Empty and multi-ring layouts, pointer-hover rendering, dark appearance,
compact layout and iOS were not visually exercised.

## Generated private aliases — 14 October 2026

New Alias now separates private purpose/description from public email-address
choice. Private creation asks for owner-visible context and Rust generates an
unpredictable 128-bit address. The list and alias-lane detail show the description
prominently; Copy address copies only the address. Confirmed creation selects the
alias ID from the verified outcome. Private replacement generates another address
and preserves the description. Public creation uses a chosen address; a current
public alias instead offers Manage public alias, where replacement is available.
Rust owns the at-most-one-current-public-alias rule, including concurrent requests;
Swift presents its projection and never supplies a private mailbox candidate.

Signed management uses version-2 commands/acknowledgements and schema 39, without
old-format runtime readers. The explicit offline creation upgrade refuses populated
legacy alias metadata; it does not invent descriptions or rewrite signed history.
The owner source had no alias records or commands. The preservation tool restored
a fresh copy and verified all 114 original table/sequence fingerprints, original
migration history and configured key files. Original account association and
`marcus@csqd-mail.com` were retained; no Keychain credential was replaced or
reenrolled. The schema-38 source and launcher remain the rollback checkpoint.
The current pinned owner launcher is `target/alias-creation-owner-20261014/start.sh`,
with its checked server hash and `owner-ready/verified.json`, serving the same 9693
endpoint. Evidence: `/tmp/cs-mail-alias-creation-preservation.log`.

Validation passed: scoped Rust check; the existing domain restoration and portable
client recovery experiments; the existing PostgreSQL lifecycle/replay experiment,
extended to race different public addresses for one owner and replace the winner;
strict lint for the six affected Rust packages with all targets/features; identity
source pins; formatting/diff checks; canonical signed Apple build; and the existing
native enrollment/alias experiment against a fresh local simulation. These challenge
invalid restoration, encrypted description storage, exact retries, atomic failure,
public-alias cardinality, permanent reservation, Directory isolation and account/
credential continuity. Existing experiments were adapted; no test cases were added.
Evidence: `/tmp/cs-mail-alias-restoration-tests.log`,
`/tmp/cs-mail-alias-recovery-tests.log`, `/tmp/cs-mail-alias-creation-postgres-final.log`,
`/tmp/cs-mail-alias-creation-lint-final.log`,
`/tmp/cs-mail-alias-creation-native-checks.log` and
`/tmp/cs-mail-alias-creation-apple-build.log`.

The canonical app was installed with its original signing requirement and
quit/reopened on the preserved owner deployment. Native inspection checked both
creation types, private draft enablement, Cancel and a fresh empty dialog. No
owner alias was created (both alias routes and command counts remain zero).
Populated alias screens, the existing-public-alias navigation branch, clipboard
interaction, dark appearance and iOS were not visually exercised; native contract
checks covered populated projections. The full quality gate and unrelated suites
were not run. Alias mail delivery/publication and public-provider delivery remain
outside this management change; local simulation is not evidence of live delivery.

## Express lanes navigation icon — 14 October 2026

Column A uses two upward arrows side by side for Express lanes, composed from
matching native symbols. Its label, click region and navigation meaning are
unchanged. The canonical signed build installed with the original signing
requirement and was quit/reopened; the paired arrows were visually checked in
light appearance. Evidence: `/tmp/cs-mail-express-arrows-build.log`. Diff checks
passed. No tests or unrelated Rust/PostgreSQL suites were run for this icon-only
change; compact layout, dark appearance and iOS were not visually inspected.

## Matching rail and top-row canvas — 14 October 2026

Column A now explicitly draws the shared canvas instead of inheriting it. The
root canvas extends through the top safe area, and the macOS toolbar's separate
background is hidden so the top row uses that same color: `#F7F6F2` in light
appearance. The main space retains the same canvas. Column B's native container,
toolbar cap, perimeter lighting and spacing remain in place. An opaque toolbar
fill was removed during inspection because it covered the panel's top edge.

The canonical signed build and diff checks passed. The app was installed with
its original signing requirement, quit/reopened and inspected on Communications
in light appearance: the top row, rail and main-space backgrounds match and the
panel's full illuminated perimeter remains visible. Evidence:
`/tmp/cs-mail-unified-canvas-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; dark appearance, compact layout and iOS were not inspected.

## Column B colors by area — 14 October 2026

Column B now uses pale gold for Relationships (including its address and profile
destinations), cool light-gray silver for Express lanes, electric blue for
Communications and muted olive green for Economics. One presentation-owned palette
selects the spill from the current navigation area. Communications, Relationships
and Economics use a one-point rim in a darker shade of their wash color (65% of
each RGB component, drawn at 55% opacity). Express lanes uses a white rim at
32% opacity, as do contextually lit navigation previews. All cove rims now share
one-point thickness. Each rim is centered on the panel outline,
with color immediately adjacent to its outer edge. The wash starts at 13%
opacity and fades quadratically over twelve
points. The drawing surface extends fifteen points beyond its layout bounds
on every side so the left spill has the same drawing room as the right.
The earlier six-point near-white transition was removed
because it made the colored spill appear detached from the rim.
The native panel, toolbar cap, shadow, narrower column A and gutter are retained.
Home and Notes have no column B. Navigation previews use their area's cove
lighting only when column B is absent; otherwise they retain their original
border and shadow. The member view passes column visibility explicitly to the
navigation shell, including the sidebar's show/hide state. Preview lighting
reuses `EditorialCoveLight`, with no separate palette or falloff implementation.

The preview wash starts at 20% opacity rather than column B's 13%, so the muted
colors remain discernible beside its white rim on the shared ivory canvas.
The twelve-point quadratic fade and visibility condition are retained. The
canonical signed build and diff checks passed, installation retained the original
signing requirement, and the app was quit/reopened. The Economics preview over
Home was visually checked and left open. Evidence:
`/tmp/cs-mail-preview-wash-visibility-build.log`. No tests or unrelated
Rust/PostgreSQL suites were run; the other palettes, dark appearance and iOS
were not visually rechecked for this opacity adjustment.

The white rims were subsequently reduced from three points to one, matching the
tonal rims. Their colors, opacity, wash falloff and contextual preview behavior
are unchanged. The canonical signed build and diff checks passed, installation
retained the original signing requirement, and the app was quit/reopened.
The Economics preview over Home and Express lanes column B were visually checked
in light appearance. Evidence: `/tmp/cs-mail-uniform-rim-build.log`. No tests or
unrelated Rust/PostgreSQL suites were run; dark appearance and iOS were not checked.

The conditional treatment passed the canonical signed build and diff checks,
was installed with the original signing requirement and quit/reopened, and was
visually checked in light appearance: cove previews over Home and Notes, original
preview shading over a visible Communications column B. Evidence:
`/tmp/cs-mail-contextual-preview-light-build.log`. No tests or unrelated
Rust/PostgreSQL suites were run. Manually hidden-sidebar behavior, dark appearance,
compact layout and iOS were not visually exercised in this pass.

The thinner tonal column B rims passed the canonical signed build and diff
checks, were installed with the original signing requirement and quit/reopened,
and all four area screens were visually inspected in light appearance, including
the preserved Express lanes white rim. Evidence: `/tmp/cs-mail-tonal-rim-build.log`.
The native containers, panel positions and contextual preview behavior are
unchanged. No tests or unrelated Rust/PostgreSQL suites were run; dark appearance,
compact layout, preview rims and iOS were not visually rechecked for this pass.

The earlier unconditional preview-lighting experiment was removed: previews retained their original
border and shading, without a cove rim or colored wash. The restoration passed
the canonical signed build, was installed and quit/reopened, and the Communications
preview was visually checked and dismissed with Escape. Evidence:
`/tmp/cs-mail-restore-preview-shading-build.log`.

Economics was then toned down from neon green to olive (RGB 0.55, 0.62, 0.30).
Its white rim and wash strength/falloff remain unchanged. Lighting now has one
presentation owner in `EditorialCoveLight`; the former private implementation was
removed. The canonical signed build and diff checks passed, installation retained
the original signing requirement, and the reopened Economics screen was visually
checked in light appearance. Evidence: `/tmp/cs-mail-olive-glow-build.log`.
No tests or unrelated Rust/PostgreSQL suites were run; dark appearance, compact
layout and iOS were not exercised for these revisions.

The canonical signed build passed, preserved the original signing requirement,
was installed and quit/reopened, and all four area screens were visually checked
in light appearance. Express lanes is left open. Evidence:
`/tmp/cs-mail-area-glow-build.log`. Diff checks passed. No tests were added or
unrelated Rust/PostgreSQL suites run. Dark appearance, compact layout and iOS
were not visually exercised; no account, credential or domain state was changed.

The rim is now one three-point stroke on the outline rather than three filled
bands. Its outer edge defines the colored wash's origin. The canvas was expanded
without moving either column to remove left-side clipping. The canonical signed
build passed, retained the original signing requirement, was installed and
quit/reopened, and both verticals, rim and corners were visually checked on
Communications in light appearance. Evidence:
`/tmp/cs-mail-perimeter-rim-build.log`. Diff checks passed. No tests or unrelated
Rust/PostgreSQL suites were run; other palettes, dark appearance and iOS were
not visually rechecked for this shared geometry correction.

The wash was subsequently softened from roughly 24.5% opacity at the rim to
13%, and shortened from a 24-point total spread to 15 points with a faster fade.
The white rim, palette assignments, native panel and restored column spacing are
retained. The canonical signed build and diff checks passed; the app was
installed with its original signing requirement, quit/reopened and visually
checked on Communications in light appearance. Evidence:
`/tmp/cs-mail-softer-wash-build.log`. No tests or unrelated Rust/PostgreSQL suites
were run. Other palettes, dark appearance and iOS were not visually rechecked.

The adjacency correction passed the canonical signed build and diff checks,
was installed with the original signing requirement and quit/reopened, and was
visually checked on Communications in light appearance. The blue spill now
touches the white rim around the panel. Evidence:
`/tmp/cs-mail-adjacent-glow-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run. The other three palettes, dark appearance and iOS were not
visually rechecked for this shared boundary correction.

## Narrower column A — 14 October 2026

The vertical navigation rail is 64.5 points wide, three quarters of its former
86 points. Buttons retain their 64-point height and use the available rail width.
Relationships scales modestly to remain on one line; Express lanes wraps onto
two lines. An additional 21.5-point space after the rail preserves column B's
original horizontal position. Preview positioning includes that space as well.
The compact horizontal rail is unchanged.

The vertical divider now stops above the window bottom, with its bottom inset
matching the top safe-area inset. Column widths, gap and panel placement are
unchanged; the compact horizontal divider retains its existing layout. The
canonical signed build and diff checks passed, the app was installed with its
original signing requirement and quit/reopened, and the divider endpoints were
visually checked on Express lanes in light appearance. Evidence:
`/tmp/cs-mail-divider-inset-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; dark appearance, compact layout and iOS were not checked.

The initial implementation incorrectly moved column B left with column A. The
spacing correction restores its original position while keeping column A narrow.
The canonical signed build passed, retained the original signing requirement,
was installed and quit/reopened, and the larger gap and unchanged native column B
were visually checked on Communications. Evidence:
`/tmp/cs-mail-column-gap-build.log`. Diff checks passed. No tests or unrelated
Rust/PostgreSQL suites were run; previews, dark appearance, compact layout and
iOS were not rechecked for this spacing correction.

The canonical signed build passed, retained the original signing requirement,
was installed and quit/reopened, and was inspected on Express lanes. The native
column B container, pale-gold glow and inter-column gutter were preserved. The
Relationships preview was opened through its accessibility action, checked for
placement and dismissed with Escape. Evidence: `/tmp/cs-mail-narrow-rail-build.log`.
Diff checks passed. No tests or unrelated Rust/PostgreSQL suites were run;
pointer-hover dismissal, compact layout, dark appearance and iOS were not
exercised in this pass.

## Cooler ivory canvas — 14 October 2026

The shared light-appearance canvas is now `#F7F6F2`, replacing `#F7F6EF` across
column A, column B, the main space and navigation previews. The silver lighting
and dark palette are unchanged. The canonical signed build passed with the
original signing requirement, was installed and quit/reopened, and the light
Express lanes screen was visually inspected. Evidence:
`/tmp/cs-mail-cool-ivory-build.log`. Diff checks passed. No new tests or unrelated
Rust/PostgreSQL suites were run for this color-only change; iOS was not inspected.

## Column B perimeter-lighting experiment — 14 October 2026

The earlier Express lanes trial combined a thicker white edge reflection
with a very pale gold spill around column B's whole rounded perimeter. The
white rim holds for two points before a six-point blend into the spill; the
24-point outward falloff and opacity are retained. Its interior remains untouched. Adjacent outward bands
follow the measured panel bounds and include the native toolbar cap with its
eight-point top gutter. The former flat silvery-gold tint, right-edge-only gradient
and rejected Daniel portal trial are removed. The decoration accepts no input,
adds no accessibility content and has no animation. This remains a local visual
experiment, not an adopted toolkit rule.

The thicker white/pale-gold revision passed the canonical signed build, was
installed with the original signing requirement and quit/reopened, and was
visually inspected on Express lanes in light appearance. The native panel shape,
sizing, toolbar cap and shadow are unchanged. Evidence:
`/tmp/cs-mail-pale-gold-rim-build.log`. Diff checks passed. No tests were added
or unrelated Rust/PostgreSQL suites run; dark appearance and iOS were not inspected.

The attempted shadow-free replacement panel was rejected and removed. The
original native column B container, toolbar cap and sizing are restored, with
the six-point silver reflection and lighter spill retained. The restoration
passed the canonical signed build, preserved the signing requirement, was
installed and quit/reopened, and was visually checked on Express lanes.
Evidence: `/tmp/cs-mail-restore-column-build.log`. The native shadow remains.
No automated tests or unrelated Rust/PostgreSQL suites were run for this
presentation-only restoration; dark appearance and iOS were not inspected.

The canonical signed build passed, preserved the original signing requirement,
and was installed at `target/apple/CsMail-continuity-20261012.app`. The prior
process was quit and the app reopened. All four illuminated edges and rounded
corners were inspected in light appearance on the running Express lanes screen.
No account, credential or domain state was changed. Evidence:
`/tmp/cs-mail-silver-rim-build.log`. Diff checks passed. No tests were added
or unrelated Rust/PostgreSQL suites run. Dark appearance, alternate window sizes
and iOS were not visually exercised in this pass.

## Visual simplicity pass — 14 October 2026

The [Swift design principles](../../docs/swift-design-principles.md) now guide
native presentation. Express lanes and My addresses no longer show independent
reload buttons or observation timestamps. Alias metadata loads on entry and
foreground return, and refreshes every minute while the workspace is active.
The single toolbar Refresh targets the current workspace. Address changes still
require a current Rust projection; failed reads retain a concise notice. Read
cancellation preserves the active session and committed navigation, while
submitted changes retain their existing journal/recovery semantics.

Directory has one name/address/link input, with exact references validated by
the existing Rust lookup contract and textual searches using the listed-results
contract. The duplicate toolbar refresh is removed. My approach page uses Save
changes and shows recovery only when a saved publication is pending. Refresh
preserves unsaved edits; a changed saved revision requires an explicit discard
choice before replacement. Home replaces generic scope/validity qualifications
with the projected lane expiry. List dates are shorter; lane details retain
exact expiry, purpose and message allowance.

Creation, alias lifecycle, composer and recovery copy is shorter. Preserved
conversations use available correspondent names and derived display labels instead
of raw identifiers. The Rust financial presentation uses plain charge/refund
labels without changing amounts or lifecycle decisions. Development amounts
remain explicitly simulated units: the current contracts do not supply a
currency code and decimal scale, so no currency conversion is inferred.
Disclosure consent, charge authorization, permanent address reservation,
permission limits and unfinished delivery remain visible where relevant.

The canonical app was signed with its original requirement, installed at
`target/apple/CsMail-continuity-20261012.app`, quit/reopened and inspected against
the preserved owner account. Home, lane grouping/details, My addresses, both
creation purposes, approach-page controls, Directory search/exact lookup,
financial details, and empty native/external composers were inspected. Rapid
lane-to-communications navigation retained available composing controls without
a manual refresh. Inspection drafts were closed; no messages, address creations,
profile changes or payment authorizations were submitted. Marcus's original
`marcus@csqd-mail.com` publication and account/credential association remain in
place. The existing pinned owner backend was restarted without a database or
configuration cutover.

Six existing client tests, formatting/diff checks and the final signed native
build passed. Evidence: `/tmp/cs-mail-visual-cleanup-client-tests.log`,
`/tmp/cs-mail-visual-cleanup-final-build.log` and
`/tmp/cs-mail-visual-cleanup-owner.log`. No automated tests were added. Unresolved
publication/address recovery, populated alias details, deliberately delayed
read cancellation, iOS and public-provider delivery were not exercised in this
pass. Unrelated PostgreSQL suites and the full quality gate were not run.

## Express lane groups — 14 October 2026

Column B now separates Alias-scoped express lanes from Sender-scoped express
lanes. Incoming/Outgoing belongs only to the sender group. Sender details retain
their scope, expiry and rate; alias rows and details show the Rust-projected
lifecycle and link to the existing address manager. Rust classifies aliases from
their checked grant, excludes public addresses, and retains closed lifecycle
states. Swift never treats an alias identifier as a sender identity.

The canonical signed app was installed with its original signing requirement,
quit/reopened, and inspected against the preserved owner account. Both headings,
the empty alias group, incoming sender details and outgoing switching were
verified in the running app. It is left on Incoming with Elena's sender lane
selected. No owner test aliases or credentials were created. Populated alias
rows/details were not visually exercised in the owner account, which has no
additional addresses; classification and closed states were exercised through
the real isolated native bridge experiment.

Six client tests, strict client/FFI lint, formatting/diff checks, the signed native
build and the existing scoped native enrollment experiment passed. The experiment
was extended to challenge public-address misclassification and loss of closed
alias lifecycle state; no test cases were added. Evidence:
`/tmp/cs-mail-lane-groups-client-tests.log`,
`/tmp/cs-mail-lane-groups-lint.log`,
`/tmp/cs-mail-lane-groups-native-checks.log` and
`/tmp/cs-mail-lane-groups-build.log`. No schema or remote API change was needed.
Unrelated PostgreSQL suites, the full quality gate, iOS and public-provider
delivery were not run. Additional-alias mail delivery remains pending.

## New Alias creation dialog — 14 October 2026

A subsequent terminology pass changes the Relationships entry and workspace to
My aliases, and aligns empty states, private/public purpose descriptions,
alias-scoped lane details, lifecycle confirmations and pending-update recovery
with alias wording. Email address remains the label for the actual mailbox value;
physical-address, Directory and external-recipient wording retain their meanings.
This pass changes presentation copy only, with no new domain rules or tests.

The canonical signed build passed and installed with the original signing
requirement. After quit/reopen, native inspection checked the Relationships entry,
My aliases workspace and both private/public-facing creation descriptions; the
empty public-facing dialog was left open. No alias was created. Evidence:
`/tmp/cs-mail-alias-terminology-build.log`. Diff checks passed. Populated lifecycle
states and mutation/recovery outcomes were reviewed in source but not exercised;
dark appearance, iOS and unrelated Rust/PostgreSQL suites were not run.

The creation control and dialog title now read New Alias, with Create alias and
matching progress/outcome wording. The permanent-reservation/incomplete-delivery
paragraph was removed from this dialog at the owner's request. Purpose selection,
Rust validation and creation/recovery semantics are unchanged. The canonical
signed build and diff checks passed, installation retained the original signing
requirement, and the app was quit/reopened. The New Alias button and private
creation dialog were visually inspected; no alias was created. Evidence:
`/tmp/cs-mail-new-alias-copy-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; mutation outcomes, public-facing selection, dark appearance
and iOS were not exercised for this copy-only change.

New address opens a dedicated creation sheet with Full address, purpose, Create
address and Cancel. Creation is no longer an inline navigation destination or a
focus event. Cancelling dismisses the draft without submitting it. A confirmed
creation opens the verified record; an unknown outcome retains the original Rust
journal and directs the member to Resume update. Closing after an attempt is
labelled Close, not cancellation of the operation. Lock dismisses the sheet and
session-generation checks suppress late result callbacks.

The final canonical signed build was installed and quit/reopened with its original
signing requirement and account association. Native inspection checked that the
button opens a sheet, typing enables the separate Create action, Cancel closes
the sheet, reload still shows no additional addresses, and a fresh dialog opens
with an empty draft. Its layout was inspected. The app was left with the new
dialog open. No owner address was reserved. Evidence:
`/tmp/cs-mail-new-address-dialog-final.log`. Diff checks passed. No tests were
added and no Rust/PostgreSQL suites were run for this Swift presentation change.
Creation commit/recovery and iOS runtime were not re-exercised in this repair.

## My addresses management — 14 October 2026

The unified app has a My addresses workspace under Relationships, with an address
list in column B and details in C; New address opens a creation sheet. Private/public purpose, lifecycle,
transition status, available actions and preserved conversation facts come from
Rust projections with an observation time. Swift captures user intent and revision;
Rust validates and journals Create, Restrict, Replace and Retire. A typed refusal
is distinct from an unknown outcome. Resume update reuses the original signed
operation; stale snapshots disable new changes. Lock clears address presentation,
and session generations reject late results. No account or Keychain credential
is created or replaced by these operations.

The existing primary address and My approach page remain intact. Additional
addresses explicitly state that sending/receiving is unavailable until delivery
integration. They do not receive public profiles, paid-request links or automatic
permission transfers. Composition, receiving-route evidence, held-mail handling
and alias-aware acceptance remain subsequent work.

The offline `tools/development-continuity/alias_metadata_upgrade.py` restored a
fresh schema-38 owner copy and verified all 110 original table/sequence
fingerprints, original migration history and configured key files. The original
schema-37 database remains untouched as a rollback checkpoint. The new pinned
owner launcher is `target/alias-native-20261014/start.sh`, using
`target/alias-native-20261014/owner-ready/maild.json`; its server hash and
preservation result are retained beside it. It serves the existing 9693 endpoint.
No reenrollment, owner-address substitution or direct canonical-record edits were
performed.

Scoped validation passed: six portable-client tests, including extended exact
alias recovery and typed refusal; strict lint for the client/bridge with all
targets/features; all 101 identity source pins; formatting/diff checks; Swift
compilation; and the existing native enrollment journey extended through the
real isolated backend for create/restrict/replace/retire, invalid transition
refusal, retired-address reservation, Directory isolation and retained account/
credential. No test cases were added. The native run used the explicit alias
scope; unrelated conversation/payment branches were not run. Evidence:
`/tmp/cs-mail-address-client-tests.log`, `/tmp/cs-mail-address-lint-final.log`,
`/tmp/cs-mail-address-native-checks.log`, `/tmp/cs-mail-address-preservation.log`
and `/tmp/cs-mail-address-swift-final.log`. Actual alias delivery, iOS runtime,
held-mail release and public-provider delivery were not exercised.

Installation completed at the canonical
`target/apple/CsMail-continuity-20261012.app` path after Marcus authorized the
original signing key directly in macOS. The build script verified the original
signing requirement before replacement. Quit/reopen and refresh were inspected in
the installed app: My addresses loaded its real empty list and observation time;
private/public purpose copy and the creation confirmation were checked; the
inspection draft was cancelled and cleared without reserving an address. The
original `marcus@csqd-mail.com` publication, simulated coverage and saved contacts
remain present without reenrollment or device-credential replacement. The app is
left on My addresses. Evidence: `/tmp/cs-mail-addresses-apple-final.log`.

Address-detail lifecycle controls were exercised through the real isolated native
bridge experiment, but were not visually exercised with populated addresses in
Marcus's account, which has no additional address records. No owner test addresses
were created. This validation does not establish alias mail delivery.

## Alias backend foundation — 14 October 2026

The portable Rust client and backend now have checked alias lifecycle, explicit
receiving scopes, signed owner management commands and encrypted exact-operation
recovery. The native bridge understands a pending alias update so the existing
resume path can resolve its original operation. Native alias controls and the
native/SMTP delivery integration are pending; this build does not make aliases
usable for mail. See the [implementation boundary](../../docs/application-persistence-boundaries.md#alias-foundation--14-october-2026).

The canonical signed Apple build was installed at
`target/apple/CsMail-continuity-20261012.app`, after comparing its original signing
requirement. Marcus authorized macOS's existing signing-key prompt directly.
Quit/reopen and refresh were inspected against the original pinned owner backend;
the approach page retains `marcus@csqd-mail.com`, simulated coverage and the original
contacts. No reenrollment or replacement device credential was introduced. The
backend had been stopped and was restored with its existing
`target/column-b-navigation-20261013-8a4b/start.sh` launcher. Its verified database,
configuration and pinned executable were retained; migration 0038 was not applied
to that owner deployment. Build evidence: `/tmp/cs-mail-alias-apple-build.log`.

Backend validation uses an isolated PostgreSQL database and extends existing
conjecture-driven experiments without adding test cases. The final gate run passed
identity pins, dependency direction, formatting, production lint, 114 workspace
checks/tests, 106 PostgreSQL tests and application binaries. Its aggregate result
retains an earlier all-target lint failure from a newly extended test fixture's
method/enum syntax. That fixture was corrected; the subsequent strict workspace,
all-targets, all-features lint rerun passed. Formatting and diff checks were then
rechecked. Evidence: `/tmp/cs-mail-alias-quality-gate-final.log` and
`/tmp/cs-mail-alias-clippy-final.log`. Actual alias delivery, concurrent alias
delivery/blocking, held-mail release, iOS and public providers are outside these
experiments.

## Neutral contact focus in column B — 13 October 2026

Sidebar rows suppress the system's blue focus effect and draw a neutral outline
for focus, retaining the existing neutral selection wash and large click regions.
This applies to the shared contact/request/lane/financial rows and sidebar destinations.
The canonical signed build was installed and reopened. Pointer selection in the
active Communications screen and Tab focus were inspected; focus remains visible
without blue highlighting. Build evidence: `/tmp/cs-mail-contact-focus-build.log`.
No tests were added or unrelated Rust/database suites run. iOS was not inspected.

## Area-specific selection in column B — 13 October 2026

Column A selects an area; B selects an item; C opens that item's contents. Home and
Notes have no sidebar. Communications has one searchable correspondence list,
including external mailboxes, and email composition is a toolbar action. External
reading no longer introduces another mailbox list; multiple conversations for the
same selected mailbox use a conversation picker.

Relationships retains Directory and My approach page, with Incoming/Outgoing request
filters. Each request has its own identity, status and recorded terms-issuance date;
direction is retained when opening its conversation. Express lanes lists current
incoming and outgoing grants, with expiry in the row and scope/rate in the detail.
Economics lists financial records with concise payment/refund summaries and issuance
dates, and opens one selected record. Q&A has one destination. The duplicate global
relationship/lane cards, self-links, permission subtitles in unrelated lists and
sidebar implementation explanations are removed.

The portable client obtains owned incoming and outgoing records through authenticated
member reads and projects lanes for an explicit time. The new Outbox read is restricted
to the signed address; recipient names have their own outgoing-view field. Navigation
does not change the encrypted cache format, create relationships, grant permission,
authorize charges or record sending consent. Its read failure is shown as a refresh
error rather than an empty-list claim.

The current owner launcher is `target/column-b-navigation-20261013-8a4b/start.sh`.
It uses the same `target/principles-alignment-20261013/owner-ready/maild.json`, verified
owner database, deployment keys and device credential. The prior pinned backend is
retained; there is no schema migration or reenrollment for this update.

Relevant validation: existing client tests, lint checks for the affected Rust crates,
the PostgreSQL acceptance scenario extended with owned-Outbox and other-persona signing-key refusal
checks, and the existing native journey extended with lane identity/direction/expiry
assertions. No test cases were added. The initial native run stopped in the optional
failed-payment experiment; its log is retained. The scoped final native journey passed
without that unrelated optional branch, including external consent/reply/permission
workflows. Evidence: `/tmp/cs-mail-column-b-client-tests.log`,
`/tmp/cs-mail-column-b-clippy.log`, `/tmp/cs-mail-column-b-service-tests.log`,
`/tmp/cs-mail-column-b-native-checks-final.log` and `/tmp/cs-mail-column-b-build.log`.
The installed owner app was inspected for list/detail selection, request filters,
historical record dates, both lane directions, full-width Home/Notes and the empty
email composer. Its owner has no external threads, so the integrated external-mailbox
list and multi-conversation picker remain unexercised interactively there. iOS,
public-provider delivery and unrelated Rust/database suites were not checked.

## Consistent reading and copying — 13 October 2026

`EditorialReadingView` gives each reading region one native, read-only text surface.
Conversations, external-message transcripts, Q&A, profile descriptions, request terms,
financial records, referenced text and account status support selection across their
paragraphs and blocks. Controls and editable fields remain separate. Navigation rows
and contact/profile cards expose a context-menu Copy command; their labels do not
compete with clicking through text-selection gestures. The source-selection editor
retains its existing policy-aware selection mapping.

The macOS view measures with a separate layout manager, so SwiftUI width probes cannot
resize the displayed text. Unchanged document refreshes preserve selection; appearance
changes preserve it when the text is unchanged. Text remains wrapped and scrolls in
its existing reading container.

Relevant validation: the canonical signed Apple build, actual dragging across Q&A
paragraphs and two native messages, copying/pasting into an unsent composer, scrolling,
Select All confined to the document, and a contact-card Copy command that leaves
navigation unchanged. Verification drafts were cleared without
sending. Original account and Keychain continuity are preserved. No tests were added
and unrelated Rust/PostgreSQL suites were not run. Build evidence is
`/tmp/cs-mail-reading-install.log`. iOS runtime, every secondary form and external
transcripts with messages have not been interactively exercised in the owner app.

## Audit repairs and preserved owner deployment — 13 October 2026

External mutations now return provider-signed acknowledgements bound to the exact
command digest, caller key/signature, recorded time and outcome. Authenticated
semantic refusals commit a receipt without conversation, ciphertext or delivery
changes. Replaying one after consent restoration or unblocking retains that original
refusal; a new attempt requires a new operation. HTTP errors, unavailable facts,
unresolved policy and altered acknowledgements retain the encrypted journal.

External paging follows cursor progress through expired-only pages. Native and
external contacts share one permission projection with explicit time and matching
lane evidence. Swift renders standing, bounded and closed perimeters from that typed
projection. Restored protocol records reject multiple unresolved requests and foreign
owners. A malformed stored record is a storage fault, preserving received work for
recovery instead of committing a permanent refusal. Legacy DMARC evidence acquisition
is service-owned; temporary failures remain retryable. Older atomic storage
orchestration is retained where this repair established no behavioral defect.

Schema 37 separates external operation receipts from conversation existence, so a
refused first invitation creates no conversation. It refuses populated predecessor
environments. The offline `tools/development-continuity/external_receipts_upgrade.py`
creates and verifies a fresh copy, including retained maintenance history and
sequence positions, before it can be used. Runtime cache format 3 and original
credentials are unchanged; there is no compatibility reader or reenrollment.

The current owner backend uses the verified `cs_mail_principles_owner_ready_20261013`
copy, with the same original account, personas, address, signing keys, service URL
and Keychain item. Its launcher is `target/principles-alignment-20261013/start.sh`;
its configuration and preservation record are under that directory's `owner-ready/`.
The original `cs_mail_continuity_20261012` database and earlier configuration remain
unchanged rollback evidence. Incomplete earlier copy attempts are retained and were
never started. The canonical app remains
`target/apple/CsMail-continuity-20261012.app` (**cs-mail Development**).

Validation extends existing experiments without adding test cases: exact replay,
transport/refusal distinction, acknowledgement substitution, expired-page traversal,
restored-state corruption/recovery, and external permission routes. The complete gate
and native member journey use isolated local PostgreSQL, fictional browser identities,
simulated payments and the local relay. An initial native run stopped on an unavailable
response; the failure record is retained alongside the successful fresh rerun.
Final gate evidence: `/tmp/cs-mail-principles-finished-quality-gate.log` (all eight
checks passed). Final native journey: `/tmp/cs-mail-principles-native-final-checks.log`.
The maintenance copy and exact rerun matched 101 tables and nine sequences on an
inactive check copy as well as the deployed preservation copy.
The installed app reopened the original address and displayed the external reader
and popup composer with unchecked disclosure consent and a disabled empty send.
Keychain item identity and credential fingerprints matched before and after.
Public/provider delivery, iOS, actual power-loss recovery and interactive external
lane/blocked perimeter inspection remain unestablished by this pass.

## External email in the unified app — 13 October 2026

Communications → External email opens the conversation reader and popup composer.
Choose an external address and explicitly authorize plaintext disclosure to the
relay and mailbox for that conversation. Only authored text is supported; native
quotations, sharing and references are excluded. The development backend uses a
local file relay, so no public email leaves this environment.

Rust owns signed commands, ciphertext preparation, encrypted local journaling,
exact-operation retry and canonical permission projections. Swift renders waiting,
unknown handoff, relay acceptance and provider reports separately. Provider
acceptance does not establish delivery. A lost response preserves the original
command; replacing an unresolved send is refused. Known acknowledgement remains
successful even if the subsequent view refresh fails.

Authenticated replies appear in an unpaid pending invitation. Confirmed Accept,
Reject, a seven-day express lane, Block and revocation use the existing relationship
and external-conversation owners. Disclosure withdrawal cancels waiting work but
cannot guarantee recall after dispatch begins; restoration does not revive cancelled
messages. Paid guest requests remain their existing workflow.

The agreed experiments challenge consent refusal, authenticated pending reply
admission without a paid request, withdrawal/restoration, rejection, explicit new
invitation, acceptance, lane, blocking, encrypted persistence, same-device reopening,
and exact ciphertext replay after response loss. The native harness uses isolated
accounts and the trusted synthetic receiver fixture, not a public authentication
endpoint. The owner deployment was explicitly authorized for a local-relay cutover.
Read-only preservation checks confirmed unchanged original Keychain entry/credential,
account/persona/device-key associations and existing deployment keys. No schema change
or account reenrollment was needed.

Existing PostgreSQL suites challenge email authentication, consent/dispatch
serialization and durable deduplication. Public SMTP/provider callbacks, full visual
and accessibility checks and iOS runtime behavior remain outside this pass.

# cs-mail Apple client

SwiftUI presentation over `cs-mail-client::member`, exported through
`cs-mail-client-ffi` with UniFFI 0.31.0. Domain and identity decisions retain their
existing Rust owners. Minimum development targets: macOS 14 and iOS 17.

Implemented: browser sign-in and first-device enrollment; encrypted device-state
persistence; reopening the same device; inbox/request reading, standing acceptance, rejection and express-lane acceptance;
durable decision and encrypted-send retry; native conversation reading and composing; explicit offline, denied-custody, review and expiry errors.
The interface requires fresh synchronization before enabling decisions.

## One unified application — 13 October 2026

There is one member app, with no demo mode or separate preview/assessment app.
Open `target/apple/CsMail-continuity-20261012.app` (display name **cs-mail Development**).
It runs all implemented native workflows against the preserved local development
account `marcus@csqd-mail.com`. The navigation rail, reactive lists, portal perimeter
feedback, popup composer, request initiation/review, follow-ups, received-conversation
replies, source reuse, contact controls, Directory, approach-page editor and recorded
request finances are available in that app. External-email composition, consent and
reply review now use the portable Rust client. Notes, annual finance interfaces and
replacement-device recovery remain unimplemented.

The transient fixture adapter, demo branches, startup flag and demo-packaging script
are removed. `MemberModel` and `DirectoryModel` share one `ClientWorker`; every action
uses the portable Rust client and canonical service. There are no fake decisions,
transient demo publications, mode switches or substitute readers. Simulated bank
and payment evidence and the persisted fictional participant accounts remain backend
adapters in this development environment, independent of the application interface.

Build and launch only this app:

```sh
DEVELOPER_DIR=/Library/Developer/CommandLineTools \
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk \
  scripts/apple-build.sh
open target/apple/CsMail-continuity-20261012.app
```

The build owns the original credential service, encrypted v3 store, service URL and
bundle identity. It requires stable signing and refuses to replace an existing app
with a different designated signing requirement. The redundant account-build wrapper
and named review-build variants are removed. Existing app bundles were archived and
verified byte-for-byte before separate demo/review/legacy bundles were retired.
`target/apple` now contains exactly one app bundle. Rollback archives are in
`target/apple-rollback-20261013`; restore a checkpoint only with its matching database,
cache and service binary. Original account/identity databases and older caches are
retained unchanged; the original schema-32 app is preserved in its verified archive.

Validation: the full eight-check quality gate including PostgreSQL passed
(`/tmp/cs-mail-unified-quality-gate.log`). The existing native journey passed against
separate test databases (`/tmp/cs-mail-unified-native-checks.log`), covering quotes,
reopening, funding/submission, follow-ups, multiple replies, acceptance, source reuse,
policy controls, revoke/block/unblock, retry/refusal recovery, SQLite and isolated
Keychain checks. No automated test cases were added. The final signed build passed
(`/tmp/cs-mail-unified-install-build.log`). Read-only comparisons preserved the
original Keychain persistent item identity and credential bytes, account/persona
association, public-key records, original caches and signing requirement.

The updated app reopened the original account without enrollment. Native inspection
confirmed the original address and saved Daniel correspondence; New conversation
opened the real composer and its retained-source picker. The empty verification
composer was closed without sending. iOS execution, live providers and a comprehensive
visual/accessibility audit remain outside this pass. Earlier references below to
review apps and offline fixtures are historical validation records, superseded by
this unified application cutover.

## Navigation framework — 13 October 2026

The member application now has an icon rail (Column A), a reactive navigation
list (Column B), and the main communication space. The destinations are Home,
Communications, Relationships, Express lanes, Economics and Notes. Home retains
the membrane overview; Communications searches existing people and opens their
correspondence. Relationships exposes incoming/outgoing request records, Directory,
the member's approach page and the existing saved-request review sheet. Selecting
a contact from a request or lane list keeps that area's rail selection.

The lane list describes incoming permission only; outgoing authority remains
separate in the contact controls. Economics reuses recorded request finances and
Q&A. Annual-service/distribution interfaces and forecasts, drafts/sent overview,
dedicated lane history/editing and a Notes editor are not implemented by this pass.
Their absence is explicit; no synthetic financial estimate or note-save action
is offered. On narrow layouts the rail becomes a row, with the same destinations.

Main-space portals highlight their existing boundary on hover, leaving their
interiors unchanged and preserving standing gaps and bounded-lane markers. The
entire portal remains clickable. Contact-list, conversation, Directory and
request-class rows retain whole-region shading and matching hit regions.
The rail previews the same navigation component
used in Column B, after a 250 ms initial pointer dwell; an already-open preview
switches immediately when another icon is hovered. A 220 ms dismissal grace period
allows movement into the scrollable preview. Pointer-opened previews stay open
only while the pointer is over an icon or the visible popup. Explicitly
keyboard-opened previews also retain the matching rail or popup focus.
Tab reaches the rail; Right Arrow opens a preview,
and Escape or Close dismisses it. Previewing does not change the selected area or
main space. Activating a preview item performs ordinary navigation. Keyboard,
accessibility actions and ordinary click navigation provide alternatives to hover.

One typed Swift destination owns the selected area and detail route. Swift owns
search, focus, preview and navigation state. Rust continues to provide permission,
request, lane, financial and retry meaning; no domain rule, schema, client journal
or credential operation changed. The correspondence view remains mounted while
another destination is shown, preserving its reading and popup-composer context.
Locking still removes the session and its transient readable content. Hover uses
existing summaries and initiates no decryption, funding, grant or send.

The agreed validation scope uses existing suites and native journeys, with no
new automated test cases. All eight quality-gate checks passed, including isolated
PostgreSQL suites (`/tmp/cs-mail-navigation-quality-gate.log`). The existing native
journey passed against a separate fictional deployment
(`/tmp/cs-mail-navigation-native-checks.log`), including exact request retries,
same-device reopening, encrypted persistence and isolated Keychain checks. Signed
macOS packaging passed (`/tmp/cs-mail-navigation-owner-build.log`). The earlier
build/gate attempt failed because the compiler had no Apple SDK path; the successful
runs supply `SDKROOT` explicitly and use Command Line Tools.

The isolated demo established three-slice layout, conversation search, Directory
routing, selected-contact/search retention, accessible previews, preview-item
navigation and Escape dismissal. The final demo starts without an unsolicited
preview. Tab followed by Right Arrow opened Communications' preview while Home
remained selected; search within that preview retained keyboard focus, and Escape
dismissed it without navigating. Pointer dwell/cross-gap timing was not established
through the available native automation. iOS execution, interactive expiry,
an exhaustive focus/clipboard/accessibility audit and composer retention under
owner-account navigation are not established by the native harness.

The temporary review bundles used for this pass have been retired. The unified
application section above records the current app and preservation checks.

### Hover correction — 13 October 2026

Marcus reported that hovering Home could show Communications and that leaving
the icon/popup did not dismiss it. Keyboard focus could overwrite the pointer's
destination, while popup tracking included transparent padding across the rail.
The correction gives pointer and keyboard previews distinct typed sessions and
one owner. One pointer observation now selects from measured icon and visible-popup
bounds in a shared coordinate space; transparent positioning padding is excluded.
Incidental rail or text-field focus cannot pin or rename a pointer-opened preview.
Previewing still performs no Rust operation or authority change.

The two challenged claims were that Home's visible hover region selects Home
regardless of earlier focus, and that leaving both icon and popup dismisses a
pointer-opened preview. The corrected offline demo was rebuilt and reopened;
Marcus confirmed both behaviors work. Exact dwell/cross-gap timing and all-icon
stress traversal were not separately measured. No automated test cases were added.
All eight quality-gate checks passed, including isolated PostgreSQL suites
(`/tmp/cs-mail-hover-fix-quality-gate.log`). The signed macOS build passed
(`/tmp/cs-mail-hover-fix-build.log`); candidate signing requirement and account
configuration still match the preserved owner app. The running owner app was not
closed or replaced, and iOS remains untested.

### Portal perimeter feedback — 13 October 2026

Main-space portal hover strengthens only the supplied membrane stroke. Its interior
has no hover fill; standing openings and bounded-lane markers retain their existing
shape. Home and the relationship/lane overview cards share this portal style.
The full card remains clickable, while its decorative strokes ignore hit testing.
Column A/B and ordinary list rows retain their region shading. No Rust, permission,
operation or persistence behavior changed, and no automated test cases were added.

The signed macOS build and all eight quality-gate checks passed, including isolated
PostgreSQL suites (`/tmp/cs-mail-portal-perimeter-build.log` and
`/tmp/cs-mail-portal-perimeter-quality-gate.log`). The refreshed offline demo opened;
portal activation still opened the selected correspondence, and returning to Home
restored the overview. Candidate signing/account configuration still matches the
original owner app. Full light/dark hover and iOS audits were not performed. The
owner app was not closed or replaced.

## Principles repair and received-conversation replies — 13 October 2026

Every received native conversation now has a confined reply route, independent
of request origin and broader reverse-lane expiry. Starting another conversation
still requires separate permission. Conversation receipt evidence and a distinct
relationship reply-withdrawal epoch replace per-request answer permits. Blocking
or revocation advances that epoch; unblocking alone does not revive old offers.
A request's broader reverse-lane consent is separate immutable submission evidence,
so later receipt of a message cannot silently restore withdrawn broader consent.

Both request and ordinary publication declare receipt evidence with mailbox
publication in one atomic consequence. Request delivery distinguishes creating a
conversation from updating an existing one. Unchanged opposite directions have
no invented transition or journal entry. The native source picker explicitly
loads retained message detail for the selected source contact. Same-contact
navigation, request formation and cancellation reload current correspondence.

Protocol outcomes have one typed application owner; the client verifies their
commitment against the signed receipt. Billing refusals are durable outcomes and
native billing responses bind the exact command and outcome under the provider's
signature. Definite refusals resolve setup journals; transport uncertainty retains
exact bytes. The superseded standalone native class-publication workflow is
removed in favor of atomic address publication. Request finances distinguish
pending, finalized, voided and reversed funding from the authorized amount.

Schema 36 and device-cache format 3 are explicit cutovers. Ordinary migrations
require a fresh environment. The preservation-checked maintenance tool upgrades
Marcus's existing database and a separate encrypted cache copy, retaining the
original Keychain entry, account/persona association and address. It records reply
receipt corrections, preserves withdrawals and financial history, and never
changes historical agreement to a broader reverse lane. Prepared ciphertext and
its durable intent survive uncertain commit acknowledgements and can be reconciled
without re-enrollment or a second charge. Runtime has only the format-3 reader.

Agreed checks challenge receipt substitution, refusal replay after policy change,
source-picker discovery through the actual presentation model, same-person reload,
failed funding, original-conversation replies and interrupted conversion recovery.
All eight local quality-gate checks passed, including PostgreSQL
(`/tmp/cs-mail-repair-clean-quality-gate.log`). The native journey passed
(`/tmp/cs-mail-repair6-native-checks.log`), including exact Unicode selections,
source discovery, same-contact reload, setup refusal and voided funding.
Standalone maintenance-tool tests, clippy and build passed. An injected lost
commit acknowledgement retained and reconciled the encrypted prepared cache.

The original account upgrade and signed app installation are complete. A fresh
backup comparison preserved account/persona records, endpoint keys, requests,
billing, request finances and ledger records. The original v2 cache and Keychain
credential remain unchanged; the new separate store uses v3. The signing
requirement matches the previous app. All four original fictional peer endpoints
decrypted exact verification replies from the original owner identity. Native
inspection confirmed the original address, updated synchronization, and Reply in
Elena's retained conversation and Maya's expired original conversation.
The contact overview distinguishes outgoing initiation permission from permission
the contact has to approach the member. Standing permission and active outgoing
lanes offer New conversation; otherwise Request a relationship opens the selected
Directory profile. Received-conversation replies remain separate. Requests retain
published-approach selection and signed quote review.
Native clipboard/focus interaction, full visual layout, iOS, real providers
and replacement-device recovery remain separate evidence.

## Pending incoming request entry — 13 October 2026

Contact and Directory actions use one Rust `RequestEntry` projection over the
authenticated pair. A received request still within its decision window projects
IncomingPending; the contact overview offers Review request instead of Request a
relationship, preserving Reply in the existing conversation. A pending outgoing
request, existing initiation permission, unavailable direction and own address
have distinct alternatives. Public Directory search results are Unverified;
profile lookup independently obtains the private authenticated request state.
A published class is an invitation, not eligibility or charge authorization.

The projection reuses `RequestLifecycle::awaits_decision_at`, the same rule used by
paired preparation/submission. It passes time explicitly; Swift does no deadline
comparison. Passed decision windows no longer appear as awaiting a decision even
when expiry recording is delayed. A later quote/formation still rechecks the
protected pair, history, class and policy before creating payment work.

Agreed checks extend the existing native journey to challenge incoming/outgoing
classification, Directory consistency, terminal-state release and retained replies.
Native inspection confirmed Maya's pending notice and Review request, the existing
request's decision controls and Reply, and no Directory request action after class
selection. The first native harness run exposed a self-profile lookup constructing
a self relationship; OwnAddress now preserves public profile access without that
pair read. The fresh native journey passed (`/tmp/cs-mail-request-entry3-native.log`), including
incoming/outgoing and Directory classification, terminal-state release and retained
replies. Its Directory check publishes its fictional sender's public profile before
lookup; an unpublished profile is correctly unavailable. All eight quality-gate checks passed (`/tmp/cs-mail-request-entry-quality-gate.log`),
as did the signed build and signature verification. Final native inspection again
confirmed Maya's pending notice, Review request and retained conversation access.
Expiry uses the existing domain decision-window
experiment; no interactive wait across expiry or iOS execution was performed.

The preserved local backend now runs independently from an immutable binary copy,
using the same database, configuration and keys. Its restart script also pins each
built binary so rebuilding the repository cannot replace its executable in place.
The earlier isolated fixture and its data are retained; no owner account was reset.

## Preserved acceptance alignment — 13 October 2026

The original accepted Daniel and Elena development samples were converted without
reverse-lane agreement. The explicit maintenance correction obtains fresh signed
consent from each original fictional recipient key, while retaining the historical
request terms and acceptance evidence. Its current policy, immutable source
request and acceptance event define the correction window. Expiry remains original
acceptance time plus the policy lifetime; the correction does not restart it.

The application owns the deterministic correction decision. The storage adapter
protects both directions and current authority, then uses the ordinary lane
transition and receipt persistence. Effects, receipt and correction provenance
commit in one transaction. Exact prepared commands survive unknown acknowledgements;
a changed retry is refused. Later withdrawals are compared by canonical event
order. Current stronger permission, later explicit grants, expired windows and
unresolved reverse requests are preserved. The maintenance API is compiled only
with `development-maintenance`; it is not a runtime fallback or a new app endpoint.

The original account correction was applied after rehearsal. Account/persona,
operational/content keys, requests, billing, request finances and ledger records
compared unchanged. The original Keychain helper verified credential continuity.
Both corrections applied once; Maya and Jules were preserved because their
original requests were not accepted. Rehearsal retained the exact correction
through simulated lost acknowledgement, replayed without another grant, and
refused resurrection after signed block/unblock and an altered retry. Evidence is
in `target/acceptance-alignment-20261013`. Actual power loss and iOS are not covered.

The sidebar quietly labels its groups Who can contact you. Manage contact
separates the member's outgoing route from permission they grant to the contact.
Inactive incoming lanes no longer appear as current express-lane permission.
The signed macOS build and signature verification passed. All eight final gate
checks passed (`/tmp/cs-mail-acceptance-alignment-final2-quality-gate.log`). Native
inspection confirmed the original address, Who can contact you, Daniel's New
conversation action, a fresh empty composer, and the two directions in Manage
contact. The validation composer was closed without sending. Exact original-account
replay reported both corrections as replayed with no second grant. Original
signing requirement, Keychain credential and predecessor encrypted cache remain
preserved. iOS and a comprehensive visual/accessibility audit remain untested.

## Contact actions and recipient expiry reminder — 13 October 2026

Rust projects the outgoing route for New conversation; Swift does not interpret
incoming standing permission as permission to send. The contact header explicitly
labels permission for contact from the other person. Request a relationship opens
that person's published approaches before quote review. Profile lookup finishes
before Directory navigation, and initial result loading cannot replace a profile
or erase its lookup failure.

A typed Rust projection supplies a recipient-side expiry reminder only for a
current lane within the confirmed three-day window. The effective expiry uses an
expiring lane's horizon and a persistent lane's final validity. It is evaluated
from explicit lane evidence and time on synchronization/contact reads, with no
permission mutation. The sidebar offers a review entry; the contact view offers
Upgrade to standing permission and Let it expire. Upgrade uses the existing
recipient-signed standing transition after confirmation. Let it expire merely
acknowledges the reminder for that open view; the lane's schedule remains unchanged.
No operating-system notification or background reminder service is established.

The existing native journey was extended (no new test cases) to challenge a
recipient-only two-day reminder, no seven-day reminder, the standing upgrade,
and unchanged reverse permission. It passed on an isolated environment
(`/tmp/cs-mail-contact-actions-native.log`). All eight quality-gate checks passed
(`/tmp/cs-mail-contact-actions-quality-gate.log`). Native inspection confirmed direct profile routing, approach selection, signed
quote review remaining presented through journal updates, discarding the uncharged
quote, and Reply in the existing conversation. Quote presentation is now owned by
the main member view rather than a Directory row whose lifetime can end on update.
The saved-workflow review retains Done even after the quote is discarded. No
validation charge was authorized. iOS and interactive near-expiry reminder layout
are not established. The preserved owner fixtures do not establish outgoing
standing/lane permission: native checks used isolated two-party actors for those
routes; the header distinguishes incoming permission from the outgoing action.

## Appearance control restoration — 13 October 2026

The shared member view exposes a toolbar button to switch between light and dark
appearance. Before the first explicit choice it follows the system appearance;
once selected, `csMailAppearance` in the app's preferences retains the choice
across account locking, demo changes and reopening. Sheets inherit the window's
appearance. This is local presentation preference, with no Rust domain rule or
credential change. No automated test cases were added.

Validation scope: switch both ways in the signed macOS app, reopen to challenge
persistence, and run the existing full quality gate. iOS execution is not covered.
The signed macOS build and all eight quality-gate checks passed
(`/tmp/cs-mail-appearance-quality-gate.log`). The installed app was observed
switching to Light and back to Dark. After a confirmed process exit and reopening,
Dark remained selected and the toolbar offered Light. The original designated
signing requirement and account association are retained. iOS was not run.

## Native member workflows — 12 October 2026

The current signed continuity app now exposes native paid requests from the
Directory, explicit signed-quote review before charge authorization, encrypted
formation/submission retry, cancellation of unsubmitted preparation, and follow-ups
under the original request. No-charge and existing-request quote outcomes are
separate from refusal. Expired quotes are reported explicitly. Funding waits return
to the interface rather than polling. The staging endpoint returns the Rust
application's durable document receipt, replacing the former unbound `stored` flag.

Contact management exposes standing permission, revocation, blocking/unblocking,
a personal/human-initiated express-lane preset (20 messages per day, with reviewed
lifetime), and exact follow-up limits. Lane regrants retain the lane identity and
advance its version. Request status and current request money are available to the
sender; recorded money appears separately in Q&A, outside recipient decisions.

Authenticated native mutation refusals now commit a terminal operation receipt.
Exact replay remains refused even after permission/policy changes. The portable
client checks its operation/digest, resolves only acknowledged work, and retains
uncertain transport outcomes. Refused sends no longer prevent synchronization.
Conversation-policy changes have their own durable journal. Both participants must
approve relaxation at the reviewed revision; restriction cancels a proposal.

Each conversation projects its own confined answer permission and received/sent
request linkage. Selecting a thread scopes its request controls, and decision
callbacks name that exact displayed request. Older rejected/expired conversations
remain answerable when their original permits remain active. Historical absence of broader-lane terms does not remove confined replies;
conversation receipt evidence now supplies those replies.

Source inclusion is explicit: quotations, retained shared copies and references
are assembled by Rust from exact selections, retain provenance, and are checked
against current ancestry policy at commit. References can be opened without
treating their pointers as access grants. The composer-led passage interface below
replaces the former whole-message interface.

## Conversation navigation and exact source selection — 13 October 2026

Opening a person now shows their conversations directly, with content-derived
labels, previews and dates. Opening a row scopes reading and request decisions to
that conversation; there is no “Conversation” picker or combined-history composer.
Reply appears only for a currently projected answer/standing/lane route. Otherwise
the reader explains that replying is unavailable. Starting a new conversation is
separate from starting a request, which opens the recipient's Directory profile
and signed-quote review. Contact controls remain under Manage contact.

Reply and New conversation open an outgoing composer with a named destination.
Include an existing message opens an authenticated retained source in a resizable
panel beside that composer on macOS. Its read-only native text view supports normal
highlighting and copying. Swift maps UTF-16 highlights to ordered UTF-8 ranges in
the source's exact flattened content blocks. A selection can cross text blocks.
Images can be selected as whole blocks; embedded reference markers cannot be
selected, and crossing them is refused rather than silently dropping content.
Quote selection, Reference selection and Share selected copy add removable previews
with source attribution to that draft only. There is no source-selection state
carried between contacts and no reuse menu in the reading view. Closing an unsent
draft asks before discarding its in-memory content; locking removes it.

Rust exports the authenticated source's immutable version and selectable blocks
in memory. The bridge now requires that version and explicit elements; the old
whole-message constructor and unsupported-whole-message error are removed.
Rust validates target version, block bounds and UTF-8 boundaries for references
as well as retained copies before journaling a send. Domain policy and live
permission are still rechecked at the atomic commit boundary. Plain copying and
pasting authored text does not manufacture quotation provenance.

Agreed validation adapts the existing native journey to challenge exact Unicode
excerpt preservation, cross-block mapping, exclusion of surrounding text and
refusal of out-of-bounds, substituted-version and split-UTF-8 selections. No new
test cases were introduced. The corrected native journey, signed macOS build and
all eight final quality-gate checks (including PostgreSQL) passed. Preservation
checks retained the original credential/account/address, cache and signing
requirement; the previous app and validation logs are saved in
`target/native-selection-owner-upgrade-20261013`. The first run stopped on a test
helper attempting to match a component of a compound emoji; that helper was
corrected and the full journey passed on a fresh isolated environment.
Interactive visual behavior, image-selection runtime,
iOS, native-view focus/clipboard interaction and response-loss/crash injection are
outside this evidence. The new source reader does render retained images;
ordinary conversation reading still uses image placeholders. Image authoring
remains separate work.

Validation reused the existing native enrollment check sequence, extended for the
consequential new member journeys; no new test cases were added. Two fresh fictional
members exercised quote review without a charge, same-device reopening, finalized
funding and native submission, follow-up, answers after rejection and after a later
request, acceptance, quotation/share/reference resolution, no-charge quoting,
two-party policy relaxation, revoke/block/unblock, lane regrant, refusal recovery,
and plaintext exclusion from both encrypted SQLite caches. The complete quality
gate, including PostgreSQL, and signed macOS build passed. The first isolated run
exposed the discarded staging receipt; its database/state were preserved and the
repaired journey passed on fresh isolated environments.

The original credential was read only through the preservation helper's anonymous
pipe. Checks retain its bytes, account/persona association, registered public keys,
`marcus@csqd-mail.com`, readable sample conversations and original signing
requirement. The current encrypted cache was unchanged during the upgrade checks.
A rollback copy of the prior continuity app/cache/database is in
`target/native-owner-upgrade-20261012`; the earlier schema-32 checkpoint remains
untouched. The same schema-34 backend/configuration and keys are reused.

Interactive inspection was blocked by an app-control timeout. Reopen
`target/apple/CsMail-continuity-20261012.app` to load the rebuilt executable;
a previously running process still contains the earlier build. iOS runtime,
replacement-device recovery, annual billing/distribution interfaces, external-email
composition/consent UI, crash/response-loss injection in the new native facade,
and live providers were not exercised in this pass. Existing Rust/PostgreSQL
replay, concurrency, expiry, consent and rollback suites remain separate evidence.

## Current owner app — 12 October 2026

The recipient request screens and decision confirmations show communication
permission rather than charge/refund breakdowns. A separate Q&A sidebar section
explains charges, refunds, expiry, cancellation and the shared member pool in
plain language. This is presentation only; Rust settlement and financial records
are unchanged. The signed macOS build and full Rust/PostgreSQL gate passed,
and original-credential account/conversation checks passed. Interactive visual
verification of this Q&A change was blocked by app-control timeouts; iOS layout
was not exercised.

Open `target/apple/CsMail-continuity-20261012.app`. The explicit development
conversion restored a separate candidate from the original account backup and
verified Marcus's original account/persona IDs, registered keys, address and
Keychain credential. The four original sample conversations are now current
native documents with matching message text and original decisions/permissions.
The original schema-32 databases, signed app and encrypted device cache remain
unchanged for rollback. This is the same account, not a fresh enrollment.

The current app uses `com.csqd.member.development` for Keychain and stable signing
identity `com.csqd.member.dev`, but keeps its converted cache separately in
`target/apple-continuity-state-20261012/device`. The backend listens on 9693 using
`cs_mail_continuity_20261012`; restart with the `start.sh` in that state directory.
Do not open the saved old `CsMail.app` against the current backend.

A new, clearly labelled Maya request exercises agreed native reply permission.
The app sent a pending answer and Maya's endpoint decrypted it; the request is
left awaiting Marcus's decision. The original Maya request expired under its
original deadline and remains readable. Original samples did not offer the new
reply consent, so their conversion does not grant new reply permission. Accepting
new requests activates the agreed reverse lane. Paid request initiation, revocation/blocking and source-reuse controls are now
implemented in the native member workflow slice described above.

The full Rust/PostgreSQL gate, signed macOS build, original-credential live reads,
plaintext comparisons and transaction rollback passed. See the standalone
[development conversion record](../../tools/development-continuity/README.md)
for exact preservation checks and limitations. iOS and replacement-device
recovery remain untested.

## Current baseline — development paused 11 October 2026

The 12 October native messaging cutover requires schema 34 and device-cache format
2 in a fresh, separate environment. Migration 0034 refuses populated earlier
schemas; the preserved schema-32 environment below remains an earlier checkpoint.
Keep its databases, deployment keys, state directory, signed app, Keychain
credential and Application Support store. Current binaries do not upgrade that
checkpoint or read its old native request content/cache. Use new empty databases,
state directory, ports, client-store directory and credential-service name for
cutover validation. Retaining the old Keychain bytes does not establish authority
in a new database; no credential or account is silently recreated.

Native requests now publish ordinary documents in one restricted conversation.
The contact card lists conversations with labels derived in memory, and provides
an encrypted send journal, composing and retry. A recipient can answer the
request conversation before deciding, after rejection or after expiry. Acceptance
activates frozen reverse-lane terms while preserving an existing custom lane.
Rust owns all admission and settlement; Swift selects the conversation and
presents the outcome. Paid request initiation uses the CLI; source reuse is available through Rust
correspondence APIs. Those UI workflows are not implemented here.
This section describes a historical isolated backend experiment. Its named app
bundles are retired. Use the single unified app described above for assessment;
isolated service and CLI/native-harness experiments do not create another member app.

Validation of this cutover: the complete Rust quality gate passed (112 workspace
checks and 104 PostgreSQL experiments), along with the HTTP/CLI demonstration
of pending answers, follow-ups, reverse-lane sends and settlement. The portable
client experiment also challenged an acknowledged send followed by a failed read,
so a refresh failure cannot be presented as a failed delivery. The macOS/FFI build
and native SQLite/isolated-Keychain checks passed. Browser enrollment was skipped
in that native harness; an interactive Swift messaging journey, iOS execution and
replacement-device recovery were not validated in this pass. These results use
separate test environments, not the saved owner account.

### Fresh messaging environment — 12 October 2026

The separate persistent backend is initialized at schema 34, with its own keys,
member and identity databases, and controlled browser provider. Its service is
`http://127.0.0.1:8580`; identity and browser-provider ports are 8581 and 8582.
State is in `target/apple-messaging-state-20261012`. Its signed app is
`target/apple/CsMail-messaging-20261012.app`, with bundle identifier
`com.csqd.member.dev.messaging-20261012` and client/Keychain namespace
`com.csqd.member.development.messaging-20261012`.

The fresh app completed browser sign-in using Marcus's fictional development
evidence and the controlled Marcus login. Its newly enrolled account has claimed
`marcus@csqd-mail.com` through the signed address-publication workflow. That address
is the owner's fixed development address across environments. No old account,
credential or contacts were imported; the original account remains separate and
preserved. The fresh profile currently has no paid request classes or simulated
service purchase. Those workflows are separate from claiming the address.

If the backend is stopped, restart it with:

```sh
target/apple-messaging-state-20261012/start.sh
```

The launcher refuses concurrent ownership and changed settings; reopening retains
this environment's keys and journals. The saved schema-32 databases, owner app and
credential namespace remain unchanged.

The normal signed app reopens Marcus's persistent development account at
`marcus@csqd-mail.com`. Communication space, Directory and My approach page share
one app and navigation model. The profile is currently listed with one introduction
class; up to eight classes may be published per address. Standing contacts and
express lanes are visually separate. Directory profiles have Back navigation,
readable text supports native Copy/Paste, and clicking the selected request class
clears its selection. The earlier offline fixture mode was retired in the unified
application cutover; current screens use account data.

The preserved personal environment contains six enrolled accounts: the owner and
five independently credentialed fictional participants. Maya was left awaiting a
decision; Daniel has standing permission; Elena has a seven-day express lane;
Jules was rejected; Noah's profile is visible but new requests are unavailable.
These are dated observations: request deadlines, service periods and lane expiry
advance with real time. On resuming, refresh canonical projections rather than
expect these categories indefinitely.

Identity evidence, banking and payments are simulated. The claimed mailbox neither
verifies a real external inbox nor enables public email delivery. Validation and
recovery observations are recorded under **Persistent owner-account cutover** below.
Earlier validation sections are historical records, not current account status.

The restart command below is historical: resuming that checkpoint requires its
matching schema-32 server and saved app, rather than current cutover binaries.
The saved app reopens its device without browser login. Preserve the
two databases, `target/apple-local-state` (including participant credentials),
`target/apple-signing`, the app's Keychain credential and Application Support store.
Do not reset storage or re-enroll to recover from custody or connectivity failure.
Quit the app before replacing its bundle with a rebuilt version. These same-device
observations do not establish backup or replacement-device recovery. This
documentation pause does not shut down the app or local services.

## Cancellation and Swift concurrency — 11 October 2026

The package and iOS target now use Swift 6 language mode. Queued client work checks
task cancellation and presentation lifetime before invoking Rust; late results
cannot reopen a locked or removed member view. Already-started Rust workflows
retain their existing durable operation and reconciliation semantics. Stopping
the UI wait is distinct from cancelling a domain request. Directory cancellation
preserves the prior screen, and delayed browser cleanup is scoped to its original
ceremony. SQLite's unchecked Sendable ownership was reviewed and documented; the
native harness vault's custody-denial flag now uses its existing lock.

Validation used existing checks only, with no new tests. The macOS Swift 6 build
passed (`/tmp/cs-mail-cancellation-build.log`), native encrypted SQLite replacement,
exclusive ownership and isolated Keychain checks passed
(`/tmp/cs-mail-cancellation-native.log`), and all eight quality-gate checks passed,
including PostgreSQL (`/tmp/cs-mail-cancellation-gate.log`). The initial sandboxed
gate failed on local socket permissions; the successful run had local test access.
Browser enrollment was explicitly skipped in the native harness. Native UI
cancellation/lock races, browser interaction and iOS builds/runtime were not
exercised. Full Xcode currently requires license acceptance; macOS validation used
the installed Command Line Tools. This pass did not replace the running signed
app or alter the preserved account environment.

## Visual development

The first visual direction draws on Cereal's editorial typography and warm
off-white palette, and John Pawson's proportions and use of space. Helvetica Neue
Light headings and regular Helvetica Neue controls/message text follow the
owner's preference for sans serif typography. Headings use a restrained scale,
labels use sentence case and ordinary spacing, and the light canvas is #F7F6EF.
The shared `CsMailPresentation` views provide the inbox, request reading area,
decision controls and empty state; the real member view supplies canonical facts
and retains the existing authenticated decision and confirmation workflows.
Dark appearance has its own palette. Inspection of the live reference pages found
NeueHaasUnicaPro-Regular for Cereal's sans-serif controls and
UniversNextPro-Light for Pawson's navigation and heading. Neither family is
installed here. The current typeface is an available approximation; those sites'
font files have not been copied or licensed for the app. The lighter weights,
smaller hierarchy and consistent family replace the initial generic sans-serif
treatment, without copying the sites' very small text sizes into reading surfaces.

The communication-space view now makes the membrane visible: a thin enclosing
boundary contains standing relationships and bounded express lanes. Pending
approaches without such permission sit outside it; closed/revoked history sits
outside as well. A pending request on a permitted relationship does not displace
that relationship from the interior. Each relationship portal has a continuous
closed outline, a broad opening for standing permission, or a narrow passage
with two boundary marks for an express lane. Text labels carry the same meaning,
so access is not communicated through geometry alone.

Rust's existing client projection now supplies a typed `PermissionRoute` alongside
the permission label. Both come from one match over the canonical relationship
state, and the narrow Swift binding forwards that type. Swift receives the route;
it does not infer membership from labels. This describes the recorded arrangement,
not current authorization to send: lane scope, expiry and allowance checks stay
with their existing owners. Cached views remain snapshots.

Selecting a visible correspondent card expands its boundary into their framed
portal over a short, 0.42-second transition; returning contracts it into the
communication space. Sidebar navigation remains available. Both the native member
app and preview use the same `EditorialSpaceView` and geometry namespace. "Read request" or
"Open correspondence" expands that frame into the reading surface with a short
local transition. System reduced-motion preferences disable motion; the preview
also exposes a local motion toggle. Opening and returning change only navigation;
both surfaces remain mounted to retain reading position. Reading never opens a
permission route. A return control leads back to the communication space, and
the sidebar supplies a familiar alternative to spatial navigation. Separate
conversation selection and side-by-side sources await richer client projections.

During an operation the member view disables correspondent selection. It also
requires the displayed conversation and selected sender to match before showing
a portal or executing a decision, preventing stale correspondence from supplying
the context for a different selection. These are presentation safeguards; no
permission or settlement rule changed.

The standalone design-preview executable has been retired. Visual review now uses
the member executable with one local-demo account, its real encrypted inbox, and
visible sidebar destinations for Communication space, Directory and My approach page.
The shared editorial presentation remains the member application's presentation.

Validation scope for this presentation slice: compile the shared views and native
preview, inspect layout and interactions, parse the member view, and attempt the
existing quality gate. No new tests or domain-rule changes. Full backend journeys and
iOS/device behavior require separate validation. Earlier presentation passes were
blocked because the sibling identity checkout lacked `identity-contract` and did
not match `identity-source.sha256`. Pulling identity-model from GitHub on 10 October
restored all pinned sources without changing the manifest. PostgreSQL suites were
explicitly skipped in those earlier passes; they provide no evidence from those runs.

Observed on 10 October 2026: the native preview compiled and launched; visual
inspection covered light and dark appearance. Conversation selection, the
external-email disclosure label, an established conversation without decision
buttons, accept/reject explanations and the empty inbox were exercised through
the native UI. Swift parsing and whitespace checks passed. Full member-view
type checking, live decisions, resizing/accessibility audits and iOS behavior
remain unverified.

The subsequent sans-serif and membrane/portal pass also compiled and launched.
Native inspection confirmed separate Requests/Relationships headings, explicit
standing/lane/revoked labels, overview-to-reader navigation, and the same scroll
position after returning and reopening. A revoked example retained its historical
message without decision buttons; an external request retained its disclosure
label. The empty inbox was exercised again. These are presentation observations
over fictional content, not new evidence for server authority or settlement.
At that stage, the existing quality gate was attempted again with PostgreSQL
explicitly skipped and remained blocked by missing/mismatched identity-model sources.

The spatial pass compiled the shared native presentation without warnings and
launched the membrane view. Native inspection covered closed approach portals,
standing openings, bounded passage marks, overview/reader navigation and the
preview reduced-motion path. Swift parsing, direct Rust formatting checks and
whitespace checks passed. At that stage, the Rust client projection/FFI additions
and the full member integration had not compiled against identity-model in this checkout;
the repository gate remained blocked. No new PostgreSQL evidence, accessibility
audit, lane-expiry experiment or iOS runtime evidence is claimed.

The subsequent card-to-portal pass restored identity-model by a fast-forward pull
to `f530e8c`, with all pinned hashes matching. The full Rust bridge, generated
Swift bindings, shared presentation, member view and native app compiled, and
the macOS app launched. The existing native harness passed SQLite durability and
exclusivity, isolated Keychain creation/reopen/refusal, browser enrollment through
the local identity service, encrypted-cache reopening and denied custody checks.
Native preview interactions covered standing and bounded portal navigation,
the reduced-motion path, and a closed request remaining closed while reading.
Reading position survived overview/reader navigation. No tests were added and
no domain permission or settlement rules changed. Live request decisions through
SwiftUI, a full accessibility audit, lane-expiry behavior and iOS runtime behavior
remain outside this pass's evidence.

The full repository quality gate subsequently passed all eight checks, including
the existing workspace and PostgreSQL suites, with
`CS_MAIL_TEST_DATABASE_URL=postgresql://localhost/cs_mail_portal_gate_1010`.
The controlled enrollment journey used separate fresh member and identity
databases (`cs_mail_portal_member_1010` and `cs_mail_portal_identity_1010`), fictional
browser identities and simulated banking. This establishes local integration
evidence; it does not establish real-provider assurance or release readiness.

## Build and run

```sh
scripts/apple-build.sh
open target/apple/CsMail.app
```

The script generates bindings, builds the native static library and Swift package,
and produces the unified development app. Once a signing identity is selected,
subsequent builds reuse it; otherwise the bundle permits visual review only. Generated binding files are ignored
and regenerated from the Rust interface. The Mac app uses a single window and a
client worker actor; blocking network work never runs on the UI actor.

For the persistent local backend, the **first initialization** requires two empty
local PostgreSQL databases. Later starts use the same databases and the same state
directory; they neither reset the databases nor regenerate keys. For example:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/apple_mail \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/apple_identity \
  scripts/apple-local.sh
```

The state directory defaults to `target/apple-local-state`; set
`CS_MAIL_LOCAL_STATE_DIR` for a separate validation environment. Database URLs,
ports and directory identity are bound in a private manifest. Changing any of them
requires a separate environment. Initialization refuses nonempty databases,
missing durable keys, or occupied ports. An advisory lock excludes a second
launcher; shutdown waits for companion processes before releasing ownership.
The cs-mail and identity databases, simulator records, deployment/evidence keys,
and OIDC signing key persist. OAuth codes and browser ceremonies remain transient.
Operator bootstrap and configuration use existing idempotent Rust commands.

The default ports are 8480–8482. The prepared personal development environment
uses 9693–9695, databases `cs_mail_personal_development_1011` and
`cs_mail_personal_identity_development_1011`, and the default state directory.
Restart it with:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/cs_mail_personal_development_1011 \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/cs_mail_personal_identity_development_1011 \
CS_MAIL_DEMO_PORT=9693 CS_MAIL_DEMO_IDENTITY_PORT=9694 CS_MAIL_DEMO_OIDC_PORT=9695 \
  scripts/apple-local.sh
```

The controlled browser provider offers Marcus plus fictional participants Maya,
Daniel, Elena, Jules, Noah, Alice and Bob. These are local fixture logins, not
real-user authentication. Every account must still enroll through the existing
identity application with distinct fictional evidence and simulated banking.
Naming a fixture does not create an account, relationship or financial effect.
There is no hosted service or external email in this environment.

Select an existing code-signing identity once, then build:

```sh
scripts/apple-signing-setup.sh
scripts/apple-build.sh
```

The selection script reads existing valid identities in Keychain and records only
the selected certificate fingerprint. It changes no certificate trust. Set
`CS_MAIL_APPLE_SIGNING_IDENTITY` when more than one identity is available.
Account packaging refuses ad hoc signing. The bundle is signed and verified before
replacing the previous bundle. The app uses the preserved development service at
`http://127.0.0.1:9693`. The build keeps the original account configuration;
it does not import or replace a device credential.

The selected signer on this Mac is the existing Apple Development identity. The
signed app automatically reopens saved device state and synchronizes; offline
failure preserves a visibly dated snapshot. Unfinished enrollment reconciles its
existing operation. Custody failures never replace the stored credential or make
another account. Browser cancellation fences callbacks from an older ceremony.
The app shows that banking and payments are simulated. It has one account-backed
execution path and no separate demo mode.

The persistent owner account is enrolled and has claimed `marcus@csqd-mail.com`
through the ordinary signed address publication workflow. Its public name is
Marcus; all enrollment evidence is fictional and service/payment activity is
simulated. The app reopens this account as its normal experience. The development
profile starts listed and contains one illustrative introduction class with 500
minor units collateral plus the operator's 50-unit processing component.

This Mac uses Chrome for browser login. Set `CS_MAIL_LOGIN_BROWSER_BUNDLE_ID` when
building to select an installed browser; the choice is retained under
`target/apple-signing/login-browser.txt`. Without a saved choice, macOS uses its
system authentication browser. The development app has its own callback scheme,
`com.csqd.member.development`, separate from older preview bundles. The controlled
provider presents an explicit return link so Chrome can return to the signed app
without weakening the login-page security policy. Rust still checks issuer,
state, nonce, PKCE and enrollment possession.

Fictional counterparts have separate development credentials and use the normal
signed CLI workflows. Provision or resume them with:

```sh
cargo build -p cs-mail-cli
python3 scripts/apple-local/provision-participants.py \
  --state-dir target/apple-local-state --recipient marcus@csqd-mail.com
```

The script binds the participant stores to this recipient, excludes concurrent
provisioners, waits for canonical coverage, and preserves journals on uncertain
outcomes. It does not recreate completed requests on rerun. The CLI fixture keys
are local development files protected by directory/file permissions; the owner's
credential remains in Keychain and is never imported into these stores. All
canonical effects pass through signed services; no database population bypasses
authority or settlement. The current app contains Maya's pending request, Daniel's
standing contact, Elena's seven-day personal/human-initiated express lane (20
messages/day), and Jules's rejected approach. Noah's public profile remains
visible with new requests unavailable because he has no purchased service.

A terms refusal creates no request or charge. The CLI retains that refused
operation's journal and retires only its unfinished approach plan, so a later
approach can start with a new operation. Transport failures retain the same plan
and signed bytes.

Device enrollment does not buy annual service or publish request classes. Those
remain separate workflows. The portable client and Swift binding now expose
signed purchase and class-publication use cases. My approach page now exposes
class/profile editing and the configured simulated annual-service purchase;
general billing and financial management remain separate work. Each setup request is committed to the encrypted setup
journal before transmission, and a retry retains the exact signed bytes.
A purchase acknowledgement does not imply funded service: current coverage comes
from the authenticated account view. The app displays an empty inbox until a
sender has submitted a request to a properly configured recipient.

Request confirmations now retain the reviewed sender and request ID. Selection
changes or locking dismiss the confirmation. Swift checks that the same request
is displayed, and Rust refreshes canonical state and requires that exact request
to remain awaiting a decision before signing. A later request from the same
sender cannot silently replace the reviewed request.

Rejected-request financial outcomes include a quiet "How the pool works" link.
Its visibility comes from the Rust financial projection's recorded nonzero
collateral forfeiture, forwarded as a typed flag; Swift does not infer it from
permission or status labels. The sheet explains pending forfeiture and maturity,
the previous calendar year's shared pool and equal shares for qualifying members,
and CSQD's separate processing charge. It does not estimate a personal payout.
The shared native preview includes Jules Morgan's fictional rejected request.
The full app and preview built; native inspection verified link placement,
sheet opening/dismissal, and absence on a pending request. The full quality gate
passed, including PostgreSQL. No tests were added;
live settlement, iOS and a full accessibility audit were not repeated for this
explanatory presentation change.

## Existing identity model and browser configuration

The client calls `/v1/enrollments`, `/complete`, `/renew` and the reconciliation
endpoint. It uses the existing enrollment intent/device proof and never decides
subject matching, login ownership, bank assurance or review outcomes. The local
`browser-identity` host uses the real identity application, PostgreSQL adapter and
`OidcJwksSessionVerifier`; only its loopback HTTP fixture hosting differs from the
production HTTPS host. The fixture emits signed RS256 JWTs, single-use codes and
S256 challenges. It does **not** establish a real person's identity.

`cs-maild` publishes this optional public configuration at `/v1/deployment`:

```json
{
  "browser_login": {
    "issuer": "http://127.0.0.1:8482",
    "client_id": "cs-mail-apple",
    "redirect_uri": "com.csqd.member.development:/oauth/callback"
  }
}
```

Register a public native client with code flow, S256 PKCE and this exact callback.
Issuer/client ID must match the identity service's OIDC configuration. A production
issuer and endpoints use HTTPS. Only loopback HTTP is allowed for local work.
Browser state, code verifier, tokens and raw applicant claims are transient. The
encrypted reservation stores a details fingerprint and the existing enrollment
intent, so restart resumes the same ownership claim. It never persists the ID token.

“Open account” reopens a device already enrolled here. Existing-account enrollment
on a replacement device remains a separate task using the identity model's
`RecoverDevice` contract; there is no new-account fallback and no profile importer.

## Local data and interrupted work

Keychain service/account: `com.csqd.member.development` / `member-device-v1`, device-only,
unlocked access. SQLite: `Application Support/com.csqd.member.development/member.sqlite`.
It contains one authenticated encrypted snapshot, using a separate Keychain cache
key. Cached message records remain HPKE ciphertext even inside that snapshot.
SQLite uses full synchronous commits; a process lock prevents concurrent owners.
No secret/debug formatting or HTTP response bodies appear in client errors.

Recipient-key registration and decisions persist the exact signed operation before
sending it. A lost response retains uncertainty; retry replays those bytes. Only a
validated provider receipt resolves a decision journal, followed by a canonical
refresh. The UI clears plaintext when locked/backgrounded and discards late results
from an older presentation session. Browser sign-in can be cancelled explicitly.

## Validation and current limits

The agreed claims concern secret/plaintext leakage and incorrect outcomes after
interruptions. Rust experiments challenge expired/substituted browser responses,
restart of enrollment, lost decision responses, failed local commits, ciphertext
integrity and denied key access during content retrieval. Existing service and
PostgreSQL suites cover canonical decisions and idempotence.

Run the native checks against a fresh local deployment:

```sh
CS_MAIL_APPLE_SERVER=http://127.0.0.1:8480 \
  apps/cs-mail-apple/.build/debug/CsMailAppleCheck
```

The harness checks native SQLite durability/exclusivity, actual isolated Keychain
creation/reopen/overwrite refusal, and Swift-to-Rust browser enrollment against the
existing identity service. Its fictional browser approval is performed over HTTP;
it does not automate Safari. Temporary native test credentials/files are removed.
This executable is used because the installed Command Line Tools lack XCTest.

Validated locally on 8 October 2026: macOS build and launch, enrollment form and
system authentication session startup, the controlled login page in Chrome,
native persistence/Keychain/FFI enrollment
checks, and the repository quality gate including PostgreSQL suites. Visual
inspection of the Safari authentication window was unavailable because Computer
Use lacked permission. Full visual sign-in, an interactive request decision, real
provider/bank assurance, signing/notarization and App Store distribution remain
unverified. This is the first development slice, not the full Milestone 4 exit.

An iOS Xcode target shares the same Swift package and Rust facade:

```sh
# Requires full Xcode and the relevant installed rustup target:
# aarch64-apple-ios-sim or aarch64-apple-ios
scripts/apple-ios-build.sh simulator
scripts/apple-ios-build.sh device
```

Full Xcode and iOS Rust targets are absent on the current machine. The project
plist parses, but iOS compilation, packaging and suspension/device behavior have
not been validated. The local loopback provider works on the Mac or simulator;
a physical phone needs a reachable HTTPS development deployment.

## Address publications and directory — 11 October 2026

The sidebar navigates between **Communication space**, **My approach page** and **Directory** in the same window. The editor publishes the
current device address's member-authored name, description, optional image, visibility
and up to eight request classes as one signed, revision-checked operation. Initial
publication is listed; visibility can be changed independently of access through
an exact reference or shared link. Image selection uses Apple's decoder to prepare
a bounded PNG and strips the input file's metadata. The screen displays the current
operator collateral bounds and processing component. Saved quotes are unaffected.

Directory search shows public publications; direct lookup includes unlisted ones.
A recipient detail shows currently quotable classes and their conditional-charge
rules. Class selection is review only: it does not authorize payment or contact.
Suspended/closed accounts return unavailable, without a reason. Expired coverage
leaves information visible and disables selection. An empty class collection offers
no paid request route. The sidebar separates standing contacts, express lanes and
closed approaches, retaining the existing Rust permission projection.

Portable Rust owns validation, signing, the encrypted publication journal and exact
retry. Explicit publication refusals resolve that operation; transport failures do
not. Reload retrieves canonical state. A second address needs its own recipient-key
authority; this slice does not introduce alias creation or device-key management.
The local service exposes `POST /v1/directory/commands`, `/search` and `/lookup`.
The public query endpoints support the future website, but this pass adds no website
or hosting. Discovery uses existing numeric public persona references and `/r/` links;
exact lookup also recognizes configured generated member email addresses. Human-readable mailbox claims were added in the subsequent owner-account cutover
recorded below; richer search and additional-address lifecycle remain later work. Search pages are live views, so edits
between pages can move entries; no frozen cross-page snapshot is claimed.

Migration 0031 adds address publications and operation receipts, with no replaced
reader or duplicate class store. Use fresh isolated development databases for validation;
older evidence databases remain untouched.

Validation completed locally on 11 October 2026. The full gate passed all eight
checks, including 113 ordinary tests/doctests, 94 PostgreSQL cases and the pinned
identity source manifest. Four new PostgreSQL experiments challenge foreign-address
(including same-account alias) authority, receipt-failure rollback and concurrent
edits, unlisting/account status/coverage, and replay over later edits. One portable
client experiment challenges lost responses and failed local acknowledgement commits,
requiring identical signed bytes after encrypted-store reopening.

The final macOS build and native harness passed SQLite/Keychain checks, browser
enrollment against the existing identity application, PNG publication, directory
search, unlisting, shared-link lookup and reopening. Native visual inspection found
and repaired editor clipping. Actual SwiftUI controls published the fictional Alice
profile and existing class, removed it from browsing through unlisting, opened it
through exact lookup, and preserved name/description/classes/unlisted state after
quit/reopen. Selecting a class displayed its 1550-minor-unit conditional charge
without authorizing a charge. Profile images were exercised by the native FFI
harness; the graphical file chooser was not separately exercised.

The final isolated demo runs against `cs_mail_directory_visual_1011` and
`cs_mail_directory_visual_identity_1011` on ports 9493–9495, with its encrypted
store at `target/apple/directory-final-recipient-1011`. The normal member store
was untouched. An earlier demo credential refused access after an ad hoc rebuild;
it and its database were preserved, and no replacement credential was written
into that store. Same-binary quit/reopen passed in the final demo. Earlier harness
checks used separate `cs_mail_directory_native_1011` and identity databases.

Not covered: public-web behavior/indexing, iOS runtime, a full accessibility audit,
remote image-decoder abuse campaigns, additional
address/device management, cross-page snapshot consistency or sending a paid request
from the directory. This slice publishes and reviews approaches; it adds no new
payment authority, public hosting, bank connector or external mail sending.

## Historical visual validation — October 2026

The offline visual fixture mode used in the October 11 navigation experiment has
been removed. Those observations did not establish canonical service outcomes.
The current application has one account-backed execution path. The subsequent
presentation observations below retain their original validation scope.

The navigation correction was checked in the rebuilt offline macOS demo: search
for Daniel, open his profile, refresh it, return to the same filtered results,
and open Maya through a shared link before returning to those same results.
The native build and all eight quality-gate checks, including PostgreSQL, passed
(`/tmp/cs-mail-directory-back-build.log`, `/tmp/cs-mail-directory-back-gate.log`).
No automated tests were added under the agreed scope. Live-service navigation,
paginated results and iOS runtime behavior were not exercised in this UI check.

Readable member-screen text inherits native text selection from `MemberView`,
including directory profiles, explanations and status text. The standard macOS
Edit menu supplies Copy/Paste; no separate clipboard implementation is needed.
Contact and navigation rows remain buttons that open their readable detail views.
Native verification selected Daniel's profile description by dragging, copied it
with Command-C, and pasted it with Command-V into both Directory search and the
profile description field. The editor draft was reloaded without saving.
The build and full quality gate passed (`/tmp/cs-mail-text-selection-build.log`,
`/tmp/cs-mail-text-selection-gate.log`). No automated tests were added; iOS selection
and every individual text/control combination were not exercised.

Clicking the selected “Ways to approach me” class now clears the selection and
its conditional-charge explanation. Clicking another class switches the single
selection. Native macOS verification covered selecting Alice's introduction,
switching to research collaboration and clicking research again to clear it.
The native build and full quality gate passed (`/tmp/cs-mail-class-deselect-build.log`,
`/tmp/cs-mail-class-deselect-gate.log`). No new automated tests were added under
the agreed visual-review scope; live-service and iOS interaction were not exercised.

The Swift presentation consolidation replaces label-string navigation with
`MemberDestination`, introduces distinct presentation identifier structs, moves
Directory's search/profile/return/refresh/failure state into `DirectoryModel`,
and exposes model output read-only. Session and operation enums replace stored
flags. Scoped errors preserve unrelated content and ordinary refusals no longer
mark the connection offline. A Swift actor replaces `ClientWorker`'s manually
confined queue and unchecked Sendable declaration. See
`docs/apple-client-architecture.md` for the boundary and ownership details.

Validation retained the agreed scope, with no new automated tests. The native
build and all eight existing quality-gate checks, including PostgreSQL, passed
(`/tmp/cs-mail-swift-state-build.log`, `/tmp/cs-mail-swift-state-gate.log`). A complete
Swift concurrency-check build of CsMailApple passed without source diagnostics
(`/tmp/cs-mail-swift-concurrency.log`; SwiftPM's user-cache access warnings remain).
Native offline checks covered contact/request navigation and its decision
explanation, filtered Directory search, unavailable lookup preserving results,
screen switching retaining search, profile refresh and return, shared-link lookup,
class selection/deselection, editing/saving a profile, Copy/Paste into search,
and quit/reopen resetting demo edits. The actor's live-service call path, browser
cancellation, response-loss recovery, paginated results, image selection and iOS
runtime were not exercised in this visual pass. Existing Rust suites remain
separate evidence for canonical behavior.


## Historical durability groundwork — before owner enrollment

Durable-development validation on 11 October 2026 used the existing suites and
native harness, with optional coordination added to its existing enrollment/reopen
scenario for a supervised-service restart; no new test cases were added. The
first restart attempt exposed shutdown/port-reuse races; the launcher now waits
for children and permits ordinary TCP restart reuse while refusing live listeners.
The final observation used `cs_mail_durable_check_1011_2` and
`cs_mail_durable_identity_check_1011_2`, with state in
`target/apple-durable-validation-2`. Its 12 deployment/evidence/OIDC key digests
were identical after restart, the reopened native account ID was identical, and
the account count remained one. The native check also passed encrypted SQLite
replacement/exclusivity, isolated real Keychain creation/reopen/overwrite refusal,
directory editing/discovery/unlisting, and custody-denial checks
(`/tmp/cs-mail-durable-native-final.log`). The existing interrupted-enrollment,
response-loss decision/publication and local-commit recovery suites passed in the
full eight-check quality gate (`/tmp/cs-mail-durable-gate.log`). The stably signed
app built (`/tmp/cs-mail-durable-final-build.log`), preserved its designated
requirement across rebuilds, and native inspection confirmed development labeling
and the saved 9693 enrollment endpoint. At that groundwork checkpoint no personal enrollment had been submitted;
the prepared personal database contained zero accounts. The completed cutover
below supersedes that status. Existing account stores
and the first validation attempt's databases remain preserved.

Groundwork limits at that time: persistent **personal** account quit/reopen and
credential access across an app rebuild still required actual enrollment; the native harness
used an isolated fictional account. Replacement-device recovery, aliases, active
counterpart provisioning, live banking/payments, public email and iOS runtime are
not established by these observations. The subsequent cutover adds checked mailbox claims; broader naming and
address-lifecycle policy remain separate work.


## Persistent owner-account cutover — 11 October 2026

Migration 0032 adds a unique normalized public mailbox to the address directory.
Rust's checked `MemberAddress` distinguishes it from `ProtocolIdentity`; a typed
`DirectoryReference` selects exact protocol or mailbox lookup through one storage
projection. The signed publication atomically claims the mailbox, saves the
profile/classes, and records its receipt. The protected absence/ownership check
and PostgreSQL uniqueness constraint exclude competing claims. This development
slice limits claims to the configured namespace (`csqd-mail.com`), reserves the
old generated-address prefix, and supports simple lowercase names containing
letters, digits, hyphens and periods. Once claimed, a mailbox cannot be removed or
renamed by profile editing. Additional naming and reclamation policies remain
future work. This is local address assignment, with no DNS or external delivery.

Public profile names populate native correspondence projections; Swift does not
hardcode live participant names. The Rust client now journals standing, rejection
and scoped express-lane evidence as distinct command alternatives and checks the
same provider receipts before clearing uncertainty. Service activation displays
canonical current coverage separately from command acknowledgement. Successful
publication refreshes cached Directory results.

Validation used the agreed existing gate/recovery suites and native checks, with
no new automated test cases. The existing cross-address PostgreSQL experiment was
extended to challenge exclusive mailbox claims, refusal without class effects,
and normalized direct lookup. All eight gate checks passed, including PostgreSQL
(`/tmp/cs-mail-account-cutover-gate.log`). The native SQLite/Keychain, interrupted
browser enrollment, directory/image/unlisting/reopen and custody-denial checks
passed against fresh isolated databases on 9793–9795
(`/tmp/cs-mail-cutover-native-check.log`).

Native interaction enrolled the owner with fictional evidence in Chrome, claimed
the requested mailbox, published an introduction class, activated simulated
coverage and read encrypted participant messages. Standing acceptance and an
express lane each showed a confirmed simulated 550-unit refund; rejection showed
50 units retained and 500 units pending forfeiture. Directory Back navigation,
unlisted exact mailbox lookup, unavailable Noah and publication editing were
checked. The owner account, contacts and unlisted setting survived a signed rebuild
and quit/reopen without a credential prompt. A publication attempted with the
backend stopped remained pending after offline quit/reopen, then replayed after
restart; its receipt count increased from two to three exactly once, restoring
listed visibility. All six account IDs and all 12 deployment/evidence/OIDC key
digests were unchanged. No canonical database record was written directly.

Not covered: replacement-device recovery, iOS runtime, live identity/banking or
payments, public hosting/DNS/email, additional address management, a full
accessibility audit, or paid sending from the Directory UI. The owner's enrollment
is a persistent local development account and supplies no verified real-world
identity or mailbox-delivery claim.


The final signed build reopened against the restarted personal backend. Native
inspection confirmed the listed six-profile directory, exact lookup of
`marcus@csqd-mail.com`, its introduction class, and the populated communication
space. The app was left on that screen. An older running copy was closed after
bundle replacement prevented further UI control; saved state was preserved and
the final copy reopened successfully.

The browser is used for development enrollment. The app opens the local OIDC
fixture; selecting Marcus authorizes a fictional login assertion, not legal
identity or bank ownership. Rust binds the returned code to the attempt, and the
identity service validates signed evidence and device proof. See the
[production boundary](../../docs/apple-client-architecture.md#browser-evidence-and-production-boundary).
Production authentication/verification providers and real payment integration
are not established by this simulated journey.
