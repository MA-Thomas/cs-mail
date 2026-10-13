# Apple member application

Decision agreed 8 October 2026. Build the member application with SwiftUI,
shipping macOS first and designing for iPhone and iPad from the outset. This
replaces the planned Tauri shell; there is no implemented Tauri application to
migrate or retain. The first native implementation now lives in
`apps/cs-mail-apple`; its runbook and current evidence are in that directory’s README.

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
