# C-SQD domain model

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Confirmed product rules, updated October 19, 2026. Start here when changing
the code or documentation. These rules record the user's confirmed clarifications.
The [deployment profile](cs_mail_deployment_profile.tex) gives the detailed
financial-program requirements; the [protocol specification](cs_mail_protocol_spec.tex)
governs requests and communication permission. The
[Rust architecture](cs_mail_rust_reference_architecture.tex) describes their
implementation. Implementation choices must not silently become product rules.

Native conversations and onward reuse follow the separately agreed
[headless conversation contract](cs_mail_membranes_portals_conversation_policy_source/HEADLESS_CONVERSATIONS.md).
It defines the one-to-one correspondence scope, two-party policy relaxation,
reference/copy lifetimes, and the atomic encrypted-mailbox delivery boundary.

Managed decryption and replacement-device message access follow the
[consent and key-custody contract](cs_mail_membranes_portals_conversation_policy_source/CONSENT_AND_KEY_CUSTODY.md).
Authentication, retained-copy entitlement and scoped consent are separate
authorities. CSQD decryption requires consent; it is not prohibited categorically.

## Person, account, and authority

The [product identity model](docs/product-identity-model.md) defines one durable
login identity per product, multiple authentication methods, and the shared
subject association across cs-mail and Phoros. A shared subject identifier does
not establish biological identity or satisfy Phoros's separate enrollment policy.

- For the current product, one identifiable person has one account and may have
  multiple email addresses. Identity is tied to a verified bank account.
- Enrollment is open to anyone who completes a fresh login and bank-ownership
  verification. There are no invitations. The one-person-one-account rules are
  the only gate:
  - A cs-mail login belongs to one account; reusing it is refused.
  - A person is identified by their evidence: legal name, date of birth, bank
    account, address and phone. The applicant states all five. The name, address
    and phone must be ones the bank reports for an owner of the verified account;
    the date of birth is the applicant's statement.
  - Evidence is compared exactly, after normalizing case, spacing, punctuation and
    phone format, against every other cs-mail enrollment. An enrollment that shares
    its bank account, or any two fields, with another is held for review. It cannot
    activate until a CSQD operator approves it; a denial is final for that enrollment.
    An approved enrollment may share a bank account with another account.
  - The identity service owns this evidence and the review, and keeps the evidence
    only as keyed fingerprints. cs-mail never stores it.
  - Each product sets its own evidence policy. Phoros will require different
    evidence, and enrollments are never compared across products.
  - Verifying the phone by text-message code is deferred.
- Account, member, private principal, and public communication identity are
  distinct roles and scoped identifiers for that person, not permission to create
  multiple accounts or member shares. Keep bank and identity evidence out of
  messages, relationship records, and telemetry.
- Organizational legacy senders and possible future protocol participants do not
  establish organizational C-SQD accounts as part of this product.
