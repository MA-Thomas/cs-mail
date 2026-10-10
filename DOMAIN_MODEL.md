# C-SQD domain model

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Confirmed product rules, updated September 29, 2026. Start here when changing
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

Each protocol identity (email address) publishes its own set of up to eight
user-defined request classes describing the approaches it welcomes. Addresses of
the same account have independent sets. Each class has its own collateral `S`,
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
recipient's behalf. A publication is authorized only by a key of the address it
concerns and is independent of any relationship. See the
[request pricing design](docs/request-pricing-design.md).

The recipient may accept a request with standing directed permission or with an
express lane. Both modes record a full `C + S` refund obligation and commit the
granted permission with settlement. An express lane concerns the relationship;
it may be scoped, for example by purpose, time, or volume, but is usually not
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

A lane grant also resolves a pending request when issued through lane management.
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
