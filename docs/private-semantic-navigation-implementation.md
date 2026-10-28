# Private semantic navigation implementation

This record implements the architectural memorandum of 25 October 2026 without
treating its proposed model and storage defaults as settled choices. Appendix B
and the owner's subsequent hardware/deletion instruction remain the product authority.
The current Mac integration adds offline EmbeddingGemma 2 text inference and
SQLCipher/FTS5/vec1 exact retrieval over retained correspondence. It is a bounded
retained-text implementation, not completion of the memorandum's format, draft,
synchronization or structured-search scope.

## Ownership and current code

`cs-mail-search` owns checked occurrence identity, exact source versions, canonical
text locations, passage checksums, embedding compatibility profiles and deterministic
derived-source generation decisions. Restored records use checked deserialization.
Equal text never merges accounts, occurrences or permissions. A passage checksum
checks its mapped text; it does not establish reading or reuse authority.

Source owners supply eligibility and time. A source transition produces one checked
consequence containing the next generation and extraction/synchronization or
removal/synchronization work. Removed identities cannot be revived; publication must
match account, source version, generation and current eligibility. Older draft
revisions cannot advance a newer source projection. This sequential revision check
is not a distributed draft-conflict algorithm. Application/storage adapters must
commit complete consequences atomically when they are connected.

`cs-mail-client/member/search.rs` replaces search code formerly in personal mailbox
organization. It authenticates selected original content, binds the result to its
account/message/version, and returns the matching canonical text part and exact
range. Native subjects, body text, quoted occurrences and shared-copy occurrences
retain different locations. The binding maps byte coordinates to UTF-16 for Swift.
Result subjects and commit times come from the authenticated matched message.
HTML coordinates currently name canonical extracted text, not a DOM/source-HTML map.
There is no fallback to an unverified excerpt when original content disagrees.

On account open, Swift dispatches Rust-owned local engine preparation in a separate
background task, before mail refresh completes and independently of retention work.
Rust checks the unlocked lifetime and custody, preloads currently eligible ready
projections, and exercises the existing encoder with static non-mail text whose
vector is discarded. Preparation creates no query identity or result, changes no
ranking policy, and does not republish retained embeddings. The numerical helper,
compatibility fingerprint and encrypted formats remain unchanged. No foreground
lock spans model startup. Lock invalidates the lifetime and interrupts the process;
late preparation cannot revive it. A foreground query arriving early shares the
engine queue and may wait for the remaining preparation. Optional preparation
failure does not prevent downloads; foreground search still reports engine failure.

The authoritative per-message encrypted commit now stores a Pending, Building or
Ready projection with source-bound passages, a compatibility fingerprint and vectors.
Four passages are prepared per numerical turn, outside foreground/domain locks.
Publication reacquires custody and rechecks current source identity, version, content,
expiry and unlocked lifetime before the existing atomic retained commit.

The previous word-posting implementation is removed. Index ciphertext uses v3 AAD;
existing v2 records require the explicit preservation-checked maintenance conversion
in `tools/development-continuity`. The member runtime has no predecessor reader.
Original copy ciphertext, account cache, writing, metadata and credentials are not
rewritten by that conversion. PostgreSQL has no schema change.

Rust supervises a bundled offline Python numerical/storage adapter through bounded
anonymous pipe frames. The adapter supplies tokenizer offsets, model vectors and
FTS/vector candidate lists; Rust owns eligibility, publication, reciprocal-rank
fusion, current-source validation and disclosure. A private SQLCipher session
projection contains FTS5 text and vec1 vectors, keyed with an HMAC-derived cache key.
The original credential is never passed to the subprocess. Lock or source removal
interrupts the worker and discards its session cache; durable source-bound vectors
remain individually encrypted in the authoritative device records. Whole-process
crash cleanup and forensic erasure are not established by these checks.

The pinned Mac package is about 2.3 GB, including Python dependencies, text-only
Gemma weights and the qualified SQLCipher/vec1 libraries. It runs Metal float32
with 512 normalized dimensions and a 256 MiB SQLite cache suggestion. This is a
replaceable initial Mac runtime, not a mobile distribution or footprint target.
Model inference has a 30-second supervisor bound; SQL query execution has a
2-second progress deadline. These bounds do not establish interactive latency.

Word and quoted searches assemble contiguous overlapping canonical part ranges
before matching; an embedding window is not a literal-search boundary. The search
domain owns checked query parsing, whole-word conjunction across message parts,
and literal phrases within a part, with Unicode lowercase expansion mapped to
original character boundaries. Word results enumerate every eligible retained
source, without the semantic candidate cap, and can authenticate original copies
when numerical projections are not ready or no encoder is available. Numerical
preparation is not required for this search. Disclosure still validates the exact
passage against the authenticated original and current authority.

