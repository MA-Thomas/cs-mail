# Incoming conversation mail — supported MIME profile

The Rust `cs-mail-mx` executable receives supported plain-text and HTML MIME mail to member addresses and replies to existing external
conversations. It offers STARTTLS, performs SPF/DKIM/DMARC through the existing
`mail-auth` verifier using the actual socket peer and SMTP envelope, matches the
single From mailbox to the conversation, encrypts preserved MIME content to the member, and commits
admission before the final DATA `250`. It has no outbound relay or SMTP AUTH.

This is a local, validated receiver implementation. Public MX/DNS deployment and
Internet deliverability have not been established. The outbound adapter is now implemented locally (see [SMTP2GO](smtp2go-outbound.md));
General member-address mail, paid acceptance reply routes and SMTP2GO transactional
notices are implemented in the [local external-email journey](external-email-journey.md).
Attachments and request-page confirmation for weakly authenticated mail remain later work. Unsupported content/authentication is explicitly refused.

## Addresses and supported email

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

The receiver resolves the accepted SMTP envelope destination and commits receiving
alias and permission evidence with delivery. Private-alias admission checks its explicit receiving permission. Domain-scoped
grants consume authenticated domain evidence; mailbox identity remains distinct
for correspondence and blocks. Generated identity/conversation routes
below remain distinct, without inferred alias membership. Alias-aware public
approach links and held-message release remain unfinished.

