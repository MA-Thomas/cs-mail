# Apple member application

Decision agreed 8 October 2026. Build the member application with SwiftUI,
shipping macOS first and designing for iPhone and iPad from the outset. This
replaces the planned Tauri shell; there is no implemented Tauri application to
migrate or retain. The first native implementation now lives in
`apps/cs-mail-apple`; its runbook and current evidence are in that directory’s README.

The [Swift design principles](swift-design-principles.md) guide presentation and
interaction. Keep routine loading and coordination behind the scenes; surface
information when it helps a member understand or act.

The [build plan](../cs_mail_build_plan.tex) owns sequencing and milestone exits.
The [Rust-domain principles](rust-domain-principles.md) and existing product,
identity, correspondence and consent contracts remain binding.

## Ownership and dependencies

The application follows this boundary:

```text
SwiftUI views and Swift presentation state
                  |
       narrow generated Swift bindings
                  |
       portable Rust client use cases
                  |
       authenticated service APIs
                  |
       Rust application and domain owners
```

Server Rust remains authoritative for account, permission, request, settlement,
billing and distribution decisions. The client never writes canonical tables.
The Rust client owns portable signing, content encryption/decryption, protocol
handling, command recovery and synchronization workflows. Existing client code
is reused and extended as needed; this decision does not claim those workflows
are all complete.

Swift owns screens, navigation, presentation state, accessibility and Apple
integration. Platform adapters implement the Rust client's ports for key custody,
local persistence, transport, time and lifecycle services as needed. The exact
adapter implementation language follows the platform requirement; domain rules
do not move into Swift or get duplicated at the binding boundary.

Express-lane and address metadata refreshes on entry, on returning to the active
app, and periodically while those workspaces remain active. One toolbar Refresh
action targets the current workspace. Routine observation timestamps and separate
reload controls are not shown. Abandoning a read preserves the active session and
committed navigation; cancellation of submitted work retains its original Rust
journal and recovery path. Approach-page refresh preserves unsaved edits and
requires an explicit choice to discard them if the saved revision has changed.
Recovery actions appear only for unresolved work. Money remains in explicitly
labelled simulated units until the contracts supply a currency and scale; Swift
must not infer either from a numeric settlement-unit identifier.

## One application, one backend path — 13 October 2026

The member application has no demo mode or parallel preview app. `MemberModel` and
`DirectoryModel` share a concrete `ClientWorker`; all workflows use the portable
Rust client and canonical service. The transient fixture adapter and its alternate
mutation/read behavior have been removed. The macOS entry point opens the preserved
account with its original credential service and encrypted device store. The existing
local evidence/payment simulators and persistent fictional participant accounts are
backend development adapters, not an alternative application mode.

`apple-build.sh` packages one stable-signed app at the existing continuity path and
checks its signing requirement before replacement. Named review builds and their
packaging entry points are retired. Earlier signed app checkpoints remain recoverable
archives, paired with their matching databases and caches. The original Keychain
item identity, credential bytes, account/persona association and public keys were
checked unchanged across the installed update. Swift navigation and all previously
implemented native workflows now run together in this one account-backed app.

## Personal mailbox — 15 October 2026

