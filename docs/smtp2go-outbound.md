# SMTP2GO outbound integration — local validation

The member-initiated external-conversation worker submits signed plain-text email
through SMTP2GO's HTTPS MIME API. On 7 October 2026, the first authorized live test
arrived at the designated Gmail inbox. Gmail passed SPF, SMTP2GO's RSA signatures
and DMARC, but reported cs-mail's Ed25519 signature as neutral (no key); independent
verification of the same signature passed. cs-mail also left that attempt uncertain
because no usable API receipt was recorded. The original HTTP response was not
captured, so its precise cause cannot be established retrospectively.

The RSA follow-up passed a fresh live test: cs-mail recorded SMTP2GO acceptance,
and Gmail explicitly passed the deployment's `cs2` RSA signature, SPF and DMARC.
These are observations about one delivery, not evidence about every provider.

The callback pass separates receiving provider evidence from submitting mail and
places callbacks on their own loopback HTTP listener. Local HTTP/PostgreSQL checks
exercise authentication, isolation, commit acknowledgements and retries. A public
HTTPS endpoint and actual SMTP2GO callbacks remain to be connected. See
[smtp-receiver.md](smtp-receiver.md) for incoming email; delivery callbacks do not
receive email replies.

## Hosting direction and timing (updated 8 October 2026)

The user chose a rented Linux server for the first hosted pilot, with Amazon
Lightsail proposed as the starting provider. The motivation is an always-on home
for the Rust application and private PostgreSQL database, with control over HTTPS
callbacks now and the custom incoming SMTP receiver later. The tradeoff is taking
responsibility for server updates, backups and recovery. Squarespace remains the
domain/DNS provider; SMTP2GO remains the outgoing email provider.

On 8 October the user deferred provisioning and paying for that host until after
the macOS member application is built and locally validated. The immediate next
step is the application against the local Rust service, simulated payments and
controlled email transports. Public HTTPS, MX and actual SMTP2GO callback validation
wait until the application is ready for a hosted external-email pilot. The Linux
deployment package remains preparation for that later stage; Lightsail is a candidate.

No server has been provisioned. The proposed starting plan and deployment sequence
are recorded in [the current handoff](smtp2go-checkpoint.txt). Resume there before
connecting public callbacks; hosting selection does not establish live callback
or inbound-email verification.

## What this pass delivers

A signed external command queues encrypted authored text. At dispatch, cs-mail
rechecks current account/service authority, block state, content lifetime and
conversation disclosure consent, then atomically records DispatchBegun. Only after
that commit does it decrypt the relay copy, construct email, and call SMTP2GO.
Withdrawing disclosure cancels Waiting work. DispatchBegun is already across the
cancellation boundary, including a crash before the network operation starts.

The HTTPS request uses `/v3/email/mime` with a base64 MIME document and explicit
`fastaccept: true`. cs-mail signs that MIME with the deployment's DKIM key. It sends
one message to one recipient, with a stable RFC Message-ID and the conversation's
Reply-To address. Later member messages reference the conversation root Message-ID, including the
original incoming RFC Message-ID when the guest started it. Exact per-message
reply ancestry is not modeled. Subject is currently the fixed
"cs-mail conversation"; authored text is preserved as UTF-8 base64 MIME so a long
paragraph cannot exceed SMTP line limits.

There is not yet a human-readable email-alias registry. For this slice a member's
public protocol identity maps to `member-<32 hexadecimal identity>@<mail_domain>`.
It is stable per persona, contains no account/bank identifier and is distinct from
the per-conversation `reply+<conversation>@<mail_domain>` address. General incoming mail to the member address now uses existing relationship/lane
authority or queues a bounded request-page prompt. Reply-To retains conversation
continuity. Human-readable aliases remain later work.

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

