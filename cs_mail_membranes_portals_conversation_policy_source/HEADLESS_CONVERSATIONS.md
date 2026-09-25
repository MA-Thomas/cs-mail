# Native one-to-one correspondence: domain contract

Agreed implementation slice, 2026-09-21. The general design guide remains
[Rust-domain principles](../docs/rust-domain-principles.md); these are cs-mail
product decisions, not additions to that guide. This document refines sections
6–7 of the accompanying memo where implementation questions were left open.

The subsequent [consent and key-custody slice](CONSENT_AND_KEY_CUSTODY.md) adds
recoverable content and makes scoped consent the authority for CSQD decryption.
It supersedes the original endpoint-only custody assumption.

## Identity and authority

A correspondence scope is an unordered pair of distinct product personas. It
groups conversations, but grants no contact permission. Sending still requires
standing **directional** permission or an authorized native express lane. Groups,
first-contact conversation sends, SMTP and imported history remain outside this slice. Accounts and their current operational keys own
persona authority; shared SubjectId and login identity are not correspondence keys.
Express lanes concern the relationship, usually not one conversation. Admission
checks the lane scope, current validity and allowance; allowance and delivery
commit together. Request classes do not grant permission or change reuse policy.

A conversation owns one current reuse policy: across relationships by default,
or originating relationship only. Either participant can restrict it. Relaxation
requires a proposal by one participant and approval by the other at the exact
proposal revision. Restriction cancels an outstanding proposal. All changes are
signed, revision checked, recorded and serialized with outgoing commitments.

## Content and provenance

Messages are immutable versioned documents containing text, inline image blocks,
references and retained quotations/shared copies. A checked `MessageSelection`
names one message and version plus an ordered, nonempty list of `TextRange` and
`ImageBlock` elements. Text ranges use UTF-8 byte offsets; images select a whole
block. Empty ranges, overlaps, duplicate images and out-of-order elements are
rejected, including during deserialization. Gaps are allowed and remain separate
selected blocks; no omitted material is silently copied.

Coordinates address the manifest's flattened block sequence: each plain text or
image part contributes one block, each quotation/shared copy contributes its
retained blocks in order, and each reference contributes one nonselectable block.
Thus a selection can target content inside a previous quotation while retaining
its containing message's provenance. The service checks block types, byte bounds
and versions without reading plaintext. Endpoint extraction additionally checks
UTF-8 character boundaries. Native clients verify decrypted documents against
their manifests.
Each document commitment includes a fresh cryptographically random secret held
inside the encrypted document. Public versions therefore do not expose a stable
fingerprint of short guessable plaintexts. Retries preserve the original document.

References contain pointers, not text or access grants. Quotations and shared
copies contain independently retained selected text and image bytes in order,
with source provenance. A mixed quotation is one attributed selection, rather
than unrelated text and image quotations. Deleting its source cannot remove any
of its independently delivered image bytes.
Sharing in this slice is copy delivery. It never grants remote access. A reference
to an inaccessible source may be deliberately sent when reuse permits it, but
resolving it gives no source information. Assessments distinguish access and
availability from reuse permission. No automatic quote fallback exists.

Every managed derivative retains its source edges. Subsequent reuse must satisfy
the current policies of its containing conversation and all known ancestor
conversations. This deliberately conservative first rule applies even when only a
part of a derived message is selected. It is provenance, not independently
configurable message policy. Missing source/policy records mean unresolved policy,
never unrestricted reuse. Arbitrarily retyped text or a malicious endpoint cannot
be recognized inside ciphertext; this is not a redistribution-prevention claim.

## Commit, retries and deletion

Preview assessment has no authority. The commit rechecks live account authority,
directional contact permission, and all current source policies under persistence
protection. It atomically publishes endpoint and recovery copies to the two participants'
mailboxes, the immutable manifest, an audit record of policy revisions and trusted
time, and the idempotent outcome. This mailbox commit is the disclosure boundary.
There is no external delivery queue in this slice; later fetch is access to an
already delivered copy. A later restriction governs new reuse and does not cancel
that completed disclosure. Exact retries return the original outcome; changed
retries conflict. Read responses are never cached as operation receipts.

Deleting one's retained conversation removes only that participant's existing
encrypted endpoint and recovery copies. It neither deletes another participant's copies nor closes the
conversation. Future messages can still arrive when contact permission allows.
Headers, source edges, policy state and minimal audit records remain control
records; they contain no plaintext. Deleted content is not recoverable through
retry receipts. Quoted copies elsewhere remain readable. Source resolution checks
the requesting participant's own retained copy and does not search another person's
mailbox. Missing content gives an unavailable result without erasing the reference.

## Endpoint and service boundary

Endpoint code owns document construction, passage selection, attributed copies,
encryption and decryption. Private workspace material is never an API input to
send: only an explicitly assembled outgoing document is serialized. The correspondence service
receives opaque ciphertext plus signed structural manifests and source edges. The
separate consent-gated custody service can decrypt specifically authorized copies
and re-encrypt them to a grant-bound destination.
Those edges are necessary control metadata and must not appear in recipient
responses unless deliberately transmitted in their authorized document.

`ImageContent` contains bounded image bytes and a declared PNG, JPEG or WebP
encoding. It does not contain an external URL or access grant. Image bytes are
encrypted together with text for each mailbox owner. Manifests expose block types
and byte lengths, but contain neither image bytes nor a public image-content hash.
Encoding declarations are not decoder validation: frontend decoding, rendering,
upload controls, alt text, cropping and out-of-line attachment storage remain later
work. URLs remain ordinary text; linked pages/images are never fetched implicitly.

