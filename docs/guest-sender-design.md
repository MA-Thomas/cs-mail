# Guest senders: design (Milestone 3a)

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Status: agreed design, 2026-09-25. All decisions are recorded below. This note belongs with the
[C-SQD domain model](../DOMAIN_MODEL.md) and the
[product build plan](../cs_mail_build_plan.tex) (Milestones 3a–3c).

## Purpose

A **guest** is a person without a cs-mail account who approaches a member by
paying a request bond, through the member's public request page or by replying to
email. After acceptance, communication should feel like ordinary email to the
guest: they write from their own mailbox, the member's replies arrive in that
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
| Guest | Verified mailbox (one exact address) | No | No |

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
- **Inbound email authentication** (after acceptance; see below). DMARC results
  for the sender's domain, the exact From address, and the conversation reply
  token.

Guests never enroll in identity-model and receive no `SubjectId`. The one account
per person objective is unaffected because a guest is not an account.

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

The recipient decides as for any request: accept with standing permission or an
express lane, reject, block, or let it expire. Settlement is unchanged.

After acceptance, **the guest may communicate for free, indefinitely**, like any
accepted sender. The recipient's block and revocation remain the controls.
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

### Authenticating a guest's inbound email

An inbound message is admitted to an accepted guest relationship only when:

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

Mail from a mailbox without permission is not delivered. An unknown sender
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
- **The member's replies to a guest leave as plaintext email.** Each guest
  conversation requires the member's explicit downgrade consent, using the
  existing `DowngradeConsent` / `DeliveryRoute::SmtpDowngrade` model. Without
  consent no reply is sent (`SmtpError::MissingDowngradeConsent`).
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
- **Outbound.** Member replies and transactional notices (verification codes,
  outcomes, refunds) are DKIM-signed with the deployment domain's own key, with
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
- **General email hosting.** Members do not send email to arbitrary external
  addresses. Outbound email exists only for guest conversations and notices.
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

No guest-sender decisions remain open.