The outbound carrier and webhook adapter live in `cs-mail-smtp-gateway`; service
orchestration is in `cs-mail-service`, deterministic feedback decisions in
`cs-mail-application`, and atomic facts in PostgreSQL. `cs-maild` is a thin host.
External correspondence now has its own service/configuration and no longer needs
the guest request page's payment simulator. That page now selects transactional delivery explicitly through `guests.notices`;
verification and outcome attempts commit before SMTP2GO I/O. The callback listener
handles these notices as well. See [external-email-journey.md](external-email-journey.md).

## Handoff and delivery evidence

- Waiting: no handoff has started; disclosure withdrawal can cancel it.
- Cancelled: no new handoff will occur.
- DispatchBegun: the attempt began and its outcome remains uncertain until evidence
  arrives. Timeouts, disconnects, malformed replies, server errors and redirects
  remain here. The worker never selects this state for another send.
- Accepted: SMTP2GO positively accepted the single email and returned its email ID,
  or authenticated feedback reconciled the previously uncertain attempt. This is
  not a claim of recipient delivery or reading.
- Failed: a definite refusal or local pre-submission failure. No automatic resend.

HTTP 200 alone does not establish acceptance: a valid email ID and no provider
error establish queue acceptance. With `fastaccept: true`, the provider may omit
recipient success/failure accounting because processing happens later. If counts
and a failure list are present, they must be complete and consistent with one
success and no failures. Missing IDs, partial accounting, contradictory errors and
non-200 receipts remain uncertain. The former mandatory-count check incorrectly
classified compact fast-accept receipts as unknown. It refuses redirects, disables
HTTP retries, bounds response size, and uses connection/request timeouts. There is
no claim that a chosen Message-ID makes SMTP2GO submissions idempotent. We prevent
repeat submission through cs-mail's once-only committed dispatch lifecycle.

Processed, Delivered, hard/soft bounce, rejection and spam-complaint events are
retained as separate evidence. Delivered means the recipient's server accepted the
email, not that Carol read it. Out-of-order callbacks never erase an earlier fact.
The member's external-read response includes `provider_events` alongside handoff
state. Provider time is retained as reported text; local receipt time is stored
separately and provider time is never used as the permission clock.

The callback must authenticate with a dedicated bearer secret. Its normalized
message ID, sender persona, recipient and provider reference are checked against
stored outbound work. Waiting/cancelled/failed work cannot be declared delivered.
One provider email ID cannot bind to two cs-mail messages. Duplicate event IDs are
idempotent; altered replays conflict. The normalized event and any handoff
reconciliation commit together. Subject, provider error text, `auth` credentials
and raw webhook bodies are not stored or logged. Facts do not grant permissions,
create blocks, change financial state or automatically resend mail. Automated
outbound abuse/suppression policy and an operator reconciliation UI remain future work.

## Configuration

Schema 30 is required by the current application and SMTP receiver.
Use the project's fresh development database procedure; migrations are not declared
forward-only yet. A pre-existing configuration that relied on `guests` implicitly
starting external delivery must now supply `external` explicitly. There is no
fallback reader or implicit adapter selection.

The [Linux callback-pilot package](../deploy/linux/README.md) supplies a production
build recipe, systemd unit, callback-only configuration, HTTPS proxy template and
database backup/restore tools. It starts with sending disabled and a fresh database;
Linux host and public-provider acceptance checks remain separate from local evidence.

To enable the API and queue messages without any external sending:

```json
"external": {
  "content_secret_file": "/secure/external-content.json",
  "mail_domain": "mail.example.net",
  "delivery": { "kind": "disabled" },
  "feedback": { "kind": "disabled" }
}
```

The content secret is a stable 32-byte JSON array. Retain the prior external content
seed when upgrading an existing development configuration. Replacing it does not
re-encrypt queued content. `members` configuration and a bootstrapped deployment
are required. Omit `guests`/`simulation` for a normal build without the request page.

