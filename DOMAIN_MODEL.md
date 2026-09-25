# C-SQD domain model

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Confirmed product rules, updated September 21, 2026. Start here when changing
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
- Account, member, private principal, and public communication identity are
  distinct roles and scoped identifiers for that person, not permission to create
  multiple accounts or member shares. Keep bank and identity evidence out of
  messages, relationship records, and telemetry.
- Organizational legacy senders and possible future protocol participants do not
  establish organizational C-SQD accounts as part of this product.
- Guests (people without an account who approach a member through the request
  page or by email) are senders, not accounts. They do not enroll in the identity
  service or receive a subject. See [Guest senders](#guest-senders).
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

Recipients publish up to eight user-defined request classes describing the
approaches they welcome, and select collateral `S` for each from
the deployment's bounded menu. The sender selects a class; that class and its
financial terms are fixed for the request. The operator sets processing component
`C`. These are recipient-defined invitations, not permanent relationship types
or a CSQD ranking of purposes. Misrepresentation can lead to ordinary rejection;
there is no separate misclassification penalty or reputation mechanism.

The recipient may accept a request with standing directed permission or with an
express lane. Both modes record a full `C + S` refund obligation and commit the
granted permission with settlement. An express lane concerns the relationship;
it may be scoped, for example by purpose, time, or volume, but is usually not
scoped to a particular conversation. A request class does not itself grant
permission. Lane expiry or revocation does not reopen the settled request.

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
[implementation and cutover](docs/request-classes-implementation.md).

## Guest senders

A guest is a person without a cs-mail account who approaches a member by paying a
request bond. Individuals use this path; organizations use the legacy-domain path.
The full design is in [guest senders](docs/guest-sender-design.md).

- A guest is identified by one verified mailbox and mapped at the edge to a
  synthetic protocol identity, following the legacy-sender precedent. The request
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
- After acceptance, a guest communicates free and indefinitely, subject to the
  recipient's block and revocation, by ordinary email in both directions. The
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

## Enforcement boundaries

Rust types and validated transitions distinguish immutable service terms,
verified evidence, member preferences, operator policy, and payment states.
Durable transactions must still check current funding restrictions at dispatch,
enforce person/account uniqueness, and prevent concurrent duplicate payments.
A previously constructed Rust value cannot establish that external state has
remained unchanged.

See the [implementation record](audit/domain-model-implementation-2026-09-15.md)
for implemented APIs, acceptance evidence, and remaining deployment work.