Typing suggestions, full Words and Related are explicit result modes. Their
independent cursors are bound to the same query identity, scope and lifetime.
Related excludes every literal word match, including those not yet paged to Swift.
Only numerical suggestions/related candidates retain the bounded hybrid shortlist.
Swift requests five typing suggestions, switches to separate word/related groups
on Enter, and checkpoints submitted intent with navigation. No learned relevance
threshold or confidence score is introduced.

Each search captures a distinct request identity before dispatch, separate from
its unlocked lifetime. The worker rejects obsolete requests after queue waits;
superseding active search work terminates its subprocess without cancelling newer
or background work. Search pages retain that same request identity. Runtime
capability failure, temporary contention and query supersession have separate
outcomes. Swift pauses failed indexing and continues downloads, with bounded
contention retries and another pass available through ordinary Refresh.

## Locking and deletion

Search requests and background tasks capture an unlocked lifetime before dispatch.
Rust checks that lifetime before access, after waits and before disclosure. Lock
invalidates it without waiting on network or an index mutex. An active holder
releases its decrypted index when it exits. Explicit unlock establishes a new
lifetime; stale cleanup cannot close it. Swift independently rejects stale
presentation generations and clears search/navigation content on lock.

The existing signed deletion journal is durable before transport. Its pending
deletion suspends local retrieval until the original operation is reconciled.
Confirmation commits exclusions through the existing atomic retained-copy/index
contract. A failed checkpoint cannot expose the prior copy on a warm or reopened
client because the journal still requires suspension. A durably confirmed refusal
restores visibility; an uncertain outcome keeps the original operation and suspension.
This changes local pending-deletion visibility, not the other participant's copies.

Background retention runs outside the foreground client actor. It holds the operation
mutex only while choosing work or publishing a bounded checkpoint, never while
waiting for a download. After a wait it reloads current source state and checks
the original lifetime. Device secrets are reacquired after the download rather than
retained across it. A foreground deletion can prevent publication. Background
work yields when foreground state is busy. One-copy scheduling limits each turn.
Expiry cleanup removes a bounded set of copies and their search projections through the
same atomic commit boundary; rebuilding cannot restore expired entries.

## Presentation

A/B retain their existing mailbox navigation and reader. HTML readers expose the
verified canonical matched passage above the original presentation, since canonical
text coordinates are not a claimed DOM map. Opening a long email positions its
header and matched passage at the top. Opening a result carries
an explicit passage intent; opening an outgoing message or a whole conversation
retains its distinct meaning. The shared find owner verifies the supplied range
against displayed canonical text, then reveals that occurrence. The navigation
checkpoint preserves the selected passage alongside query, filters and list anchors.
Opening a passage grants no quotation, reference, sharing or external-retrieval
authority. Existing main-space writing and Column B source actions remain their owners.

## Decisions and remaining implementation

The owner's 25 October follow-up designates the available Mac as the embedding
comparison reference: MacBookPro18,2, Apple M1 Max, 32 GB. This replaces the earlier
newest-Mac proposal. Record OS, model/runtime precision, dataset and thermal/power
conditions in each report. iPhone runtime and resource validation remain separate
work; this desktop comparison establishes no mobile performance claim.

The selected backup maximum is seven days from deletion. The domain computes an
explicit checked deadline. Server record cleanup, backup inventory/rotation,
restoration filtering and deadline enforcement still require their own adapters
and operational evidence. No deployed backup-deletion guarantee is asserted here.
Deletion markers need durable retention independent of that deadline; an offline
device must not revive a removed identity on return.

Qwen3-Embedding-0.6B and the Apache-2.0 EmbeddingGemma 2 text-only configuration are
the current evaluation candidates, initially at 512 dimensions. The owner's follow-up
replaces the original gated EmbeddingGemma-300M candidate; the earlier memorandum's
candidate list is historical. Compatibility fingerprints bind weights, tokenizer,
preprocessing, passage rules, query/document instructions, precision/runtime,
dimensions and normalization. Adapter vectors are checked before use and
incompatible profiles cannot be compared. The Mac integration provisionally packages EmbeddingGemma 2 text-only with vec1;
model/engine selection remains subject to broader evaluation. See the
[measured comparison](embedding-model-evaluation.md) for results and limitations.