For a local file handoff, a build with `--features simulation` may instead select
`{"kind":"development","directory":"/private/local-outbox"}`. This retains the
existing explicitly selected development adapter; it is never a sending fallback.
`scripts/m3c-demo.sh` uses this configuration and needs no provider account.

When a live account is ready, replace only `delivery`:

```json
"delivery": {
  "kind": "smtp2go",
  "region": "global",
  "api_key_file": "/secure/smtp2go-api-key.txt",
  "dkim_selector": "cs2",
  "dkim_secret_file": "/secure/domain-dkim-rsa2048.pem"
}
```

`feedback` is a separate, required choice inside `external`. To receive callbacks
with sending still disabled, use:

```json
"external": {
  "content_secret_file": "/secure/external-content.json",
  "mail_domain": "mail.c-sqd.com",
  "delivery": { "kind": "disabled" },
  "feedback": {
    "kind": "smtp2go",
    "listen": "127.0.0.1:8582",
    "secret_file": "/secure/smtp2go-webhook.json"
  }
}
```

The callback listener must be loopback and separate from the main API listener.
It exposes only `POST /v1/external/smtp2go/events`; member, operator and readiness
routes are absent. The route has been removed from the main API. Both ports are
reserved before workers start, and both HTTP servers drain on shutdown. The receiver
has its own bounded pool of two concurrent storage calls and a 64 KiB body limit.
It rejects duplicate Authorization headers and does not log headers or raw bodies.
Authentication failures return 401; invalid input 400; identity/replay conflicts 409;
storage failures 503. Successful commits and identical retries return 204.

Cutover: remove `delivery.webhook_secret_file`, add explicit `feedback`, and point
the HTTPS proxy at the dedicated listener. The old configuration/route is not a
fallback. No schema migration is needed. Callback-only operation needs the existing
member/content keys and database, but no SMTP2GO sending API key or DKIM private key.
It starts no external delivery worker. Do not expose the main service port through
the callback proxy. Keep the proxy's access/error logging free of Authorization and
request-body data; do not enable request inspection at a tunnel provider by default.

Supported API regions are `global`, `us`, `eu` and `au`. The host selects the official
HTTPS endpoint; arbitrary production URLs cannot receive the API credential. The
library permits an explicit loopback HTTP endpoint only for local experiments.
The API key file contains plain text; the webhook secret is a 32-byte JSON array.
The DKIM key is an unencrypted RSA-2048 PKCS#8 PEM (`BEGIN PRIVATE KEY`). The host
validates it before starting delivery workers. The old Ed25519 JSON signing-key
format is no longer accepted. Operator, identity and content keys are unchanged;
incoming authentication still accepts valid external Ed25519 signatures.
Protect all private files with owner-only access and keep them outside version control.
The sending API key and webhook secret have separate authority.

Generate a fresh DKIM key with OpenSSL in a private directory (use a new filename):

```sh
umask 077
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out domain-dkim-rsa2048.pem
openssl pkey -in domain-dkim-rsa2048.pem -pubout -outform DER | openssl base64 -A
```

Publish only the second command's public output as `v=DKIM1; k=rsa; p=<output>`
at `cs2._domainkey.<mail_domain>`. A new selector allows publishing before cutover;
retain the previous selector's public record while already-sent messages may need
it. The checked-in RSA fixture is public test material, never a deployment key.

Enabling this configuration starts dispatching eligible Waiting messages. Review
the queue before connecting a real account. The original live test configuration is stopped; inspect its queue before any restart.

## Later account connection

1. Verify the sending domain in SMTP2GO with its supplied DNS records. Publish the
   public half of cs-mail's configured DKIM key at `<selector>._domainkey.<domain>`
   as well; this integration sends cs-mail-signed MIME. Do not reuse test keys.
2. Create a restricted sending API key. Disable open/click tracking, unsubscribe
   footers, archiving and auditing/BCC additions for this correspondence route.
