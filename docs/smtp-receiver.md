# Incoming conversation mail — supported MIME profile

The Rust `cs-mail-mx` executable receives plain-text and paired plain-text/HTML mail to member addresses and replies to existing external
conversations. It offers STARTTLS, performs SPF/DKIM/DMARC through the existing
`mail-auth` verifier using the actual socket peer and SMTP envelope, matches the
single From mailbox to the conversation, encrypts text to the member, and commits
admission before the final DATA `250`. It has no outbound relay or SMTP AUTH.

This is a local, validated receiver implementation. Public MX/DNS deployment and
Internet deliverability have not been established. The outbound adapter is now implemented locally (see [SMTP2GO](smtp2go-outbound.md));
General member-address mail, paid acceptance reply routes and SMTP2GO transactional
notices are implemented in the [local external-email journey](external-email-journey.md).
HTML-only messages, attachments and request-page confirmation for weakly authenticated
mail remain later work. Unsupported content/authentication is explicitly refused.

## Addresses and supported email

The [2026-10-14 alias design](../DOMAIN_MODEL.md#aliases-and-express-lane-scope)
adds private aliases with automatic alias-scoped lanes and no bonded route;
public-facing aliases retain the existing permission/request process. This is
agreed product scope, not implemented receiver behavior. Its detailed admission
and lifecycle policies remain open. The address forms, sender checks and
request-page prompts documented here describe the current sender-scoped paths.

A conversation's address is `reply+<32 lowercase hexadecimal conversation ID>@<mail_domain>`.
For example, conversation 7001 uses
`reply+00000000000000000000000000001b59@mail.example.net`.
The development relay artifacts now include this address as `reply_to`.
The address identifies a route; it conveys no permission. Automatic authentication,
exact From-mailbox matching and current conversation/relationship authority still
apply. Member-selected invitations require no interactive mailbox-control challenge.

The receiver accepts one recipient per transaction and either one `text/plain` part
(including the MIME default when Content-Type is absent) or a flat
`multipart/alternative` containing exactly one plain-text and one HTML part, in
either order. It selects the actual plain-text part; it never converts HTML into
text. Subject and decoded text are retained together as encrypted external text,
at most 16 KiB, with no native quotation/provenance assertion. A single valid From
and Message-ID are required. Authentication receives the complete original email,
including the HTML representation, before any admission occurs.

This uses MIME's alternative-representation convention; it does not establish that
the HTML and plain text say the same thing. HTML is neither rendered nor stored.
Conventional MIME preamble/epilogue are framing, not delivered content. HTML-only
mail, attachments (including named inline files), extra or nested parts, ambiguous
part headers, unsupported encodings/charsets and truncated boundaries receive an
explicit unsupported-content response. Recognized charset decoding and standard
identity/base64/quoted-printable transfer encodings are supported. Decoded U+FFFD
is conservatively refused because it may indicate lossy decoding, even when that
character was intentional. Boundary validation prevents the parser's permissive
recovery from accepting a missing closing delimiter. The profile is deliberately
narrower than arbitrary Internet MIME; it does not silently accept an attachment
and discard it.

Email carries `GuestSmtp` provenance with unspecified origin and purpose. It never
claims that a human initiated it. Guest express lanes created through the CLI now
accept unspecified email declarations; existing narrower lanes retain their own
constraints and may refuse such email. The request page's `GuestGateway` provenance
remains separate. Native upload paths cannot claim SMTP authority.

## Local configuration and startup

Migrate/bootstrap through `cs-maild` first. The receiver opens schema 30 without
running migrations and refuses a different schema version. Use a fresh development
database for this cutover; the previous synthetic receiver's retry digest semantics
were replaced, without a fallback reader.

```json
{
  "database_url": "postgresql://receiver@127.0.0.1/cs_mail",
  "listen": "127.0.0.1:2525",
  "mail_domain": "mail.example.net",
  "tls_certificate_file": "/secure/mx-chain.pem",
  "tls_private_key_file": "/secure/mx-key.pem",
  "replay_secret_file": "/secure/mx-replay.json",
  "maximum_connections": 32
}
```

The certificate file contains the PEM leaf certificate followed by intermediates;
the key is PEM. The replay secret is a dedicated, nonzero 32-byte JSON array. Keep
these files protected. No member signing, relationship derivation, or relay-decryption
secrets are supplied to this process. The checked-in certificates/private key under
`cs-mail-test-support/fixtures` are public test material, never deployment credentials.

```sh
cargo build --locked -p cs-mail-mx
target/debug/cs-mail-mx --config /secure/mx.json
```

Keep the replay secret stable across restarts and back it up with the database.
Uncoordinated replacement changes operation identities; secret rotation requires a
separate migration design. Current deployment expects a local/trusted PostgreSQL
connection; a remote TLS database connector and a hardened deployment-specific
SQL-role/privilege configuration are not supplied by this pass.

The process bounds connections (configured 1–256), commands (100 per cleartext/TLS phase),
command lines (512 bytes), DATA lines (1,000 bytes after dot unstuffing), raw email
(128 KiB), header size/count, DKIM signatures, authentication duration (30 seconds),
DATA duration (120 seconds), and session duration (300 seconds). At most eight
blocking admission operations run; their permits remain held if a disconnected
session leaves a database operation completing. STARTTLS discards the earlier
SMTP state and refuses buffered cleartext commands at the upgrade boundary.

The final DATA `250` distinguishes admission to the member from a queued request-page
prompt for an unknown authenticated sender. The latter explicitly says the original
email was not delivered. MAIL/RCPT `250` merely records
SMTP transaction inputs. Permanent authentication/routing/permission failures use
`550`, unsupported content uses `554`, oversized content uses `552`, and transient
DNS/database/transfer failures use `4xx`. A request-page prompt requires a DMARC pass,
a nonempty envelope sender and no automatic-message marker. It is bounded to one
per mailbox/member per 24 hours; forged or automatic mail cannot trigger it.
Received Authentication-Results headers never supply authentication evidence.

## Atomicity, provenance and retries

The stable message identity is a keyed fingerprint of deployment, addressed route,
exact mailbox and RFC Message-ID. A separate keyed fingerprint binds that identity
to the decoded subject and selected plain-text body. MIME ordering/transfer encoding
and HTML-only changes do not alter this delivered-text fingerprint, but every
attempt must still pass the full MIME profile and authentication. This avoids treating randomized encryption, fresh
verification time, or changing Received headers as a new message. Changed content
under the same identity is refused. The receiver reauthenticates every attempt;
an exact already-admitted retry returns its historical receipt without another
message or lane consumption, even if permission has since changed.

The gateway binds verified text, exact mailbox, conversation, replay identity and
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