The local 512-dimensional float32 screen is complete for both current candidates
on the reference Mac. It uses 54 fictional source occurrences and 48 fixed queries,
not the owner's correspondence. EmbeddingGemma 2 text-only is preferred for the next
integration experiment because of its resource profile and competitive retrieval;
the small relevance sample does not establish a general quality winner. Quoted queries require a literal phrase; ordinary queries combine semantic and
FTS candidates. Identifier-specific precedence, negative/contradictory passages and
no-answer behavior remain material requirements. No structured query parser or
answer-generation threshold is implemented.

The completed isolated integration experiment connects Gemma 2 to SQLCipher with FTS5,
vec1 exact search and sqlite-vec exact search. All three measured query plans
preserved the fixture's eligible nearest neighbors and source-bound passages;
failure/reopen, removal, expiry, profile and sequential lock cases passed. At
100,000 synthetic vectors, all revised plans missed the fixed 250 ms p95 target.
Vec1 with metadata filters was faster, while sqlite-vec used less storage. That experiment made no final engine selection or app-store migration. The
[encrypted-store report](encrypted-vector-store-evaluation.md) retains initial
failed plans, the explicit query-plan revision, measured costs and coverage limits.

The subsequent [bounded-cache/Rust-worker experiment](vector-cache-worker-evaluation.md)
preserves exact eligible results across 192 oracle comparisons and 576 warm
measurements. Both engines meet the warm 100k target at the declared 256 MiB cache
suggestion: vec1 metadata about 26/11 ms broad/filtered, sqlite-vec about 99/9 ms.
Actual caches reach about 224–227 MiB; sampled worker RSS is about 290 MiB without
the embedding model. A separate Rust connection-owning worker rejects stale queued
and completed work and supports cooperative interruption, deadlines, replacement
and reopen in the declared cases. This is isolated adapter evidence, not integration
with the member client's lifetime/custody or native UI. Production key derivation,
safe storage bindings and atomic source publication/deletion were requirements
for the subsequent Mac integration above; realistic full-mailbox validation remains
required. The small-cache failures are retained; ANN is not
justified solely by those failures.

A subsequent owner-authorized small private-mail sample exercises real MIME body
selection, token-bounded passages, offline Gemma inference and both encrypted exact
stores. An observed empty-plain/nonempty-HTML case was repaired with a changed
preprocessing fingerprint and regenerated vectors. Technical parity, passage
verification and paths for owner-supplied searches pass. The owner completed
41 distinct query/message judgments for five searches: semantic top results were
useful for 5/5, lexical for 4/5 and hybrid for 5/5. Useful top-five counts were
17/25, 13/25 and 18/25 respectively. This small sample establishes neither recall
nor reliable no-answer detection. The generic protocol/tool is in
`tools/private-mail-evaluation`; source correspondence, queries and aggregate run
artifacts stay outside the repository. Temporary encrypted indices are discarded.
This is body-text development evidence, not production import, attachment/OCR
coverage, mailbox-scale performance or a native search release.

Required subsequent work includes structured extraction and original-source maps
for the complete confirmed format scope; bounded extraction/inference workers and
durable work publication beyond this retained-text slice; production-scale native
SQLCipher/vector performance validation; authenticated
encrypted embedding/draft exchange and device-key authorization; explicit concurrent
draft branches and resolution; ordered deletion records and acknowledgement rules;
broader hybrid retrieval policy, duplicate occurrence grouping and recorded-link expansion;
Find related correspondence beside composing; bounded model upgrades; and actual
Mac/iPhone performance and energy evaluation. Temporary evaluation artifacts need
an end condition and must not become parallel production implementations.

## Validation boundary

The agreed experiments name consequential counterexamples: delayed work restores
deleted content; lock allows late plaintext state; stale synchronization revives
deletion or overwrites writing; filtering hides permitted matches; extraction loses
source meaning; or background work disrupts ordinary correspondence.

The first slice extends the existing retained-custody experiment with warm/reopened
deletion-checkpoint failure and exact retry, old-lifetime queries after unlock, and
a lock while unlock is paused in custody, durable expiry cleanup, and a real
foreground send during a blocked background fetch. Separate domain experiments
challenge removed/expired publication, account substitution, older draft revisions
and incompatible vector profiles. These do not validate distributed synchronization,
actual inference quality, OCR, original HTML mapping, backup cleanup or reference
hardware performance. The Apple runbook records executed checks and runtime evidence.

## Primary technical references

- [Apple Mac announcements](https://www.apple.com/newsroom/topics/mac/)
- [Apple iPhone announcements](https://www.apple.com/newsroom/topics/iphone/)
- [Qwen3 Embedding model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B)
- [EmbeddingGemma 2 model card](https://huggingface.co/google/embeddinggemma-2)
- [SQLCipher design](https://www.zetetic.net/sqlcipher/design/)
- [SQLite vec1 manual](https://www.sqlite.org/vec1doc.html)