3. Expose the dedicated callback listener through the deployment's HTTPS proxy,
   accepting only `POST /v1/external/smtp2go/events`. Keep the main API private.
   Configure the SMTP2GO webhook for the sending API key, JSON output, and processed,
   delivered, bounce, reject and spam events. Set its Authorization header to
   `Bearer <64 lowercase hexadecimal characters of the webhook secret>`.
   Never put the API key in the callback URL. The local cs-maild listener remains
   loopback-only; the deployment proxy must also keep authorization/body data out
   of access/error logs.
4. Connect only a designated test recipient first. Check SMTP2GO's actual MIME and
   webhook payloads, delivery/bounce outcomes and the existing inbound reply path.
   The receiver accepts flat plain-text/HTML alternatives; HTML-only mail and
   attachments remain unsupported. A local fixture round trip now passes, but
   outbound success alone does not establish a live provider round trip.

The provider memo's open item about SMTP2GO accepting member-authored correspondence
under open enrollment still needs confirmation before public operation.

## Agreed experiments and limits

The original implementation challenged these agreed claims:

| Claim | Counterexample sought | Consequence |
|---|---|---|
| Withdrawal prevents an unstarted handoff | A provider request after the cancellation commits | Disclosed content against current consent |
| Uncertain responses cannot duplicate sending | Two HTTP submissions after simultaneous workers, lost response, restart or failed receipt persistence | Two external disclosures/messages |
| Provider feedback affects only the correct message atomically | Wrong mailbox/persona/reference, reused provider ID, altered event replay, partial reconciliation or late-event overwrite | False or corrupted delivery history |

Two PostgreSQL experiments exercise real local HTTP calls and controlled responses,
including rejection inside HTTP 200, HTTP 401/503, malformed JSON, redirects,
provider-side disconnect, callback before submission response, and SQL trigger
failures. They inspect the actual MIME and verify its DKIM through controlled DNS.
The prior external-conversation and incoming SMTP experiments remain in the gate.

For the RSA/receipt follow-up, the owner agreed to adapt existing experiments to
challenge two claims: RSA-signed mail verifies and detects header/body tampering;
and provider acceptance is recorded without implying delivery or resending an
uncertain attempt. The HTTP fixtures now include compact fast-accept receipts,
complete accounting, missing IDs, contradictory errors, partial accounting and
server errors carrying IDs. Real DNS verification uses the RSA fixture; external
Ed25519 verification remains exercised by the incoming MIME scenarios.

Not covered by local experiments: provider queue durability, public webhook retries,
public inbound TLS/DNS, every crash location, throughput/load/fuzzing, provider
outage recovery UI, or attachment/HTML-only reception. The first live email proves
one delivery, not these broader claims. It is never resent or retroactively marked
Accepted on assumed API evidence.

The callback pass adds one agreed experiment through actual TCP HTTP and PostgreSQL.
It configures receiving with delivery disabled and verifies no sending worker starts;
rejects other routes, missing/wrong/duplicate authorization, oversized or malformed
input and mismatched recipients; injects a failed event insert and observes 503 with
no partial acceptance; observes an HTTP request waiting on an actual database lock
without acknowledging; and discards a committed success response before retrying.
The event is retained once, altered replays conflict, late processing evidence does
not erase delivery, and unsubmitted work remains Waiting. Synthetic builders remain
in development support. Existing domain-level feedback experiments remain unchanged.

This does not establish public HTTPS availability, SMTP2GO's actual callback field
format/IDs, provider retry behavior or proxy logging. Those require the connected
provider experiment. No new live email or public exposure occurs in this pass.

Protocol references checked for this pass:
[SMTP2GO MIME submission](https://developers.smtp2go.com/reference/send-mime-email),
[observed compact fast-accept response in another client](https://github.com/onetimesecret/onetimesecret/issues/4190),
[webhook fields and semantics](https://developers.smtp2go.com/docs/webhooks-overview),
[webhook authorization configuration](https://developers.smtp2go.com/docs/setup-a-webhook).
