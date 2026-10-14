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