**Further alias clarification (2026-10-14).** One
participant protocol identity may control multiple alias routes without public
linkage. Sender-identity blocks apply across all receiving aliases. Aliases
have no automatic expiry and offer Restrict, Replace, Retire. Suspicious or
insufficiently authenticated alias mail is held for review without ongoing
permission or payment. SMTP envelope recipients and authenticated native targets
establish receiving-route evidence. The [domain model](../DOMAIN_MODEL.md#aliases-and-express-lane-scope)
owns these rules. Detailed held-message release, outbound/reply addressing and
alias-aware public publications remain separate work. Failed or insufficient
transport authentication is still explicitly refused by this receiver; this pass
does not implement a held-mail inbox.

A conversation's address is `reply+<32 lowercase hexadecimal conversation ID>@<mail_domain>`.
For example, conversation 7001 uses
`reply+00000000000000000000000000001b59@mail.example.net`.
The development relay artifacts now include this address as `reply_to`.
The address identifies a route; it conveys no permission. Automatic authentication,
exact From-mailbox matching and current conversation/relationship authority still
apply. Member-selected invitations require no interactive mailbox-control challenge.

The receiver accepts one recipient per transaction and a bounded complete MIME tree:
plain text (including MIME's default), HTML, alternatives containing one of each,
and related inline PNG/JPEG/WebP resources with unique Content-IDs. The original
accepted MIME is retained encrypted, up to 2 MiB and 64 parts. Named inline raster
resources are supported; attachments and other multipart/content kinds are refused.
A single valid From and Message-ID are required. Authentication receives the complete
original email before any admission occurs.

Rust derives a restricted HTML presentation and local searchable text without I/O.
The supplied plain-text alternative remains distinct; it is not evidence that both
representations say the same thing. Ambiguous headers, unsupported encodings/charsets,
lossy decoding and incomplete multipart framing are refused. An unsupported sibling
cannot be silently discarded to admit the rest.

The Apple reader preserves supported HTML/CSS appearance in an isolated nonpersistent
WebKit view. Scripts, frames, forms, automatic navigation and external requests are
blocked. Supplied raster bytes use an app-owned local resource scheme, with decoder,
format, frame-count, dimension and pixel limits. External `img` pictures are listed
for explicit per-message approval, individually or as a selected collection.
Retrieval uses anonymous HTTPS on port 443, pinned public DNS results, no proxy,
redirects or automatic retries, and a 4 MiB response limit. Attempts precede requests
in encrypted device storage. Successful bytes retain their provenance separately
from the original message and are reused locally until message expiry. External
stylesheets/fonts/CSS background URLs remain blocked and have no loading action.
No sender-wide permission or automatic background retrieval is introduced.

DNS/database/transfer failures use `4xx`. A request-page prompt requires a DMARC pass,
a nonempty envelope sender and no automatic-message marker. It is bounded to one
per mailbox/member per 24 hours; forged or automatic mail cannot trigger it.
Received Authentication-Results headers never supply authentication evidence.

## Atomicity, provenance and retries

The stable message identity is a keyed fingerprint of deployment, addressed route,
exact mailbox and RFC Message-ID. A separate keyed fingerprint binds that identity
to the decoded subject, every supplied text/HTML representation and each CID resource.
Transport trace headers, alternative ordering and equivalent transfer encodings do not
change that identity; changes to HTML or resource bytes do. Every attempt must still
pass the full MIME profile and authentication. The first accepted original remains unchanged. This avoids treating randomized encryption, fresh
verification time, or changing Received headers as a new message. Changed content
under the same identity is refused. The receiver reauthenticates every attempt;
an exact already-admitted retry returns its historical receipt without another
message or lane consumption, even if permission has since changed.

The gateway binds verified content, exact mailbox, conversation, replay identity and
ciphertext in a private `AuthenticatedReply`. Storage accepts only this value; a
caller cannot supply independently constructed authentication evidence and content.
The application receives plain facts for deterministic rule evaluation. Verification
remains an adapter responsibility, and a record of prior verification grants no
current permission by itself.

Admission uses the existing receipt-order protection and rechecks account status,
coverage, mailbox, invitation, relationship, lane scope/rate and current content
key, sampling current time after the required locks. A content-key rotation between preparation and commit produces a retryable
failure. Ciphertext, message, authentication provenance and lane consumption commit
together. SQL stores the verified domain/mechanisms/time and hashes of transport
context and raw email, not raw email or plaintext. Failure or a crash before commit
cannot acknowledge delivery; loss of the success reply after commit can safely be
retried. The receiver never creates standing permission itself.

## Validation and remaining deployment work

The user agreed these conjectures before tests were added:

| Claim | Falsifying observation | Consequence |
|---|---|---|
| Forged authors or forwarded routes do not grant access | Tampered DKIM, claimed authentication headers, duplicate From, a wrong mailbox or unknown conversation is admitted | Anyone could speak as an invited guest |
| Current permission controls new mail | An unseen message receives success after revoke or block | Recipient controls would be ineffective |
| SMTP success follows atomic commit | Success while a database write is blocked, or any committed partial delivery after an injected failure | Senders would believe undelivered mail was delivered |
| Lost acknowledgments cannot duplicate delivery | A fresh transport attempt, encryption and receipt time produce a second message under the same email identity | Ordinary SMTP retry would duplicate delivery or lane consumption |

Four PostgreSQL scenarios in `cs-mail-service/tests/core_loop/external_flow/smtp.rs`
exercise the complete TCP/STARTTLS/receiver path with real RSA and Ed25519 DKIM verification
and controlled local DNS. They also check transient DNS failure, encryption at rest,
altered retries, and no pre-TLS greeting authority after upgrade. The MIME pass
extends permission and lost-acknowledgment scenarios to multipart input. Its agreed
claims are that MIME complexity cannot bypass sender/current permission checks,
silently discard unsupported content, or duplicate delivery on retry. A signed
malformed-message matrix challenges content loss; an outgoing SMTP2GO HTTP fixture
followed by a signed SMTP reply challenges whether the adapters form one readable
conversation. It checks decryption, Unicode, unused-HTML tampering, alternative
order and transfer re-encoding. These are local fixtures, not Internet mail delivery.
The prior four
external-flow scenarios remain. The normal quality gate runs them with `--ignored`.
The actual executable was separately started and exercised with a verified test CA
chain, relay refusal, forged-authentication rejection and clean shutdown.

Not covered: a public-domain delivery run, public DNS failures/propagation, resource
exhaustion/load/fuzz campaigns, a kill-at-every-instruction crash matrix, automated
certificate renewal, sender reputation/abuse controls, or remote database TLS.
A database failure and lost acknowledgment challenge the commit boundary, but are
not a universal crash proof. Public operation also requires MX and A/AAAA records,
valid TLS certificates, an HTTPS MTA-STS policy, its `_mta-sts` TXT record, and TLS
reporting. Those DNS/HTTPS changes are deployment work and were not performed here.

For example, if the receiving domain and its MX are both `mail.example.net`, serve
this only after validating that deployment, at
`https://mta-sts.mail.example.net/.well-known/mta-sts.txt`:

```text
version: STSv1
mode: testing
mx: mail.example.net
max_age: 86400
```

Publish the matching policy ID and reporting mailbox through DNS; evaluate reports
before moving to enforce mode. SMTP state resets follow
[RFC 3207](https://www.rfc-editor.org/rfc/rfc3207.html); DATA acknowledgement and
retry behavior follow [RFC 5321](https://www.rfc-editor.org/rfc/rfc5321.html).
Alternative representations and multipart framing follow
[RFC 2046](https://www.rfc-editor.org/rfc/rfc2046.html).