- Guests (people using external mailboxes without a cs-mail account) may approach
  a member or be contacted first by a member. They are not accounts and do not
  enroll in the identity service or receive a subject. See
  [Guest senders](#guest-senders).
- Account actions require explicit account authority. Permission to send messages
  is not permission to change financial policy. Separate signing contexts do not
  require a second user-managed billing credential.
- Assume one configured payment-processing arrangement. A communication-service
  provider and a payment processor are different roles; federation terminology
  does not imply multiple payment processors.

## Aliases and express-lane scope

Agreed 16 October 2026. Address creation and express-lane permission are separate
product decisions. Rust owns an independently revisioned receiving permission,
signed pairing changes and admission. Swift presents those choices through the
same journaled client contract. The first Rust foundation implements
checked alias ownership,
lifecycle, conversation-specific approval, named receiving scopes, signed management,
atomic persistence and a deterministic route-authorization contract. Native management
now presents additional address records, lifecycle actions and exact-operation recovery.
The receiving-inbox implementation adds explicit alias-targeted native delivery
and authenticated SMTP envelope-address routing, with receiving evidence committed
alongside the message. Identity-addressed delivery remains explicitly without alias
attribution. Alias-aware publications, ordinary composer address selection, outbound
alias/reply addressing and held-message release remain separate work. Route
authorization is not proof that a message was delivered.
The [aliases and express lanes memo](cs_mail_aliases_and_express_lanes.tex)
explains the motivation and examples; this section owns the confirmed rules.

- CS-MAIL, operated by C-SQD, PBC, owns alias creation, routing, permissions,
  lifecycle management and correspondence. These capabilities must work
  independently of Phoros. Phoros Account, operated by Reservatory, PBC, will
  later use them through a linked CS-MAIL account.
- Each member has **one protocol identity**, with **at most one current public-facing
  alias** for unsolicited approaches and **multiple private aliases** for
  providers, employers and other contexts within the same account. These are
  communication addresses, not additional accounts or login identities.
- CS-MAIL generates an unpredictable email address for each private alias; the
  member supplies its **purpose/description** instead of choosing its address.
  The purpose/description is owner-visible metadata, separate from the email
  address, and is not disclosed unless the owner explicitly shares it.
- The member chooses the public-facing alias's email address. It may be replaced
  under the agreed replacement lifecycle; the previous alias remains permanently
  reserved and may complete its explicit transition, without becoming a second
  current public alias. The signed Rust management contract implements these
  creation and cardinality rules; this does not establish alias mail delivery or
  the alias-aware publication cutover.
- A **protocol identity** identifies a communication participant whom another
  participant can accept or block. An **alias** is a separately identified route
  controlled by that participant. One protocol identity may control multiple
  aliases. Creating, replacing or retiring an alias does not create or replace
  the protocol identity, account, login or shared subject. This supersedes the
  address-equals-protocol-identity product assumption; the existing implementation
  still uses address/persona coupling and requires an explicit cutover.
- Alias ownership must not automatically disclose the association between aliases
  to correspondents. Cross-alias enforcement does not authorize public linkage.
- A **private alias** is a receiving address, independent of express-lane
  permission. Creating it grants no lane. The member explicitly pairs it with
  alias-scoped or sender-scoped permission for the intended use. An unpaired
  alias supplies no admission authority; any other permission must independently
  authorize the named receiving route.
- An **alias-scoped express lane** authorizes correspondence through its named
  receiving alias, subject to admission policy, including from previously unknown
  senders given that address. A healthcare alias can be explicitly paired with
  this lane so referred specialists need no separate sender grant.
- A **sender-scoped express lane** explicitly authorizes a sender subject under
  its receiving scope and validity rules, including through request acceptance.
  A private alias for Amazon can instead be paired with a lane whose subject is
  a recipient-selected authenticated legacy domain. Possessing that address does
  not authorize other senders. Existing native-identity, external-mailbox and
  authenticated legacy-domain subjects remain distinct; a domain does not prove
  organizational ownership, and permitted domains are never inferred from names.
  Standing acceptance remains a separate permission.
- A public-facing alias grants no automatic lane. Existing standing permission,
  sender-scoped lanes and invitation rules apply. Approaches lacking permission
  use the existing bonded-request process through the public approach route.
  Private aliases offer **no bonded request route**, whichever permission is
  paired with them; a permission mismatch never creates a charge automatically.

- Permission through a private alias does not grant access through the account's
  public-facing or other private aliases. Restricting, retiring or replacing a
  private alias must not introduce a bonded fallback on that route.
- The receiving alias identifies the route the user supplied, not proof of the
  author's organizational identity or of who disclosed the address. Sender
  authentication is separate evidence; privacy of an alias is not a guarantee
  that nobody will forward or disclose it. External email retains its existing
  plaintext/disclosure boundaries.
- Linking Phoros does not itself grant access to correspondence, decryption or
  healthcare records. Annual service, identity and member-distribution rules
  remain separate; absence of a request charge does not waive annual service.

### Blocking and alias lifecycle

Blocking a sender protocol identity overrides standing permission, sender-scoped
lanes, alias-scoped lanes and invitations across every alias controlled by the
recipient. Changing the receiving alias cannot bypass the block. External sender
identity is limited to what authenticated evidence establishes: different
mailboxes must not be assumed to belong to the same person. Alias retirement or
replacement preserves identity-level blocks; unblocking alone does not recreate
previously terminated grants.

Private aliases have no automatic expiry; they remain active until explicitly
changed, subject to service coverage and current admission controls. The member
has three distinct actions:

- **Restrict:** stop new correspondents while preserving authorized existing
  conversations that already have admitted correspondence on that alias when
  restriction commits. Freeze those exact conversations, not a list of everyone
  who has ever written. New conversations require explicit approval. Held mail,
  failed attempts and prepared paid requests do not qualify. Restriction cannot
  restore consent, extend a grant or override a sender block.
- **Replace:** create a new alias with an explicit transition policy for the old
  alias. Preserve identity, blocks, correspondence and original receiving-route
  history. Do not automatically transfer unknown-sender admission from the old
  route to the new one. Transition duration and treatment of portal login,
  recovery messages and notices require an explicit policy.
- **Retire:** stop all inbound delivery through that alias and preserve received
  correspondence and history. Explain before confirmation that notices and
  recovery messages will no longer arrive through it. A retired alias address
  must never be reassigned to another member.

### Alias-scoped lane actions

Lane permission is managed separately from address lifecycle:

- **Restrict lane:** stop new conversations through the broad lane while preserving
  the exact conversations with admitted correspondence when restriction commits.
  Held mail and failed attempts do not qualify. Other independently granted
  permission remains separate.
- **Remove lane:** withdraw all authority supplied by that lane for future delivery,
  including existing conversations. Other independently valid permission may still
  authorize delivery. Received mail remains readable under its existing retention.
- **Replace with sender-scoped permission:** atomically withdraw broad access and
  activate the explicitly selected sender grants for this receiving alias. Earlier
  correspondents are not grandfathered. Native identities, exact guest mailboxes
  and authenticated legacy domains retain their distinct subjects and evidence.

The address and inbox remain in all three cases. Replacing the address is a separate
operation; restriction and retirement of a route constrain every permission using it.
Opening a broad lane again requires an explicit signed grant and does not release
held or refused mail. Removal is serialized with delivery: prior committed delivery
remains a fact; subsequent delivery needs current authority. Signed exact retries
return their historical outcome without reactivating superseded permission. Pairing
and its receipt commit together; allowance effects commit with message and route
evidence. No pairing action settles requests or grants disclosure consent.

Sender-restricted route grants use the existing signed sender-lane validity, scope,
authentication and allowance rules. The member explicitly selects their expiry and
message limit. Domain authentication establishes authorized domain use, not company
identity; the actual mailbox identity remains available for blocks and correspondence.

These actions introduce no bonded fallback on a private route. They do not erase
history, settle a paid request or imply disclosure consent. Replacement does not
assert that an external organization's portal or recovery address was updated.

### Receiving-route evidence and held correspondence

Every admitted message records the receiving alias and the permission basis used.
For SMTP, the accepted envelope recipient establishes the route, not the visible
`To` header. Native delivery binds the target alias into the authenticated command.
Preserve the route through admission, storage and replies; alias changes must not
rewrite historical delivery evidence. Outbound composition must identify the
selected sending alias; detailed reply-address behavior remains to be designed.

Admission resolves the route and its lifecycle, checks service coverage, evaluates
sender authentication, enforces identity-level blocks and evaluates the route's
permission policy. An unresolved sender identity must not be treated as evidence
that the sender is unblocked. Suspicious or insufficiently authenticated
private-alias correspondence is held separately for member review, with an
explanation and explicit controls. A hold is not ordinary delivery, an ongoing
permission grant or a payment invitation. It must not bypass an established block.
Detailed holding, release, retention and refusal contracts remain to be designed;
review or release must not silently authorize future correspondence.

### Remaining design decisions and implementation boundary

Sender-scoped grants and standing acceptance default to the receiving alias where
permission is granted. A recipient may explicitly select several named aliases;
there is no all-current-and-future-alias scope in the first implementation.
Creating another alias never expands existing grants. A replacement offers explicit
selection of permissions/conversations to carry forward; it does not infer transfer.
Permission opens selected entrances; blocking closes every entrance to that sender.

Still open: replacement transition policy and portal/recovery/notice continuity;
detailed outbound and reply addressing; held-message handling; privacy-preserving
identity references and alias-aware publication. The current cutover preserves
original signed evidence offline and converts recorded grants explicitly, without a
runtime predecessor reader. Do not infer all-alias permission from a sender-scoped
grant, or reuse sender-lane domain binding or expiry as alias-scoped policy.
Alias creation must not inherit pending-request settlement effects.

## Alias inboxes

Agreed 16 October 2026. Every alias has its own receiving inbox. Communications
column B selects an alias inbox or All inboxes; the main space browses and reads
that selection. All inboxes is a union of the same underlying correspondence,
not another copy or another read-state owner. Custom unions and organization
within individual alias inboxes remain later work.

Membership comes from the receiving assignment recorded with each message. Never
infer a historical alias from a sender, the current public address or a present
permission grant.
Every received message requires an explicit receiving-inbox assignment. All inboxes
derives its received correspondence from those assignments; it cannot contain an
unassigned received message. Existing development placeholders are assigned by an
explicit migration whose provenance is distinct from historical delivery evidence.
Retirement and replacement preserve the original alias's history. A conversation
may appear in several inboxes, retaining one conversation identity and the exact
receiving alias of each message. An alias-scoped reader shows and acknowledges
only that inbox's messages; opening the full conversation is explicit. Read state
is shared by message identity across the alias inbox and the union view.

Express lanes separates People and organizations (sender-scoped permission) from
Alias-scoped lanes (receiving-route permission). A private alias may be paired
with either lane scope; groups describe permission, not private/public address
classification. Receiving-alias filters use explicitly recorded grant scope.
Mailbox membership grants no permission, and a known sender's permission never
implies access through every alias. Shopping organizations may use private aliases
paired with explicitly authorized
sender-scoped lanes. An alias inbox exists independently of whether a lane is
paired with its address; membership remains a historical delivery fact.

## Annual service purchase

The full annual utility charge purchases a specified year-long service period.
Collection occurs well before that period starts. A failed collection and its
retry have the same agreed price and service period; payment timing does not
shift or extend coverage. New coverage requires verified finalized funding and
the current time to fall within that period. Late funding is not retroactive.

Keep four concepts separate: **collection date, service period, completed pool
year, and distribution date**. Do not derive them from a single billing-cycle
date. Exact calendar/anniversary anchors, collection lead time, and distribution
dates remain explicit policy decisions, not universal defaults. Multiple future
charges are not an assumed normal workflow.

## One annual distribution

One ordinary distribution payment covers the person's share `D_i` of the
**previous calendar year's company-wide collateral pool**. Qualifying members
receive equal shares under the published eligibility policy. The existing
once-only corporate assessment, restricted carryforward, and maturity rules
remain defined by the deployment profile.

For annual utility charge `U` (the clarified example is `$120`):

```
rebate/refund reporting component: R_i = min(D_i, U)
excess cash reporting component:   X_i = max(0, D_i - U)
single distribution payment:       R_i + X_i = D_i
```

| Allocation | Rebate classification | Excess classification | Bank payment |
| --- | --- | --- | --- |
| $80 | $80 | $0 | One payment of $80 |
| $120 | $120 | $0 | One payment of $120 |
| $170 | $120 | $50 | One payment of $170 |

The classifications are bookkeeping and reporting only. They are not separately
executed refund and payout legs and do not have separate completion states.
Processor-refundable capacity does not determine the rebate. A zero allocation
creates no external payment.

C-SQD automatically forwards the lump sum to the verified bank account associated
with the person's account. There is no independently chosen distribution
destination, per-distribution instruction, or member-selected payout minimum.
No cross-year aggregation or payment-threshold rule has been adopted.

Distribution is independent of renewal. Ordinarily it occurs during a service
year whose annual utility charge was already paid. A pending or failed charge
for the next year cannot delay, reclassify, or cancel the distribution. The
earlier service charge and the distribution are separate transactions; a
distribution does not revoke purchased coverage.

The distribution has one obligation and one payment lifecycle. Unknown processor
outcomes are reconciled under the same operation identity. A definite failure
can permit a new attempt for the same obligation; it does not create another
allocation. Failed, delayed, or reversed payments remain owed and are not
redistributed in the next year's pool. Authorized corrections must state their
cause and funding; they do not rewrite the annual allocation.

Closure preserves a minimal route for outstanding distributions and refunds.
It does not require renewal to receive money already owed. Full account
management, bank-account replacement, and detailed erasure policy remain deferred.

## Request classes, permission, and refunds

In the recipient's Apple request flow, request reading and decision confirmations
focus on messages, permission and deadlines. Financial breakdowns and general
explanations of charges, refunds, expiry and the member pool belong in the app's
separate Q&A section, as confirmed on 12 October 2026. This presentation choice
does not change financial terms, settlement or recorded payment outcomes.

Each alias offering the bonded route has its own publication of up to eight
user-defined request classes describing the approaches it welcomes. Aliases of
the same protocol identity have independent sets, authorized by that identity
for the named alias. Each class has its own collateral `S`,
within cs-mail-wide bounds `collateral_min <= S <= collateral_max`. CSQD, the only
provider, sets one processing component `C` for all of cs-mail; `C` and the bounds
form one versioned operator policy. The sender selects a class; that class and its
financial terms are fixed for the request. These are recipient-defined
invitations, not permanent relationship types or a CSQD ranking of purposes.
Misrepresentation can lead to ordinary rejection; there is no separate
misclassification penalty or reputation mechanism.

When the operator changes `C` or the bounds, issued quotes keep their terms. A
published class whose collateral lies outside the current bounds cannot be
quoted until the recipient republishes; it is never rewritten or clamped on the
recipient's behalf. A publication must be authorized by the controlling protocol
identity for the named alias and is independent of any relationship. The current per-persona
publication implementation requires an explicit alias-aware cutover. See the
[request pricing design](docs/request-pricing-design.md).

The recipient may accept a request with standing directed permission or with a
sender-scoped express lane. Both modes record a full `C + S` refund obligation and
commit the granted permission with settlement. A sender-scoped express lane
concerns the relationship; it may be scoped by purpose, time, or volume, but is usually not
scoped to a particular conversation. A request class does not itself grant
permission. Lane expiry or revocation does not reopen the settled request.

The request windows (submission and decision), the quote lifetime, cooldowns after
expiry and rejection, the retry backoff, and the member program's corporate share and
maturity are one versioned protocol policy that CSQD's operators publish. Issued request
terms keep the windows they were issued under.

Each request has one conditional charge. Expiry refunds `S`; rejection retains `C` and moves `S` into
pending forfeiture for the pool. Cancellation before submission returns the full
charge. These are real capture-linked refund obligations, unlike the reporting
classification within an annual distribution. Follow-ups add no charge. Class
switching cannot bypass pending-request limits or principal-recipient history.

A sender-scoped lane grant also resolves a pending request when issued through
lane management.
During preparation it cancels submission and voids or refunds the full charge;
a submitted request still pending is accepted with the full refund, including
after its decision deadline. Already-terminal financial settlements stay fixed.
The lane determines future access without a new bond, subject to its scope and
validity. Native correspondence enforces that permission across conversations.

The Rust implementation uses checked class publications, immutable signed class
snapshots, and atomic lane/settlement effects. See
[implementation and cutover](docs/request-classes-implementation.md). The operator
policy, per-address publication and quote pricing follow the
[request pricing design](docs/request-pricing-design.md).

Revoking standing permission or an express-lane relationship ends any retained
lane in that same direction. Blocking also ends it. The protocol declares the
lane termination, its expected revision and schedule cancellation; persistence
commits these with the relationship change and durable outcome. Neither action
retracts delivered copies or changes a completed settlement. Unblocking does not
reactivate the terminated lane; a later explicit grant is a new authority.

### Native requests and replies — agreed next implementation, 12 October 2026

These rules are confirmed product requirements. The request/document cutover and
member composing interface are implemented. The native workflow completion slice
adds explicit paid-request quote review/authorization, journaled formation,
follow-ups, contact controls, and source/policy controls. The subsequent native
UX cutover replaces whole-message inclusion with composer-led exact passage
selection and explicit person/conversation navigation. Scope and
validation limits are recorded in the [Apple runbook](apps/cs-mail-apple/README.md).
Replacement-device recovery and remaining financial/external-email interfaces
remain separate work.

The paired-request boundary is implemented as the first cutover slice.
`cs-mail-application::requests::RequestDecision` receives both protected direction
snapshots and declares the forward transition and any opposite preparation
cancellation together. The protocol owns cancellation and its financial effects;
the adapter commits them under one causal journal position. Sender blocking and
revocation are distinct refusal reasons. Guest requests use their existing path.
Native mailbox publication and automatic reverse lanes remain subsequent work.
Migration 0033 requires a fresh environment; preserved schema-32 databases are
evidence and cannot be opened with the new binary. They have not been reset.

- A native request begins one conversation. Its submitted message and follow-ups
  are ordinary correspondence documents in that conversation, with endpoint and
  owner-bound recovery copies. Acceptance continues that same conversation.
  Rejection, expiry and cancellation preserve already delivered copies.
- Every native conversation in which a participant receives a message permits
  replies in that conversation, regardless of whether it began with a request or
  ordinary correspondence. Replying does not accept or reject a request, and
  does not grant permission to start another conversation. This reply route
  survives rejection, expiry and expiration of a broader reverse lane. The sender
  may explicitly revoke or block the replying direction. Unblocking alone does
  not restore previous reply offers; subsequent received messages while that
  direction is open establish fresh conversation-scoped reply evidence.
- Accepting a native request with standing permission or a lane also activates an
  express lane in the reverse direction across the pair's conversations. Its
  purpose, allowance and lifetime are versioned operator policy, frozen in the
  request terms and agreed by the sender before funding. Authority derives from
  that agreement and the recorded acceptance, not a fabricated sender signature.
  The sender owns this direction and may grant standing permission, revoke or
  block it. Existing standing permission is never downgraded by acceptance.
  An existing custom lane that still provides relationship access is preserved
  with its own terms and remaining allowance; acceptance does not replace it
  with the operator-defined lane or refill it. When no such route exists,
  acceptance activates the lane agreed in the request terms.
- The contact overview offers New conversation when the sending direction has
  standing permission or a currently active express lane. Without that initiation
  route it offers Request a relationship, opening the person's published approaches
  and signed-quote review. While a received request remains within its decision
  window, the relationship-request entry is replaced by its pending notice and a
  link to review that request. Directory uses the same private request-state
  projection; its public invitations do not establish request eligibility.
  Existing conversation replies remain separate.
- Upgrading an express lane to standing permission is initiated by the recipient
  granting that direction. The sender is not asked to request an upgrade. Within
  three days of the lane's effective expiry, the recipient sees an in-app reminder
  to upgrade or let it expire. Letting it expire preserves the existing schedule;
  it does not revoke it early or remove received-conversation reply access.
- Submission is refused while the sender blocks or has revoked the recipient.
  It does not silently unblock or restore permission. Both directions are checked
  under the same protection as submission and delivery.
- A request conversation initially restricts reuse to its originating pair.
  Relaxation requires both participants through the existing conversation-policy
  mechanism. Rejected or expired content retains that restriction, including
  known ancestry through quotations and shared copies.
- Native conversations have a shared subject supplied by the sender at creation,
  visible to both participants and fixed thereafter. The subject is encrypted
  conversation content and searchable locally; it grants no authority. Agreed
  19 October 2026, replacing the no-stored-title rule;
  every new conversation requires a subject. Blank input receives a default using
  the sender’s local date and time, with an alphabetic month (for example,
  “Default subject: October-19-2026, 3:42 PM”). Existing placeholder conversations
  may be assigned subjects during the development cutover.
- For crossed requests, the first submitted request takes precedence. Its
  submission cancels any unsubmitted preparation in the other direction in the
  same complete consequence. Finalized funding is returned in full; uncertain
  funding is reconciled under the original operation. While an opposite request
  awaits a decision within its decision window, a new preparation is refused
  before new payment work is created. Submission rechecks this condition.
  A request past its decision deadline no longer prevents a new request,
  regardless of whether its expiry worker has run. Terminal settlements remain
  fixed; accepting an opposite request cannot revive a cancelled preparation.
- Guest requests and external correspondence retain their existing contracts.

The application coordinates these domain decisions against a protected context
containing both directed relationships, their current authority and revisions,
affected requests and funding states, conversation policy and staged documents.
One opaque decision declares every relationship, request, lane, conversation,
mailbox, ledger, audit and recoverable-work effect. Storage checks ownership and
revisions and commits that consequence; it never appends a business effect or
rewrites the protocol's settlement after the decision.

The cutover removes the replaced native request-content implementation and
reader. Ordinary schema migrations require a fresh development environment.
The separately authorized local development-account conversion operates on a
backup copy, verifying original account/persona IDs, registered keys, address,
permissions and exact message text while preserving the original Keychain item
and rollback environment. It adds no old-format runtime reader. Historical
absence of an offered broader reverse lane is retained as evidence; it does not
remove reply permission established by receiving a message. The explicit
continuity upgrade restores conversation receipt evidence while preserving
withdrawals, the original account and credential, and completed financial facts.
New requests still carry agreed broader-lane terms.
Retaining Keychain bytes alone never establishes account continuity in a newly
enrolled account. See the [conversion record](tools/development-continuity/README.md).

Agreed validation challenges atomic acceptance and reverse permission, refusal
under sender revocation/block, absence of new delivery after termination commits,
restricted reuse including derivatives, exact retry delivery, encrypted device
persistence, and crossed-request funding/expiry races. PostgreSQL is required
for atomicity and concurrency claims. Previously delivered copies remain readable
under their existing retention/custody rules; historical replay never supplies
new send authority.

## Private writing on a device

Agreed usability slice, 15 October 2026. Drafts belong to a portable client
workspace, not the protocol relationship kernel. Saved writing is private to
that device and is encrypted at rest. Saving, editing, restoring or discarding a
draft establishes no contact permission, source access, disclosure consent,
charge authorization or delivery. Native new conversations/replies, external
email, initial relationship requests and request follow-ups have distinct draft
targets. New-conversation drafts carry the sender's shared subject; replies
do not change it.

Submitting transfers one explicitly selected saved revision into the existing
operation journal atomically. A later edit cannot alter or replace that submitted
operation. Unknown outcomes retain exact retry identities; definite refusals
restore writing for review, without automatically trying another submission.
Request funding/cancellation continues under its existing financial lifecycle.
Source reuse and external disclosure still require their respective authorities
at submission; saving a source selection or recipient address is not authority.
Draft synchronization between devices and replacement-device draft recovery are
not established by this local workspace.

## Personal reading and organization

Agreed usability follow-on, 15 October 2026. The portable client owns device-local
read state, flags, Inbox/Archive/Trash placement and outgoing submission history.
These are private organization, not protocol states or sender-visible receipts.
Reading changes only the exact displayed message identities; synchronization
alone does not mark correspondence read. Manual Mark unread applies to the
selected snapshot. Native and external conversation identities remain scoped to
their respective streams.

Trash is reversible and restoration returns to the prior Inbox/Archive placement.
Neither Trash nor Archive changes permission, disclosure consent or content-copy
expiry. Native permanent copy removal is a distinct confirmed, journaled action
under the existing DeleteCopies contract. It removes the caller's endpoint and
recovery copies while retaining provenance and the other participant's copies.
Equivalent external permanent deletion is not established.

Agreed 19 October 2026: after unlocking, members can search correspondence retained
on that device without connecting to the service. Results explain device-local
coverage and any incomplete or unavailable content. Known local expiry and removal
rules apply offline, including to matches and snippets. Search does not mark mail
read, follow references implicitly or grant access or reuse authority.

The client stores authenticated copies and per-message index records independently,
encrypted at rest. Each incremental transaction commits the affected copy, index
record and progress marker together. Unlock reconstructs the in-memory word lookup
from index records without opening message copies. Connected downloads extend coverage. Search covers subjects and text, supports words
and quoted phrases, and clears transient results on lock. It distinguishes content
not downloaded from content known unavailable; neither becomes a false complete result.
Readers distinguish service commit time from provider-reported event time.

Sent derives from committed outgoing message facts. Device submission history
retains exact prepared commands and terminal evidence from this version onward;
earlier history is not invented. Native mailbox commit, external queue acceptance,
relay handoff and provider delivery observations remain distinct. Refresh observes
mail without resuming uncertain mutations; explicit recovery reuses the original
operation. The single unresolved mutation restriction remains in effect.

## Address publications and directory discovery

Agreed 11 October 2026; identity/alias ownership clarified 14 October 2026.
Accounts retain their protocol identities; each alias controlled by an identity
has its own public presentation, up to eight request classes where bonded requests
are offered, and directory visibility. A private alias must not become publicly
listed merely because it was created. The directory is a view of public-facing
publications, not a second class owner or a directory of all an identity's aliases.
The existing per-address/persona implementation below requires alias-aware cutover.

- Public profiles contain a member-authored display name, description and optional
  image. They are public statements, never copied from verified enrollment evidence.
- Initial publication is listed by default, with an explicit option to opt out.
  Enrollment alone publishes no profile. Setup explains visibility before saving.
- Unlisted addresses are excluded from name/partial-address searches and browsing,
  but remain accessible through an exact address or shared request link. A shared
  link locates public information; it is neither secret nor communication authority.
- The controlling protocol identity authorizes profile, visibility and class edits
  explicitly for the named alias. A signed edit for alias A cannot be applied to
  alias B, even under shared control. Common ownership alone does not authorize an
  edit. Combined saves commit profile, visibility, class publication and operation
  evidence atomically. Exact key-delegation and alias-binding contracts await design.
- Suspended and closed accounts disappear from search; direct lookup returns a
  neutral unavailable result without revealing the reason. Suspension preserves
  configuration for resumption. Closed accounts cannot reopen.
- Expired service preserves public information but prevents new requests. An empty
  class publication is valid and offers no paid request route. Classes outside the
  current operator bounds cannot be selected; issued quotes keep their terms.
- Initial search covers published names, address references and descriptions, with
  deterministic ordering and bounded pagination. It does not rank by collateral.
  Pagination is a current view, not a frozen snapshot across edits.
- Searches beginning after unlisting commits exclude the address. Previously
  displayed information cannot be recalled. Unlisting does not change existing
  requests, quotes, permission or settlement.
- Edits bind a reviewed revision and stable signed operation. Exact retries return
  their historical outcome without overwriting later edits. Altered retries conflict;
  transport failures retain uncertainty. Explicit refusals are recorded distinctly.

The local Rust/Swift slice uses the existing public persona references and shared
`/r/{persona}` links. Exact lookup also accepts the deployment's generated member
email addresses when external mail is configured. The subsequent persistent-account
slice also supports a uniquely claimed, normalized human-readable mailbox in the
explicitly configured development namespace, including `marcus@csqd-mail.com`.
This is local address assignment, not external inbox verification or delivery.
The current claim is immutable through profile editing; supported naming syntax,
renaming, release and reclamation are implementation scope or deferred policy,
not additional confirmed product rules. See the
[Apple runbook](apps/cs-mail-apple/README.md#persistent-owner-account-cutover--11-october-2026).
Images are uploaded bytes, not remotely fetched URLs. The initial
operational bounds are 160 name bytes, 2048 description bytes, a 256-KiB PNG with
at most 512 pixels per dimension, and 20 entries per search page. Header validation
is not proof of image-decoder acceptance; Apple prepares and displays images using
its platform decoder. No public directory website is launched in this pass.

## Guest senders

A guest is a person using an external mailbox without a cs-mail account. A guest
may approach a member through a paid request, or a member may initiate an email
conversation with that mailbox. Individuals use the mailbox path; organizational
legacy senders use the legacy-domain path.
The full design is in [guest senders](docs/guest-sender-design.md).

- A guest mailbox maps at the edge to one synthetic protocol identity, following
  the legacy-sender precedent. Guest-initiated requests require mailbox
  verification; a member-selected destination alone is not verified. The request
  kernel is unchanged. Lanes can name a guest mailbox as their subject.
- Mailbox verification establishes control of the mailbox, not personhood. The
  payment processor's instrument fingerprint is an anti-abuse signal, not an
  identity claim.
- Guest request history is grouped by both mailbox and payment fingerprint.
  Evidence that links two histories merges them prospectively into the most
  restrictive state. Several mailboxes and several payment instruments can still
  produce separate histories; each approach still costs `C`.
- A guest request has the same single conditional charge and settlement as any
  request. Refunds return to the funding payment method. Forfeitures enter the
  member pool; guests receive no distributions.
- Standing acceptance permits free ongoing communication; an express lane permits
  free communication within its scope and validity, subject to the recipient's
  block and revocation, by ordinary email in both directions. The
  member's replies come from the member's protocol identity address. The guest's
  inbound mail is admitted only with DMARC passing and an exact match to the
  verified mailbox; a reply token alone is never sufficient.
  A blocked or revoked guest's mail is rejected at the SMTP level. Automatic
  replies go only to senders whose domain passes DMARC.
- The recipient sees the guest's email address, and the guest is told so before
  paying. Guest content is not end-to-end encrypted. Each member reply by email
  requires that conversation's explicit downgrade consent.
- Mailbox and payment-fingerprint evidence is retained indefinitely by default
  and disclosed to the guest before payment. Outbound guest mail is sent through
  an email delivery service, signed with the deployment domain's key; the member's
  downgrade consent discloses that the service sees reply plaintext.
- Converting a guest into a member while keeping relationships is deferred.

### Member-initiated external conversations

Agreed 2026-10-06. A member may select an external email address and send first.
The member is responsible for choosing the destination; this does not create a
verified guest session, identity-service subject, or proof of mailbox control.
There is no separate inbox-control challenge for replies to this invitation.
Incoming email still requires automatic authentication, exact author-mailbox
matching, and the addressed conversation. A reply token alone is insufficient.

The guest may send unlimited replies within that pending conversation, without
an account, payment instrument, or request charge. There is no fixed reply count
or paid-request decision deadline. Ordinary transport resource limits still apply.
The member may read the replies and choose Accept, Reject, an express lane, or
Block. Pending access grants no permission outside that conversation. Acceptance
and lanes use the existing directional relationship rules, including lane scope,
validity and allowance. Later revocation or lane expiry cannot reactivate an old
pending invitation.

Reject closes that unpaid invitation without changing paid-request history,
settling money, or creating a block. Subsequent contact through the ordinary paid
request route remains possible under its existing rules unless blocked. Only the
member can explicitly open a new unpaid invitation; a new subject line, retry,
or guest-selected conversation identifier cannot reopen the rejected one.

The first send and subsequent outbound emails require explicit conversation-scoped
consent to disclose plaintext to the external relay and mailbox. Withdrawing that
consent cancels messages still waiting inside cs-mail. Consent withdrawal and
beginning dispatch are serialized: withdrawal that commits first prevents dispatch;
after dispatch begins, cancellation may be too late. A crash or uncertain handoff
remains an unknown outcome, never an automatic second disclosure. Restoring consent
must not silently revive cancelled messages. An inbound permission change is not
itself withdrawal of outbound disclosure consent; blocks prevent new dispatch.

The initial implementation is authored text through signed member commands and a
development relay. The receive-only SMTP adapter now admits plain-text replies and
selects plain text from paired plain-text/HTML alternatives for known conversations
with automatic authentication, STARTTLS and durable deduplication.
Email provenance does not assert human involvement or a declared purpose.
The SMTP2GO adapter submits signed plain-text MIME after committed dispatch and records
authenticated provider events independently from handoff. Lost responses remain uncertain;
provider acceptance does not establish delivery. One live SMTP2GO email reached the
recipient. A fresh live follow-up confirmed RSA-2048 deployment signing and correct
fast-accept receipt handling. Provider callbacks have a separate loopback listener
and can run while outgoing delivery is disabled; public callback verification is pending.
The 2026-10-08 continuation adds general member-address reception, a request-page
prompt for unknown authenticated senders, acceptance-to-email routes for paid guests,
transactional SMTP2GO notices and explicit consent restoration. New inbound
conversations begin without outbound disclosure consent. A route, its first admitted
message and any lane consumption commit together; paid acceptance routes commit with
their notice attempt. Existing blocks, service coverage and lane constraints still
apply. Request-page prompts do not retain the original email body or grant permission.
They are bounded to one per mailbox/member per 24 hours and suppressed for automatic
or unauthenticated mail. Weak-authentication confirmation is not part of this pass.
See [the implementation and experiment record](docs/external-email-journey.md).
Native quotation/export, public MX deployment and public provider callbacks remain
outside the locally validated journey. Guest funding still uses the simulator.

## Enforcement boundaries

Rust types and validated transitions distinguish immutable service terms,
verified evidence, member preferences, operator policy, and payment states.
Durable transactions must still check current funding restrictions at dispatch,
serialize enrollment screening so overlapping applicants cannot miss each other,
and prevent concurrent duplicate payments.
A previously constructed Rust value cannot establish that external state has
remained unchanged.

See the [implementation record](audit/domain-model-implementation-2026-09-15.md)
for implemented APIs, acceptance evidence, and remaining deployment work.
