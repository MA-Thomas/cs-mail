# External-email journey: local implementation, 8 October 2026

The two entry paths now meet at the same external-conversation model. A member
can invite an external mailbox without charging it. A guest can verify their
mailbox, fund and submit a request, and receive a reply address when the member
accepts. Funding remains simulated; transactional email can explicitly select
SMTP2GO. No account, live email, payment, server or public DNS was changed in this pass.

## Recipient control and disclosure

An alias has an explicitly selected receiving policy, independent of address
choice and directory visibility. A bonded alias has its own profile and request
classes; senders without applicable permission must explicitly authorize a bonded
request. Selected sender-scoped lanes or standing permission allow direct contact
under their named receiving scopes. Alternatively, the member can explicitly grant
an alias-scoped lane so new senders who obtain that address can correspond without
a bonded request, subject to authentication, blocks and admission controls. That
policy has no bonded fallback. Public/private is no longer a product alias kind.
The [domain model](../DOMAIN_MODEL.md#aliases-and-express-lane-scope) owns the rules.

The 21 October 2026 Rust and Swift implementation uses these separate choices,
per-alias publications and frozen request/reply routes. Earlier dated validation
below remains historical. Current validation and owner-install status are recorded
in the [Apple runbook](../apps/cs-mail-apple/README.md). Alias fees and receiving-policy
switching remain deferred; no alias fee is charged.

**Further alias clarification (2026-10-14; implementation pending).** One
participant protocol identity may control multiple alias routes without public
linkage. Sender-identity blocks apply across all receiving aliases. Aliases
have no automatic expiry and offer Restrict, Replace, Retire. Suspicious or
insufficiently authenticated alias mail is held for review without ongoing
permission or payment. SMTP envelope recipients and authenticated native targets
establish receiving-route evidence. The [domain model](../DOMAIN_MODEL.md#aliases-and-express-lane-scope)
owns these rules and remaining delivery, transition, reply and held-message contracts;
implementation and validation records below do not establish this cutover.

Mail to `member-<32 hexadecimal persona>@<mail_domain>` passes the existing automatic
SMTP authentication and MIME checks. Standing acceptance or a valid mailbox lane
allows a new conversation. The conversation, encrypted first message, provenance
and lane consumption commit atomically. Exact retries do not consume another allowance. A historical request-page prompt
stays a prompt after later acceptance; its retry cannot become a new inbox delivery.
The two outcomes share one typed arrival identity and exclude each other under the
same transaction protection.
Blocked/revoked senders cannot use a new address or subject to regain access.

An unknown authenticated sender gets a request-page prompt instead of inbox
admission. The original text is not stored and must be entered on the request page.
The SMTP acknowledgement explicitly distinguishes this from delivery to the member.
Automatic messages and empty-envelope bounces cannot trigger prompts. One prompt
per mailbox/member per 24 hours bounds repeat replies; this is an operational mail
limit, separate from paid-request pricing/history. The queue rechecks current
coverage and permission before sending. Stored metadata comprises notice intent,
mailbox/member, receipt fingerprints and delivery evidence; it is not a plaintext
mailbox spool. Retention cleanup and broad outbound abuse policy remain later work.

For a paid acceptance, one request-specific conversation and its acceptance-notice
attempt commit together. The notice's Reply-To opens that route. The original paid
request remains in the member's request view; subsequent email is in the linked
external conversation. Relationship views now expose `external_conversations`, so
members can discover the route through the existing CLI/API.

Inbound permission does not authorize outbound disclosure. New guest-originated
conversations start with `NotGranted` consent. The member runs:

```sh
cs-mail-cli --home /private/member grant-disclosure --conversation ID --operation NAME
cs-mail-cli --home /private/member email-reply --conversation ID --message-file reply.txt --operation NAME
```

The grant is signed for the fixed conversation and mailbox. Withdrawal cancels
waiting mail. Restoring consent permits new messages but never revives cancelled
ones. Reply MIME references the incoming conversation root when the guest wrote
first. Ordinary email is not end-to-end encrypted: the receiver sees incoming
plaintext, and the configured delivery service sees the member's outgoing plaintext.

## Transactional notices and uncertainty

Verification, request submission/outcome/refund notices, and request-page prompts
now use stable typed notice identities and the existing `DeliveryState` lifecycle.
Attempts commit before carrier I/O. Concurrent workers obtain one attempt; a crash,
timeout or failed local receipt remains `DispatchBegun`, never an automatic resend.
This includes the unavoidable crash window after committing the attempt but before
calling the provider. These states express uncertainty, not guaranteed delivery.

Verification commits its hashed challenge and notice attempt in the same transaction.
The plaintext code is transient and cannot be reconstructed by a worker. After an
interrupted verification send, the guest requests a fresh challenge under the
existing rate limit. A worker derives owed outcome notices from durable request and
financial facts; an interrupted worker does not lose that intent. Once an attempt
begins, its unknown outcome must be reconciled rather than resubmitted.

The authenticated SMTP2GO callback recognizes notice Message-IDs separately from
authored-message IDs. Recipient, operation and provider evidence must agree.
Provider email IDs and event IDs have one shared namespace across both mail kinds.
Handoff reconciliation and normalized event insertion commit together. No raw
callback payload, verification code or notice body is stored in these tables.
`Accepted` is provider acceptance, while delivered/bounce/complaint reports remain
separate evidence. The tables and storage APIs expose this state; an operator
reconciliation UI is still absent. Failed attempts are also not automatically resent.

## Configuration and cutover

Schema **30** requires a fresh development database. The migration refuses existing
application data rather than guessing whether an old notice was sent before a crash.
It removes the old post-send notice log. No old-reader fallback, API alias or parallel
notice implementation is retained. Preserve older development databases and keys.

Guest settings now use this explicit shape in a build with `simulation` (the guest
payment adapter remains simulated):

```json
"guests": {
  "public_url": "https://requests.example.net",
  "content_secret_file": "/secure/guest-content.json",
  "mail_domain": "mail.example.net",
  "notices": {
    "kind": "smtp2go",
    "region": "global",
    "api_key_file": "/secure/smtp2go-api-key.txt",
    "dkim_selector": "cs2",
    "dkim_secret_file": "/secure/domain-dkim-rsa2048.pem"
  }
}
```

`members` and `external` must also be configured; `external.mail_domain` must match.
The external delivery and feedback choices remain explicit and independent. The
shared callback listener handles notices even when member-authored delivery is
`disabled`; enabling `guests.notices.smtp2go` still authorizes transactional sending.
For local development use `notices.kind: "development"` with `directory`,
`dkim_selector` and `dkim_secret_file`, omitting `region` and `api_key_file`.
The former guest-level DKIM/outbox fields are rejected, not silently interpreted.
Development files are exclusive mode-0600 writes and cannot replace a prior attempt.
`scripts/m3b-demo.sh` uses the new shape.

Before exposing member addresses publicly, run the guest page and notice worker,
configure its real HTTPS origin, and validate delivery, MX/TLS and callback routing.
An independently running receiver can durably queue prompts while the notice worker
is unavailable; it does not itself send them. The existing Linux package remains a
callback-only pilot and does not enable guest payments, a public page or MX.

## Next implementation step (agreed 8 October 2026)

Build and validate the macOS member application against the local Rust service,
simulated payments and controlled email transports. Provisioning and payment for
a rented Linux host, such as AWS Lightsail, are deferred until that application is
built and locally validated. Public MX, HTTPS and actual provider callback checks
follow when the application is ready for a hosted external-email pilot. The existing
deployment package remains available for that later step.

## Agreed experiments and limits

The user approved focused local PostgreSQL and simulated SMTP/provider experiments.
Four additional scenarios challenge these claims:

| Conjecture | Falsifying observation | Why it matters |
|---|---|---|
| Unknown mail cannot enter the inbox or generate uncontrolled replies | Forged/automatic mail creates a prompt, exact retries multiply it, a queued prompt survives a block, or acceptance changes an old retry into inbox admission | Authentication and recipient controls would be ineffective |
| New member-address admission is atomic and disclosure stays separate | An injected failure leaves a route or consumes a lane; retry consumes twice; member text leaves before consent; restoration revives cancelled mail | Content loss, excess access or unintended disclosure |
| Paid acceptance creates one usable reply route without granting disclosure | An acceptance email has no durable route, duplicate routes appear, or the route starts with outbound consent | The paid journey breaks or leaks member content |
| Notice interruption and provider evidence cannot duplicate or misattribute sending | Challenge survives without its intent, concurrent workers send twice, callback failure partly reconciles, or a notice's provider ID completes authored mail | False verification/delivery history and repeated external effects |

The wire scenarios use TCP/STARTTLS and actual DKIM verification against controlled
DNS. The provider scenarios use real loopback HTTP, provider disconnects, duplicate
callbacks and SQL-trigger failure injection. Existing suites continue to challenge
blocked/revoked replies, key rotation, lock waits, altered retries, expired sessions,
refund destination, MIME tampering, permission races and plaintext persistence.
Paid request/refund operations use the deterministic payment simulator.

The quality gate runs both ordinary and PostgreSQL suites; skipped suites do not
count as evidence. All eight final checks passed: 109 ordinary tests/doctests and 90 PostgreSQL
tests. The clean CLI/HTTP demonstration and schema-30 backup/recovery drill also
passed. Logs and precise run boundaries are recorded in the current checkpoint files.
No new live send, public callback/MX validation or real payment occurred. Also not
covered: arbitrary Internet MIME (HTML-only mail and attachments remain refused),
weak-authentication held-mail confirmation, human-readable aliases, native source
exports, full crash/load/fuzz campaigns, operator reconciliation UI, and public
abuse/reputation policy. The current authentication profile accepts an aligned
DMARC pass, including `p=none`; missing/failing authentication is refused rather
than held for a confirmation flow. This bounded local implementation does not
establish the build plan's public-deployment exit condition for Milestone 3c.
