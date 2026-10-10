# Guest senders and external conversations: design (Milestones 3a–3c)

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Status: agreed design, implementation record updated 2026-10-08; implementation and validation limits are listed below.
Both entry paths now join through the [local external-email journey](external-email-journey.md): general member-address reception, paid acceptance reply routes, explicit outbound consent and SMTP2GO transactional notices. Public deployment and provider callback validation remain Milestone 3c work. The older held-mail confirmation design below is not implemented in this bounded pass. This note belongs with the
[C-SQD domain model](../DOMAIN_MODEL.md) and the
[product build plan](../cs_mail_build_plan.tex) (Milestones 3a–3c).

## Purpose

A **guest** is a person using an external mailbox without a cs-mail account.
There are two entry paths: a guest approaches a member through a paid request,
or a member emails an external address first and reviews the initial reply
without charging for it. After acceptance, communication should feel like ordinary
email to the guest: they write from their own mailbox, the member's replies arrive in that
mailbox from the member's cs-mail address, and no account or app is required.

Guests extend the product without changing the request kernel. They follow the
precedent set for organizational legacy senders: evidence is verified at the edge,
turned into a synthetic `ProtocolIdentity`, and the kernel's existing rules for
relationships, request history, acceptance, lanes, blocking, and settlement apply
unchanged.

## Sender kinds

The deployment now recognizes three kinds of sender subject. They are distinct
types at the edge (principle 1), even though each resolves to a `ProtocolIdentity`
for the kernel.

| Kind | Identified by | Account | End-to-end content |
|---|---|---|---|
| Member (native) | Enrolled account and protocol identity | Yes | Yes |
| Organizational legacy sender | DMARC-authenticated domain | No | No |
| Guest | Exact mailbox; control evidence is separate from a member-selected destination | No | No |

The legacy-domain mapping is wrong for individuals: mapping `gmail.com` to one
identity would merge every Gmail user. Individuals use the guest path;
organizations use the legacy-domain path.

**Identity mapping.** A guest mailbox resolves to a synthetic protocol identity
through a versioned mapping with its own separation string (for example
`cs-mail/guest-mailbox-identity/v1`), mirroring
`DomainIdentity::synthetic_protocol_identity`. Storage keeps a registry from
protocol identity to sender subject with a uniqueness constraint, so no two
distinct subjects can resolve to the same identity.

A new lane subject, `LaneSubject::GuestMailbox`, lets a recipient grant a guest an
express lane, beside the existing `Native` and `LegacyDomain` subjects.

## Evidence and what it establishes

Principle 5 applies: each item below is evidence from an issuer, and policy
decides what it establishes.

- **Mailbox verification.** A code or link sent to the address and returned
  establishes control of that mailbox at that time. It does not establish who the
  person is, or that different mailboxes belong to different people.
- **Payment evidence.** Funding finality for the request charge, plus the payment
  processor's fingerprint of the card or account used. The fingerprint is an
  anti-abuse signal, not an identity claim.
- **Inbound email authentication** (initial replies and ongoing mail; see below).
  DMARC results for the sender's domain, the exact From address, and the conversation reply
  token. These are automatic checks on received messages, not a requirement for
  the guest to complete a new interactive verification for every reply.
- **Member-selected destination.** An address entered by a member names where
  to send; it is not evidence of mailbox control or continuing contact permission.
  The mailbox uses the same identity if it later verifies or makes a paid request.

Guests never enroll in identity-model and receive no `SubjectId`. The one account
per person objective is unaffected because a guest is not an account.

### Verification and session persistence

The guest domain owns the verification lifecycle. A successful code check produces
one complete consequence: the consumed verification and its session. PostgreSQL
locks the challenge, samples trusted time after that wait, and commits consumption
and the unique session reference together. Wrong attempts are committed too. A
verification can create at most one session; its token digest is bound into the
consumed evidence. The code digest is discarded on consumption.

Session restoration loads that consumed verification through the stored token
reference and checks its token binding and lifetime. There is no standalone session
JSON, public session constructor, or session deserializer. Verification records use
checked deserialization for their attempt counts and time relationships. These checks
do not authenticate an arbitrary record supplied by a caller: the repository remains
the trusted source of durable evidence. Session lookup samples time after its database
read. Existing inclusive expiry boundaries remain unchanged.