The [19 October product decisions](../DOMAIN_MODEL.md#personal-reading-and-organization)
are implemented by Rust's retained-mail owner. SQLite stores independently encrypted
copies and per-message index records, addressed by opaque keyed identifiers. Each
transaction commits affected records and a revision marker together; failure leaves
durable state unchanged on rollback. Uncertain commit outcomes invalidate the cached
index and reconcile through the next durable read. Receiving metadata has its own
encrypted record and is not rewritten for each downloaded copy.

The unlocked session builds an inverted word lookup from encrypted index records
once, without reading message bodies. Queries select candidate IDs directly;
paging reads only those copies, with phrase verification and current expiry/removal
checks before snippets. Readers address individual copies and reuse the unlocked
index. Swift loads further result pages as they come into view, preserving the
query and loaded results on reader return. Lock drops the index and transient
results; a late download cannot restore a locked session's index. No local index
lock is held across network waits.

The first engine remains a portable Rust word index. It supports case-insensitive
words and quoted phrases without following references. The 64 MiB bound applies
to each encrypted record, not the aggregate retained mailbox. Cold opening and
memory use grow with the index; the separate account cache and receiving-metadata
snapshots retain their existing bounds. A cloud index and the FTS5/SQLCipher
comparison remain separate design work. Derived records grant no new authority.

The portable member client owns a separately authenticated personal-mailbox record
in the same SQLite store as the v4 device cache and v2 draft workspace. Absence means
the member has not used personal organization on this device. There is one current
reader, with checked account/persona association, distinct native/external thread
keys, bounded read identities and checked outgoing command/evidence restoration.
Format changes use an explicit preservation-checked development conversion; the
runtime has one current reader and never replaces the credential. A submission transaction now
commits all three ciphertext records or none; independent personal actions and
draft saving use the same short local lock without waiting for network work.

Read actions supply exact displayed message IDs. Inbox/Archive/Trash and flags
are personal choices, with Trash retaining its previous folder for restoration.
Neither organization nor reading changes communication permission or copy expiry.
Permanent native copy removal uses the existing conversation-command journal and
DeleteCopies service contract, retaining exact retry bytes on uncertainty. Swift
clears open content/search selections when removal begins and reloads retained-copy
availability; it offers no equivalent external permanent-deletion promise.

Reading projections preserve commit time, stream sequence, access outcome and
structured document parts. Endpoint search extracts only retained text from that
same projection, with no reference traversal. A search captures message identities
for stable bounded paging; its query/scope and Swift's transient results are cleared
when the session locks. There is no persistent plaintext index. Refresh separates
observation from mutation recovery and reports incomplete native/external refresh;
foreground/periodic scheduling belongs to Swift.

Outgoing records retain their exact signed native/external command, originating
draft revision when present, preparation time and terminal evidence. They share the
atomic draft/journal boundary. The existing execution journal remains the single
owner of unresolved mutations. Committed native delivery and external queue
acceptance have different typed outcomes; relay handoff and provider observations
remain separate structured facts. Historical submissions before this record was
introduced are not fabricated. Existing committed messages supply the Sent view.

## Device-local drafts — 15 October 2026

The portable member client owns a private draft workspace, separate from the
protocol kernel and communication permission. Distinct draft IDs, revisions,
targets and bodies distinguish native conversations/replies, external mail,
initial requests and follow-ups. A deterministic workspace transition takes the
current workspace, an explicit command and time, and returns its complete next
state. Checked restoration rejects invalid IDs/revisions, mismatched target/body
shapes, oversized writing and duplicate records. Drafts are device-local; they
establish neither remote backup nor replacement-device recovery.

The existing v3 encrypted account cache and original Keychain credential remain
unchanged in format and association. A separate v1 authenticated ciphertext
workspace uses the same credential's cache key with a distinct encryption context.
The device-store port adds independent workspace read/replacement and an atomic
cache/workspace submission transaction. SQLite stores opaque ciphertext in two
rows/tables, serializes writes and rolls back both records if either write fails.
There is no predecessor-workspace reader or account reenrollment.

Local draft calls have a separate short Rust lock and bypass Swift's serialized
network worker. Network snapshots write only the account-cache row, so they cannot
overwrite later writing. Draft updates check an expected revision. Submission
rechecks that exact revision after preparation and atomically moves the writing
out of the editable set while storing the original signed operation. No provider
I/O occurs inside that transaction. The submitted draft remains encrypted until
the journal resolves: success removes it, a definite refusal restores a newer
editable revision, and uncertainty retains the original operation. Request
cancellation and funding reconciliation continue through their existing owner.
Source selections retain exact version/coordinates/mode and are rechecked before
submission; previews and unrelated opened sources never enter the outgoing
package. Saving a draft never creates a charge or disclosure-consent grant.

Swift presents automatic local saving, explicit save/close and discard, a Drafts
list, save failures/retry, and Rust-projected address/text-limit issues. New
external disclosure consent is requested afresh, rather than restored from a
saved checkbox. Initial-request drafts are keyed to the selected approach class;
charge authorization still names the exact reviewed signed request quote.
The current cutover uses schema 43, device cache v4, workspace v2, document hash v3,
correspondence-command v5, request-document v3 and external-command v2. Older populated deployments
require explicit offline conversion; runtime fallback readers are absent.

New native-conversation and initial-request drafts include a subject. Rust supplies
a local date/time default for blank input, using the supplied timezone offset and an
alphabetic month. The subject is carried inside every encrypted native document;
a salted commitment binds creation, later messages and exact retries to the same
subject. Replies and follow-ups inherit it. External mail remains separate.

## Swift presentation ownership

The 11 October consolidation gives sidebar destinations a shared enum and uses
separate Swift structs for sender references, address references, request IDs,
request-class IDs and message IDs. Generated FFI records still carry wire strings;
conversion happens at the presentation boundary. These wrappers prevent accidental
substitution in Swift, but do not assert canonical validity or authority. Rust
continues to validate foreign inputs.

`DirectoryModel` owns search inputs, committed query/page context, navigation,
refresh, loading and failures. Its screen enum carries either results or a profile
with the exact results to return to. A failed lookup retains the current screen;
leaving and returning to Directory retains the session's search. Clearing the
session invalidates in-flight results before clearing public presentation state.

`MemberModel` represents locked/open sessions and idle/working/browser-authentication
operations explicitly. Read-only published output changes through model actions.
Failures retain their use-case scope and client-error meaning; directory errors
cannot clear unrelated conversations, and ordinary refusals do not declare the
connection offline. Rust journal metadata is reloaded after failed commands that
can retain pending work, without assuming rollback.

An actor owns the blocking Rust client. Calls execute outside the main actor and
accept Sendable closures/results; this replaces the manually confined queue and
its unchecked Sendable declaration. The actor serializes calls, while session
and directory generations reject presentation results arriving after a clear.
This is not a cancellation of already-submitted Rust operations.

The cancellation/concurrency pass uses Swift 6 language mode for the package and
iOS target. A Sendable presentation lifetime, protected by an allocated unfair
lock, invalidates queued worker calls before entering Rust. Locking or removing
the member view invalidates the session; Directory clear invalidates its reads.
Task cancellation is checked before invocation and before presenting a result.
Once invoked, synchronous Rust workflows finish their durable journal handling;
the UI reports stopped waiting without assuming rollback and requires reopening
or refresh before further decisions. Cancelled directory reads retain their last
committed screen without reporting a service failure.

Browser task cancellation terminates only the transient ceremony. A distinct
ceremony identity scopes delayed Rust browser cleanup so it cannot cancel a newer
sign-in. Durable enrollment reservations and mutation retry identities remain
owned by Rust. The remaining handwritten unchecked Sendable storage conformances
are supported by locked mutable access; generated UniFFI conformances are not
edited. iOS language settings are configuration, not device validation.

## Rust-to-Swift interface

### Navigation framework, 13 October 2026

The Swift presentation has six top-level areas, a reactive content list and a main
space. `MemberDestination` owns both the area and its detail route; a correspondence
route retains its originating area. `EditorialNavigationShell` owns transient
pointer/focus preview state. As of 16 October, every area uses its popup for
device-local layout controls. Home chooses cards/list within the existing permission
sections; Comms orders receiving-inbox/mailbox sections; Relationships chooses
column-B grouping and map/list overview; Express lanes orders alias/sender sections;
Economics orders records by date/name; Notes adjusts the current page's reading
width (writing is still deferred). Persistent column B owns receiving scope,
mailbox selection, searchable relationship groups, lane filters and financial
record direction/period filters. `MailboxNavigation` remains the one mailbox
presentation owner; detail round trips retain its query, selection and anchors.
Relationships keeps its organizer while aliases are managed in the main space.
The main-space owner supplies history return controls for details without their
own, including alias permission views and nested inboxes. Navigation checkpoints
capture mailbox context (query, folder, sorting, selection and stable anchors),
organizer filters and alias detail selection alongside the existing receiving
scope and correspondence focus. Returning restores that snapshot rather than
relying on intervening screens to leave mutable presentation state untouched.
The old multi-area `EditorialInboxView` is removed; the lane renderer is now
`EditorialLaneOrganizer`, with member-specific organizers in the Apple adapter.
These use Rust-supplied facts and do not decide eligibility, financial outcomes
or sending authority. Saved searches, broader finance and Notes remain separate
work.

The hover correction gives pointer-opened and explicitly keyboard-opened previews
distinct typed sessions. One shell pointer observer projects the hovered destination
and popup presence from measured visible regions in one coordinate space. Padding
has no hover authority. Incidental focus cannot overwrite or retain a pointer
preview; explicit keyboard previews retain their own focus. These are presentation
lifecycles, with no server command or journal effect.

Every area has an explicit configuration session: clicking its icon opens
the workspace and pins the controls; clicking it within that workspace preserves
the current detail and search. Escape, Close or an outside content click dismisses
the controls. Layout changes do not navigate. Non-sensitive grouping/overview
choices persist in device-local AppStorage and are intentionally not rolled back
by navigation history. Recent ordering uses available message/request timestamps,
with name and sender identity tie-breakers; permission remains a Rust projection.

Main-space portal buttons use a separate perimeter style over the supplied
`EditorialBoundary` shape. Hover strengthens its stroke without filling the
interior or changing its standing opening or lane markers. The full card remains
the click target; the decorative stroke does not participate in hit testing.
List and rail rows retain their existing region style. This changes presentation
feedback only, with no permission interpretation or command effect.

The existing correspondence view stays mounted behind other destinations, retaining
reading and composer state without creating another reader or journal. Session lock
removes it as before. Hover reads already-projected summaries and performs no
credential access or decryption. Whole-region buttons leave message selection and
the popup composer with their existing owners. Layout changes adapt the rail into
a row without replacing the content view. Rust contracts and durable operation
identities are unchanged. Notes and annual-finance interfaces remain deferred.

Introduce the narrow client FFI facade during Milestone 4, alongside its first
consumer. Expose client use cases and checked results, not arbitrary domain
mutation or database access. Preserve distinct identifiers, bounded values,
explicit ownership and stable error meanings across the interface. Validate
foreign inputs using the same checked Rust construction paths.

Calls that may wait run asynchronously without blocking the UI. Define callback
delivery, object lifetimes and cancellation explicitly. Cancelling a UI task is
not evidence that a submitted server command was cancelled: preserve operation
identity and reconcile an unknown outcome before showing success or retrying.
Swift presentation updates return to the main actor.

The first slice uses UniFFI 0.31.0, a Rust static library and a Swift package.
The development targets are macOS 14 and iOS 17. The macOS build is validated;
iOS simulator/device packaging and execution still require full Xcode. An
XCFramework for distribution remains later packaging work.

## Keys, local data and platform behavior

Apple Keychain backs the key-custody port. Secret key material must not enter
SQLite, logs or diagnostics. SQLite holds ciphertext, key references, cursors,
minimized relationship metadata and nonsecret preferences. Plaintext needed for
reading or composition is transient; the UI does not create a plaintext cache.
Keychain denial, locked or unavailable keys, expired authentication and unavailable
network service have explicit states, not silent fallbacks.

Account authentication, retained-copy entitlement and scoped consent remain
separate authorities under the existing consent and key-custody contract. A new
screen or a successful login does not create decryption or disclosure authority.

Push notifications and background execution are platform services. Treat a
notification as a prompt to synchronize, not canonical evidence of a decision.
Client correctness must survive suspension and resume without relying on a
continuously running process. Configure each platform's key access and background
behavior deliberately rather than assuming macOS behavior transfers to iOS.

## Shared Apple interface and later platforms

Create macOS and iOS targets from the first slice. Share presentation models and
appropriate SwiftUI components while adapting navigation, windows, menus,
keyboard/touch interaction and accessibility to each device. Check the main
journeys on iPhone early; iPad layout belongs to the same Apple design. macOS is
the first release target; an iOS release date is not committed by this decision.

The guest request page remains a web interface with its own components. Share
terminology, design intent and API contracts with it; do not require SwiftUI and
the web page to share rendered components. The membrane/portal interaction
concepts remain independent of the rendering toolkit.

Later Android and Windows applications reuse the portable Rust client and supply
their own interfaces, bindings and platform adapters. Kotlin/Compose and C#/WinUI
are candidate interfaces, not selected implementations. Their binding support and
platform behavior require separate validation. Phoros's interface choice is a
separate decision.

## First implementation slice and validation

**Unified alias creation implementation (21 October 2026).**
Use one New alias journey. Collect owner-visible purpose/description, a chosen
address or a request for generation, and an explicit receiving-policy choice:
Require a bonded request, or Allow contact through this address. A bonded alias
has its own profile and request classes; directory visibility is a separate choice.
Offer explicit sender grants where useful. Do not infer policy or authentication
from labels such as Hospital or Amazon. There is no public/private selector or
single-current-public-alias rule in the revised product model.

Rust owns validation, authority, generation, revision checks, complete consequences
and exact-operation recovery. Swift collects intent, preserves unfinished work and
navigation context, and presents the verified result in plain language. Address
possession and purpose grant no permission. Alias-scoped creation explicitly
authorizes its lane and Rust commits both atomically; a deliberately staged journey
must preserve and explain partial outcomes without asking the member to manage
internal operations. A pending outcome must not invite duplicate creation.

The unified flow replaces generated-private/chosen-public controls and the
single-public-alias limit. Alias-scoped creation commits its explicit lane atomically;
a bonded alias then receives its own profile, classes and explicit visibility choice.
The Apple runbook records validation and running-app status. Fee collection is future work: a nominal
alias charge associated with the annual utility cycle is a direction only, with
amount, recurrence, timing and failure treatment unresolved. Do not display or
collect an invented fee. The [domain model](../DOMAIN_MODEL.md#aliases-and-express-lane-scope)
owns the rules and open policy-switch, lifecycle and billing decisions.

**Further alias clarification (2026-10-14; implementation pending).** One
participant protocol identity may control multiple alias routes without public
linkage. Sender-identity blocks apply across all receiving aliases. Aliases
have no automatic expiry and offer Restrict, Replace, Retire. Suspicious or
insufficiently authenticated alias mail is held for review without ongoing
permission or payment. SMTP envelope recipients and authenticated native targets
establish receiving-route evidence. The [domain model](../DOMAIN_MODEL.md#aliases-and-express-lane-scope)
owns these rules and remaining delivery, transition, reply and held-message contracts;
implementation and validation records below do not establish this cutover.

Start with one local member journey: browser sign-in and first-device enrollment, reopen it,
synchronize a request, read its content, and accept or reject it through the
existing authenticated service. Establish Keychain custody and encrypted local
storage in this slice. Then expand to existing-account device recovery, composition, external
conversations and disclosure controls, request classes, lanes, blocks and
financial views. Milestone 4's full exit remains the build plan's responsibility.

The agreed experiments challenge the existing milestone
claims: secret leakage through persistence or diagnostics, and disagreement with
canonical server outcomes after quit/reopen or projection rebuild. Consider
Keychain denial, interrupted commands, expired authentication, offline use and
iOS suspension as concrete conditions for those experiments. The FFI packaging
check must include macOS, an iOS simulator and an iOS device build; device runtime
behavior requires its own execution evidence. The current runbook separates
completed local checks from unavailable platform and provider checks.

Development remains local against simulated payments and controlled email
transports. Public hosting, MX, HTTPS and actual provider callbacks stay deferred
until the macOS application is built and locally validated. No provisioning or
hosting payment is part of the client implementation stage.

## Enrollment boundary

The Apple client reuses `identity-contract::EnrollmentIntent`, the existing
cs-mail enrollment endpoints and the sibling identity-model application. Identity
resolution, OIDC signature verification, bank evidence, uniqueness and review
remain there. The client adds browser ceremony orchestration (authorization code,
PKCE, state and nonce), Keychain device-key generation, and recoverable registration
of recipient/content keys. It does not define another identity model.

The controlled local OIDC fixture supplies signed fictional login evidence to the
existing `OidcJwksSessionVerifier`. This validates wiring, not real-user assurance.
Returning to the same enrolled device uses its registered keys. Signing an existing
account into a replacement device must use the existing `RecoverDevice` contract;
that HTTP/UI ceremony is not implemented in this slice and must never fall back to
creating another account. No development-profile import path is retained.


## Persistent local development account

The 11 October groundwork retains one unified Swift app and uses the ordinary
Rust account/device enrollment and signed client workflows for both the owner's
persistent development account and fictional counterpart accounts. The cs-mail
PostgreSQL database owns accounts, publications, relationships, journals and
financial effects. The separate identity PostgreSQL database owns enrollment,
review and keyed evidence fingerprints. Device keys remain in Apple Keychain;
SQLite retains authenticated encrypted client progress and admitted ciphertext.
Bank evidence and payment outcomes come from the existing simulation adapters.
No special personal-account branch writes canonical records.

The local launcher now preserves deployment, evidence and OIDC signing keys,
binds them to the databases and ports, excludes concurrent launchers, and replays
idempotent bootstrap/configuration through Rust. Losing a key or changing the
binding is an explicit refusal, never automatic replacement. Account bundles use
a stable selected signing identity and are verified before replacing the previous
bundle. The existing Apple Development identity is selected on the owner's Mac;
no certificate trust setting was added. Preview signing cannot open persistent
account enrollment in the packaged app. Connected presentation is explicitly
labeled development with simulated banking and payments.

Rust projects the kind of pending device/setup operation to Swift. Reopen and
retry preserve the existing journal and operation identity; a custody failure is
not a new-account instruction. Old browser callbacks cannot finish a later
ceremony. The owner account is now enrolled with fictional evidence and has
claimed `marcus@csqd-mail.com` through a signed publication. Fictional participants
own independent credentials and exercise signed enrollment, service purchase,
request issuance, encrypted delivery and settlement. The runbook records the
completed same-device recovery observations and their limits.

`MemberAddress` owns checked normalized mailbox syntax; `DirectoryEntry` owns the
once-claimed address attachment. The application authorizes the signed publication
against the deployment's explicit namespace, current address key, revision and
pricing context. Storage protects claim absence/ownership and commits mailbox,
profile, classes and receipt together, with a unique mailbox constraint as a
second boundary. `DirectoryReference` distinguishes protocol references from
mailboxes and both use one canonical lookup projection. Searchability remains
independent of address ownership: unlisting never relinquishes a claim. Renaming,
release and reclamation are outside this development slice.

Correspondence names derive from member-authored publications and convey no
identity verification. Rust's pending decision command represents relationship
commands and signed lane grants as separate alternatives, retaining exact bytes
and receipt digests across uncertainty. Swift presents a reviewed seven-day lane
choice; Rust constructs the signed evidence and the existing server owns scope,
permission and atomic settlement. Service coverage is an authenticated account
projection, distinct from purchase acknowledgement.

A selected external macOS browser returns through the development app's distinct
registered callback scheme. Its configured bundle ID is a local packaging
preference; iOS retains the system authentication-session adapter. The controlled
login page uses an explicit return link so the browser can preserve its strict
form policy. A resumed enrollment refreshes browser configuration while pinning
the original deployment, provider key, product and login issuer/client. The
ownership reservation and device keys never change during that refresh.


## Browser evidence and production boundary

The Swift browser adapter opens an authorization URL constructed by the portable
Rust client. The browser page is served by the separate Python loopback OIDC
fixture, not by the Rust identity service. Choosing a fixture login instructs
that provider to assert the selected development subject; it checks no password
and establishes no real-person identity. The explicit return link carries a
single-use code to the app. Rust binds the callback to the attempt with state,
exchanges the code using PKCE, and checks issuer, audience, nonce and expiry.
The identity service verifies the token signature and enrollment/device proof
before applying its evidence and review policy. Fictional bank evidence comes
from the configured simulation adapter, independently of browser authentication.

A production deployment could retain this authorization-code shape while using
real authentication and bank-verification providers. Authentication establishes
control of a provider login; policy determines what separate evidence establishes
about enrollment. Signed assertions are not sufficient merely because they are
signed. Provider selection, assurance policies, real funding rails and production
operations remain work. Claiming a cs-mail address does not prove control of an
existing external inbox. Chrome is a local Mac packaging choice, not a product
requirement; same-device reopening uses stored credentials without browser login.

## Native member workflow completion — 12 October 2026

Rust owns the quote-review/authorization boundary, formation stages, exact signed
request/document journals, acknowledgement validation and cancellation recovery.
Swift displays these stages and asks for explicit authorization of the issued
terms and authored message. No-charge, existing-request, expired-quote, refused
and unresolved outcomes remain distinct. Current sender/recipient request views
are separate, and request money is presented alongside Q&A rather than in the
recipient's decision flow.

Native mutation refusal is an application-declared receipt-only consequence,
committed under the same account/contact/policy protection as successful mutations.
It does not retain ciphertext. Exact replay returns the historical refusal;
transport or unresolved-policy failures do not establish that terminal outcome.
The client resolves a send only against its matching commit/refusal and saves that
resolution before allowing replacement work.

Each thread carries its own answer permit and received/sent request linkage.
Swift filters displayed decisions by the selected thread and passes the displayed
request ID. The singular request-conversation projection is removed. Contact
controls reuse the permission journal and server-owned transitions. Policy edits
have an independent exact-byte journal; source inclusion uses the existing Rust
document/selection/provenance API. References are resolved only through current
retained-copy authority. Closing a reference view invalidates delayed presentation
results without declaring a backend command cancelled.

The existing native check sequence was extended to challenge the consequential
journeys on fresh local fixture deployments. No new test cases were added.
The original owner credential/account/address and signing requirement were checked
without replacing keys or writing the preserved original cache. Full gate and
signed macOS build passed; interactive visual checks, iOS, replacement-device
recovery, annual financial/external-email UI and facade crash injection remain
outside this pass. See the Apple runbook for concrete evidence and limitations.


## Composer-led source selection — 13 October 2026

The contact portal opens directly onto a list of conversations, each with its
content-derived label, preview and last-message date. A row opens one conversation;
its received/sent requests and reply action use that thread's projected authority.
New conversation and request initiation have separate visible entry points. The
combined-history picker and reading-view reuse menu are removed.

A native composer owns its in-memory source inclusions. Its adjacent source panel
opens a current authenticated retained message through Rust and displays exact
flattened blocks. Native UTF-16 text highlights become explicit UTF-8 ranges; images
select whole blocks. Reference markers are nonselectable. The source version and
ordered elements cross the binding boundary; Rust's checked MessageSelection owns
validity and Document owns extraction/provenance. References also validate plaintext
boundaries before send preparation. No whole-message fallback exists. Closing the
source panel invalidates delayed reads, and locking clears draft/source plaintext.

The agreed existing native experiment challenges partial Unicode quotation and
reference preservation, cross-block coordinate mapping, surrounding-text exclusion,
and refusal of invalid/substituted selections before a send journal is created.
Interactive layout, focus/clipboard behavior, image selection and iOS remain
separate validation; compiling the native view is not evidence of those behaviors.


## Principles repair — 13 October 2026

Every received native conversation establishes a confined reply route. Conversation
receipt evidence owns that scope; relationships own a separate reply-withdrawal
epoch, independent of request/payment generations. Broad reverse-lane agreement
is separately recorded at request submission and cannot be re-established by
ordinary receipt evidence. All mailbox publication and corresponding receipt
evidence commit together. Per-request answer permits are removed.

The source picker loads contact detail explicitly and keeps the draft destination
unchanged. Returning to the same person performs a real reload. Typed protocol
outcomes verify against their signed receipt commitment; signed billing responses
include durable refusals, which resolve setup journals without treating transport
failure as refusal. Funding projections preserve capture lifecycle distinctions.

The schema/cache cutover uses a preservation-checked offline upgrade and prepared
encrypted artifacts with durable recovery intent. Original identity and Keychain
credentials persist. No old runtime cache reader or parallel reply implementation
is retained.

## External-email client boundary — 13 October 2026

The portable member client now owns external authored-text preparation, scoped
consent commands and exact signed-operation retry through the existing service.
Its optional external workspace records an uninitialized or observed external
mailbox in the one encrypted format-3 snapshot; there is one cache reader and no
credential replacement or separate storage path. The workspace stores encrypted
message copies and signed commands, not plaintext. An acknowledgement resolves the
operation before fetching updated views, preventing a failed refresh from inviting
a fresh duplicate. Swift presents external conversations under Communications with
a popup composer and explicit consent/permission confirmations. It does not grant
permission, authenticate email, or derive delivery success.


## Receiving inbox projections — 16 October 2026

Every native message record requires a receiving assignment. A checked
`AdmittedDelivery` binds that record to its encrypted package; the delivery effect
also requires the conversation update and carries any allowance consumption.
Requests sign their target alias. Ordinary native sends sign a public receiving
alias; alias-lane sends use their separately authorized target. External received
messages require assignment data in their enum variant, while outgoing copies and
unadmitted incoming content have distinct representations. External conversations
fix the member's receiving alias at creation for subsequent reply-token delivery.

PostgreSQL enforces alias ownership and conversation participants. Receiving history
is a view of the message records, with no independently writable delivery-route
table. Historical assignment provenance is distinct from live permission; migration
assignments are excluded from admission-derived alias permissions. Explicit
development migrations may assign placeholders, preserving the original evidence;
there is no runtime allowance for unassigned received correspondence.

The member client derives inbox views and alias-search keys from those same records.
Encrypted metadata retains the owned alias catalog, not a second routing list.
Lock clears the active catalog and search session; unlock restores them locally.
Unavailable alias metadata cannot produce a workspace that silently places received
messages only in All inboxes. Search still uses its session index and targeted copy
reads, without reopening the account snapshot for each result page.

A mixed-alias conversation retains its scoped native/external conversation key.
Alias projections contain only matching message IDs, unread counts and activity;
search snapshots bind both query and inbox scope. The Rust personal-state owner
rejects read/unread identities outside that scope. Swift shows those messages and
can explicitly open the full conversation. Inbox selection and origin are part of
the navigation checkpoint. Existing conversation-wide organization remains in
All inboxes; per-alias organization and sending-address selection are not inferred.

Comms column B selects All inboxes, active alias inboxes or historical alias inboxes.
Descriptions stay owner-only. Express lanes separates sender lanes from private
alias lanes and exposes only explicitly recorded receiving scope for sender lanes.
The native alias-send API requires an explicit known participant and alias target;
it is not an alias directory or a privacy-preserving address resolver.

The standalone MX host may be provisioned with an explicit `alias_registration`
capability carrying the member host's same protected relationship/principal
derivation material. It can then open an Unknown guest relationship for a sealed,
authenticated private-alias arrival; this creates neither an invitation, standing
permission, charge nor outbound disclosure consent. One shared guest-pair creation
path retains existing history linking. The host still has no account/provider
signing, relay-decryption or migration authority. Without that optional capability,
a new guest pair is unavailable rather than derived under a different secret.
Alias-aware public approach links and detailed held-mail/reply contracts remain
separate from this receiving-inbox implementation.


## Readable alias previews — 21 October 2026

`cs-mail-application::aliases::generation` owns name normalization, fragment
construction, occupancy preference, unbiased suffix selection, length bounds and
bounded collision retries. It receives randomness and reservation facts explicitly.
The existing alias transaction supplies protected namespace facts and authenticates
non-reserving suggestions. Generated creation and replacement sign the exact
reviewed candidate; the old server-selected 128-bit address path is removed.

The portable client verifies the signed suggestion and keeps mutations in the
existing encrypted exact-operation journal. A confirmed address collision is distinct
from a stale revision or uncertain response. Swift holds unfinished name, fragment
and preview intent across sheet closure; changing name/fragment invalidates the old
preview and fences delayed results. It never implements generation rules. Both
receiving policies use the same generator. No account, credential, schema or existing
address conversion is involved.