The initial limits are 128 document parts, 128 flattened blocks, 128 elements per
selection, 1 MiB of retained text and 4 MiB of retained image bytes per document.
Compact serialized documents are limited to 24 MiB, including byte-array encoding
and escaped text. Restored documents and quotations obey the same aggregate bounds.

## Consequential validation claims

Tests challenge: a preview cannot survive a conflicting restriction as authority;
derived copies cannot shed known restrictions; references cannot grant access;
deleting a source copy cannot retract delivered quotations or reveal someone
else's copy; and retries/races cannot duplicate delivery or partially commit.
Falsifying any of these would violate privacy, ownership or durable disclosure.
PostgreSQL concurrency and rollback checks are required for storage guarantees.
Additional mixed-content experiments challenge malformed restored selections,
Unicode boundary errors, changed block types, selection inside quotations, and
image retention after source deletion without bypassing current ancestor policy.
These are new feature experiments authorized in this implementation, separate
from the earlier refactoring's prohibition on new test cases.

## Implemented API and storage

`cs-mail-correspondence` owns the checked conversation, scope, document, selection and
copy types. `cs-mail-application::correspondence::CorrespondenceService` accepts a
`SignedRequest` bound to product, account, persona, current operational key and
stable operation ID. Commands are `Create`, `ChangePolicy`, `Policy`, `Assess`,
`Send`, `Mailbox`, `Fetch`, `Resolve` and `DeleteCopies`.

Migration 0017 adds owner-specific recovery envelopes and consent records; the
[consent contract](CONSENT_AND_KEY_CUSTODY.md) defines their lifecycle.

The PostgreSQL implementation uses `PostgresAccountRepository`'s existing host
connection and account authority. Migration 0016 creates separate conversation,
immutable message, encrypted copy and receipt tables. Both message copies and the
receipt commit together. Receipts record actor, key, action, digest, trusted time,
policy revisions and the outcome; they never retain outgoing ciphertext. Read
operations produce no sender-visible events or durable read receipts.

The existing centralized receipt lock serializes policy/send races and account
changes; the adapter also locks the directional relationship and account rows.
New conversation creation and sends return `PendingCommands` while the canonical
protocol inbox has unprocessed commands. The host must drain it and retry, so a
previously received contact revocation cannot be overtaken by a new delivery.
Source loading is bounded to 1,024 message records. Private content is never
loaded for ancestry checks. This centralized locking is an explicit initial
throughput limit, not a claim of independent per-conversation scalability.

`NativeClient::compose_correspondence` assembles explicit document parts and obtains
the private version secret from the operating-system RNG; the domain constructor
receives it explicitly and stays deterministic.
`NativeClient::prepare_correspondence` encrypts the resulting `Document`
for the sender and recipient separately, with an additional recovery envelope for
each owner encrypted to the authenticated custody public key. `NativeClient::open_correspondence`
decrypts and validates it against the committed manifest. `Document::select`
extracts ordered text and image blocks from a locally decrypted source for reference
display; `Document::quote` uses that same checked extraction to construct a
`QuotedExcerpt`. Wrap the excerpt in `SharedCopy` for copy-sharing. A reference uses
`SourceReference(selection)` and carries no retained text or images.
For a text/image/text message, a mixed selection can be constructed as follows:

```rust
let selection = MessageSelection::new(message_id, manifest.version(), vec![
    SelectionElement::TextRange { block: 0, start: 7, end: 12 },
    SelectionElement::ImageBlock { block: 1 },
    SelectionElement::TextRange { block: 2, start: 0, end: 5 },
])?;
let quotation = document.quote(&manifest, selection.clone())?;
let reference = SourceReference(selection);
```

The client must obtain endpoint keys through authenticated key
discovery. OS device key vaults remain deployment work; the managed custody path
restores access to recoverable messages without lost device keys.

The recipient polls `Mailbox { after, limit }` (maximum 100) and advances to the
returned `next_after` sequence, then calls `Fetch` and decrypts locally. Sequence
gaps are valid. Deleted and expired copies are not returned; advancing over an
expired batch may produce an empty page with a newer cursor. Mailbox polling is
not a source-reading notification. Expiry prevents retrieval; explicit deletion
removes ciphertext. A scheduled physical-expiry cleanup and control-record
retention policy remain deployment work.

This is a library API with real account authentication and durable PostgreSQL
delivery, not an HTTP server or spatial client. It requires enrolled native
participants and standing or lane-based permission in the sending direction. The existing protocol
request/lane machinery is not replaced. Native first-contact
conversation integration, group audiences, general attachments, SMTP/import, remote
retrieval grants and desktop synchronization remain subsequent slices.

The typed-selection representation replaces the unpublished single-range anchor
representation. Document commitments use format v2; current send commands use signing domain v3
and require recovery envelopes. Older message manifests and selections are rejected; there is no fallback reader or
conversion of encrypted historical documents. Existing development data from the
single-range slice requires a separate fresh schema. No stored data is reset by
this implementation.

Run the concrete end-to-end scenarios using an isolated database:

```sh
CS_MAIL_TEST_DATABASE_URL=postgres://USER@127.0.0.1:PORT/DATABASE \
  cargo +stable test -p cs-mail-storage-postgres --test correspondence \
  -- --ignored --test-threads=1
```

The scenarios create enrolled accounts, exchange and decrypt actual endpoint
ciphertext, change live policy, delete individual copies, race independent database
connections, force a receipt-write failure, retry after restart and verify mailbox
access, key revocation and expiry. They do not claim live-provider or desktop UI
validation.