Migration 0028 requires a fresh development database and rejects populated older
databases. Old sessions lack the required provenance; no fallback reader or guessed
backfill exists. The [audit hardening record](audit-hardening.md) records the experiments
and remaining boundaries.

## Request history and anti-abuse

The request charge is the primary defense: every request costs `C` even when it
is rejected. Request history (`level`, `earliest_next_submission`, pending
reservation) exists to stop repeated approaches to one recipient, and is what
fresh mailboxes would otherwise reset.

Guest request history is grouped by **both** mailbox and payment fingerprint, as
a policy over evidence:

- A guest request resolves every existing guest history linked to its mailbox or
  its fingerprint for that recipient.
- If the evidence links two previously separate histories, they merge
  prospectively into one, and the merged history takes the most restrictive
  state: highest level, latest next-submission time, and any pending reservation.
  The merge and the evidence that caused it are recorded (principle 7). Merges
  are never undone automatically.
- Someone using several payment instruments and several mailboxes can still
  obtain separate histories. That limitation is accepted; each approach still
  costs `C`.

**Requirement on the live payment processor:** it must report a stable
instrument fingerprint. The simulator supplies synthetic fingerprints.

## Acceptance and ongoing communication

For a guest-initiated paid request, the recipient decides as for any request:
accept with standing permission or an express lane, reject, block, or let it
expire. Settlement is unchanged.

Standing acceptance permits free ongoing communication; an express lane permits
free communication within its scope and validity. The recipient's block and
revocation remain the controls.
Admission of guest messages follows the same host-level service-coverage condition
that applies to the member's other incoming communication.

Communication after acceptance is ordinary email in both directions:

- **Member to guest.** The member's reply is sent as standard email from the
  member's protocol identity address at the deployment domain (for example
  `bob@<deployment domain>`), DKIM-signed for that domain, correctly threaded, and
  with a per-conversation reply address.
- **Guest to member.** The guest replies from their own mail client. The
  deployment's own mail receiver accepts the message, authenticates it (below),
  encrypts it to the member's content key on arrival, and admits it under the
  relationship's permission.

### Member-initiated external conversations

The 2026-10-06 decisions in [the domain model](../DOMAIN_MODEL.md#member-initiated-external-conversations)
govern this path. Alice selects Carol's exact mailbox and explicitly consents to
external plaintext disclosure. No inbox-control challenge is required: Alice is
responsible for her destination choice. Automatic authentication of each incoming
email remains separate from that choice and from contact permission.

Carol can send unlimited replies inside the pending conversation. These are
uncharged, create no request or financial obligation, and do not authorize another
conversation. Alice may Accept, grant an express lane, Reject, or Block. Acceptance
and lanes belong to the existing relationship owner. Every subsequent relationship
decision invalidates the original pending opportunity, so revocation or lane expiry
cannot reopen it. Rejection closes only the invitation: the ordinary paid-request
route remains available under its existing restrictions unless Alice blocks Carol.
An explicit new invitation from Alice can reopen unpaid contact. A new conversation
under existing standing or lane permission uses that permission from the outset;
it does not silently replace lane limits with unlimited pending access.

Outbound content, its delivery instruction, the conversation and consent record,
and the command receipt commit atomically. Pending delivery is not plaintext storage.
Withdrawal cancels waiting deliveries in the same transaction as the consent change.
Dispatch checks current account authority, coverage, block and consent under the
same protection, and commits `DispatchBegun` before releasing content to transport.
Once that boundary is crossed, withdrawal may be too late. Unknown handoffs remain
unknown for reconciliation; they are not automatically sent again.

The first Rust slice exposes signed member commands and encrypted authored-text
copies, with a development relay handoff. The receive-only SMTP adapter now handles
known-conversation plain-text replies and paired plain-text/HTML alternatives; see the [receiver runbook](smtp-receiver.md).
General member-address mail, public MX deployment and live provider validation remain work. The trusted receiver API accepts no public client assertions
of successful authentication; local experiments supply synthetic evidence through
development support. Native quotation and forwarding are not exposed by this API.

### Authenticating a guest's inbound email

For ongoing communication under standing permission or a lane, an inbound message
is admitted only when:

1. **DMARC passes** for the author domain, verified by `cs-mail-smtp-gateway` at
   the receiver with the full connection context (remote IP, EHLO, envelope
   sender); and
2. **the From address exactly matches** the guest's verified mailbox; and
3. the message arrives at the conversation's **reply address**, or it is new mail
   to the member's address from a mailbox that already holds permission.

The reply token alone is never sufficient, because anyone the guest forwards a
message to also holds it. For author domains whose DMARC policy is missing or
permissive, a pass is weaker evidence. Such messages are held, and the guest
receives a link to confirm them on the request page. They are not admitted
silently.

Replies to a member-initiated pending conversation require automatic authentication,
the exact selected author mailbox and that conversation route, without a separate
mailbox-control challenge. The interactive verification rules above apply to the
paid guest entry path, not to the member-selected invitation.

Other mail from a mailbox without permission is not delivered. An unknown sender
receives an automatic reply with the member's request link, but only when DMARC
passes for the sender's domain. Otherwise the message is rejected during the SMTP
transaction, so the deployment never sends backscatter to forged addresses.
A blocked or revoked guest's mail is **rejected at the SMTP level**: after the
message data is received and the author is identified, the receiver answers with
a permanent `550` and a generic message. The guest's mail client shows the bounce;
the mail is neither delivered nor silently discarded.

## Privacy and consent

- **The guest's email address is shown to the recipient.** It is needed for
  seamless correspondence. The request page tells the guest before payment that
  the recipient will see it.
- **Guest content is not end-to-end encrypted.** The provider sees plaintext on
  arrival, from the page or by email. It is encrypted to the recipient's key on
  arrival and never stored or logged as plaintext. The page says so before the
  guest writes, and the member's application labels guest conversations.
- **The member's first message and later emails leave as plaintext email.** Each
  external conversation requires the member's explicit downgrade consent, using the
  existing `DowngradeConsent` / `DeliveryRoute::SmtpDowngrade` model. Without
  consent no member email is sent (`SmtpError::MissingDowngradeConsent`).
- **Retention.** Mailbox and payment-fingerprint evidence, and the history links
  between them, are retained **indefinitely** by default, because request history
  and anti-abuse grouping depend on them. The request page tells the guest this
  before payment. A shorter period, or erasure on request, is a later retention
  and erasure policy decision (see the domain model's deferred items).

## Money

- A guest authorizes one conditional charge `C + S` per request, exactly as a
  member does. Cancellation, rejection, expiry, and acceptance settle identically.
- Refunds use the existing capture-linked refund obligations and return to the
  payment method that funded the request. No account is needed.
- Forfeited collateral from rejected guest requests enters the member pool.
  Guests are not members and receive no distributions.

## Mail infrastructure

- **Inbound: our own receiver, built in Rust.** A separate internet-facing
  process (`apps/cs-mail-mx`) is the MX for the deployment domain. It shares the
  application layer with `cs-maild` and passes full connection context to
  `cs-mail-smtp-gateway`. It is receive-only: no mailboxes, IMAP, or outbound
  queue. It accepts mail only for member protocol identity addresses and
  conversation reply addresses. Candidate building blocks are Stalwart's
  `smtp-proto` and `mail-parser`, beside the `mail-auth` library already in use.
  - **SMTP replies follow durable admission.** `250` is sent only after the
    message is committed as admitted. Refusals are permanent `5xx`. Temporary
    failures, such as database or DNS unavailability, are `4xx`, so the sending
    server retries (principles 8 and 9).
  - STARTTLS is offered, and MTA-STS and TLS reporting are published for the
    deployment domain.
  - Connection, message-size, and time limits bound resource use. The process
    runs with minimal privileges.
- **Outbound.** Member-initiated messages, subsequent correspondence, and
  transactional notices (verification codes, outcomes, refunds) are DKIM-signed
  with the deployment domain's own key, with
  SPF and a DMARC policy published, so reputation accrues to the deployment
  domain. Outbound mail is **sent through an email delivery service at first**,
  for deliverability from a new sending IP; moving to our own sending
  infrastructure later keeps the domain's reputation because signing uses our
  key. The delivery service sees the plaintext of member replies, so the
  per-conversation downgrade consent says so.
- Bounces and complaints are processed and can suspend outbound delivery to a
  guest mailbox.

## Out of scope

- **Guest-to-member conversion.** Moving an existing relationship from a guest
  identity to a new member's identity is a separate cutover with provenance
  questions. A guest who joins starts fresh.
- **General mailbox hosting**, such as IMAP access or hosting external accounts.
  Member-initiated external conversations and transactional notices are in scope.
- **Independent-provider federation.**

## Conjectures to challenge

| Claim | Would be falsified by | Why it matters |
|---|---|---|
| No two distinct sender subjects resolve to the same protocol identity. | Two subjects of any kind registered under one identity. | Permission or blocks would apply to the wrong sender. |
| Changing mailbox while reusing a payment instrument cannot reset request history or bypass a pending request. | A second mailbox with the same fingerprint submits while the first request is pending, or at a lower level. | The request page would become a harassment channel. |
| A message with a forged From address for a guest's mailbox is not admitted. | A message failing DMARC alignment for the guest's domain reaches the member. | Anyone could speak as an accepted guest. |
| A forwarded reply token alone cannot post into the conversation. | A message from a different mailbox, carrying a valid token, is admitted. | Forwarding a thread would grant permission. |
| No member reply leaves as email without that conversation's downgrade consent. | An outbound message is sent for a conversation lacking consent. | The member's plaintext would leave without agreement. |
| A blocked or revoked guest's mail is rejected with a permanent SMTP error and never delivered. | Mail from the guest's mailbox is accepted with `250` after the block or revocation. | Blocking is the recipient's primary control once guests message freely. |
| The receiver never answers `250` before the message is durably admitted. | A crash after `250` loses the message, or an accepted message is later refused. | Senders would believe mail was delivered when it was not. |
| An automatic reply is never sent to a sender whose domain fails DMARC. | An auto-reply is sent to a forged From address. | Backscatter would damage the deployment domain's reputation. |
| A guest refund returns only to the payment method that funded it. | A refund is directed anywhere else. | Refund redirection is a fraud path. |
| Guest plaintext never reaches storage, logs, or diagnostics. | A secret-scan finds guest message text. | The privacy label would understate the exposure. |

## Decisions

Decided 2026-09-25:

1. Inbound: our own receive-only Rust receiver (`apps/cs-mail-mx`).
2. A blocked or revoked guest's mail is rejected at the SMTP level.
3. Outbound: an email delivery service first, DKIM-signed with the deployment
   domain's key.
4. Mailbox and payment-fingerprint evidence is retained indefinitely by default.

Decided 2026-09-29:

5. Members may initiate external conversations. Their initial send opens a reply
   opportunity, not standing permission. The authenticated initial reply is
   readable while the invitation is pending. The October decision below permits
   continued replies until the member decides; broader permission still requires
   Accept or an express lane.
6. This initial reply is uncharged and does not require a paid-request ceremony.
7. Email authentication is automatic per message and separate from interactive
   mailbox verification and the member's permission decision.

Decided 2026-10-06:

8. No separate mailbox-control challenge for member-initiated invitations; retain
   automatic incoming authentication and exact mailbox/conversation matching.
9. Unlimited uncharged replies in the pending conversation; broader permission
   remains a relationship or lane decision.
10. Rejection closes the unpaid invitation without blocking or changing paid history.
11. Disclosure withdrawal cancels waiting outbound messages; dispatch already begun
    may be too late to cancel. Unknown handoffs require reconciliation.

### Validation scope for this Rust slice

- Pending access cannot escape its conversation or survive rejection/block. A
  different mailbox or conversation being admitted, or a reply admitted after
  rejection/block, would falsify the claim and permit unwanted contact.
- Existing acceptance/lane rules govern continuing permission. Admission after
  revocation/expiry, or exceeding a lane allowance, would falsify the claim.
- Withdrawal and dispatch have one serialized boundary. Dispatch from a delivery
  cancelled by a prior withdrawal would falsify the disclosure promise.
- Start and retry effects are atomic and idempotent. An injected receipt failure
  leaving a conversation or delivery, or repeated delivery consuming another lane
  allowance, would falsify the storage contract.

These are PostgreSQL experiments in the member gateway suite. They do not establish
real SMTP authentication, deliverability, SMTP2GO behavior, or downstream recall.
