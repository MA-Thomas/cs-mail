## Interface B fixed system appearance defaults — 28 October 2026

Light Terrazzo is the fixed light default and Dark Green is the fixed dark default,
using the member’s original saved snapshots exactly. The palette separates Defaults
from Your profiles. System defaults cannot be renamed, updated or deleted; original
user profiles remain editable and additional profiles may be saved. Live adjustments
create a custom appearance. The toolbar light/dark switch applies the corresponding
fixed default, and system/custom selection persists across reopening.

Validation: focused disposable Swift checks passed for exact snapshot promotion,
independence from user-profile CRUD, unchanged profiles when selection clears, and
empty/corrupt user libraries. Canonical signed build/install and running-executable
identity passed. Running Mac inspection covered both defaults, retained profile
selection, toolbar mode switching and reopening. Preference checks confirmed exact
values and unchanged original profile snapshots. Continuity checks preserved account
state, nonsecret credential metadata, signing requirement and Interface C sources
and preferences. Evidence: target/interface-b-system-defaults-{build,install,check}.log,
target/interface-b-system-defaults-{preservation,running}.json and
target/interface-b-system-defaults-profile-preservation.log. No credential bytes
were read or replaced. iOS, OS-driven automatic appearance changes and unrelated
Rust/PostgreSQL suites were not exercised; no persistent tests were added.

## Interface C and shared split-view presentation — 28 October 2026

C uses a continuous satin white-gold surface across Column A, the toolbar and outer
workspace, inspired by the supplied 14-carat palladium white-gold ring reference.
A seven-stop diagonal reflection retains distinct highlights, shaded bands and
warm-gray tonal variation. The complete pattern is translated left by 10% of window
width, preserving its angle, stop colors and contrast. Faint stationary microdetail,
opaque content and independent mode-only profiles retain their existing owners.

B and C use WorkspaceColumn sizing and WorkspaceSplitSurround decoration. The shared
surround paints the sidebar toolbar cap and conceals the separator with a two-point,
noninteractive mask, sampling each interface's backdrop at workspace size. macOS
owns the divider cursor and dragging. No separate cursor/resize controller is used.

Current validation: canonical signed build/install, strict signature, running-binary
identity, continuity fingerprints and diff checks passed. Running inspection covered
C's light Home and light/dark Inbox, including the translated reflection pattern and continuous surround.
Left on light Inbox, matching the member's latest navigation. Original account state,
nonsecret Keychain metadata, signing requirement and Interface B sources were
preserved across installation. Evidence: target/interface-c-metal-baseline-build.log,
target/interface-c-metal-baseline-install.log, target/interface-c-metal-baseline-preservation.json and
target/interface-c-metal-baseline-running.json. No credential bytes were read or replaced.
No persistent tests or unrelated backend suites were added/run. iOS and exhaustive
resize/accessibility checks were outside this visual adjustment.

Shared split validation is recorded in target/interface-bc-split-build.log,
target/interface-bc-split-install.log, target/interface-bc-split-preservation.json and
target/interface-bc-split-running.json. Pointer dragging in both interfaces changed
Column B from 236 to 298 points and back. The inspection tool does not capture the
system cursor glyph, so its visible shape was not independently confirmed.

## Search engine preparation on account startup — 26 October 2026

The unified Mac app now dispatches local search preparation immediately after the
existing account's search lifetime opens, independently of mail refresh and the
retention/download task. Rust preloads currently eligible ready projection rows and
exercises the existing encoder with static non-mail text, discarding that temporary
vector. No query identity, search result or retained-source publication is created.
The numerical helper and model fingerprint are unchanged; existing embeddings need
no reindexing. The first actual query still builds its current allowed-occurrence
view and any needed vector index. No foreground/domain lock spans model startup.

Lock invalidates the lifetime and terminates the helper. A late background completion
cannot reopen the account; reopening starts preparation for the new lifetime. An
early query shares the engine queue and may wait for the remainder of startup.
Optional preparation failure does not strand mail download or conceal foreground
search errors. No spinner, alternate app mode or permanent diagnostic log was added.

Validation: ten client checks passed (`target/search-startup-client.log`), including
preparation leaving captured query intent and retained ciphertext unchanged and
closed-lifetime rejection. Focused client/bridge Clippy passed
(`target/search-startup-clippy.log`). The packaged real-model/encrypted-store scenario
passed (`target/search-startup-packaged-runtime.log`): explicit preparation took
11.486 seconds and the subsequent first query took 44.479 milliseconds on isolated
fictional data; allow-list, ciphertext and lock/reopen checks also passed. These
timings are one small local experiment, not a large-mailbox performance guarantee.
Native retention failure-isolation checks passed
(`target/search-startup-native-scheduling.log`). Formatting and diff checks passed.
PostgreSQL, iOS, broad energy/performance and live-provider suites were not run;
this change has no service, schema or identity-model source alteration.

The canonical signed app was built and installed (`target/search-startup-apple-build.log`,
`target/search-startup-install.log`). Running-app inspection found the helper already
running on Home before any query. Lock terminated it, immediate Lock after reopening
left the account locked, and reopening started a fresh lifetime. The first inbox
query after background preparation returned the five suggestions for “doctors
appointment”. Existing account/credential/signing and model-profile preservation
are recorded in `target/search-startup-install-preservation.json`; all encrypted
device-state tables were unchanged across installation
(`target/search-startup-state-install-check.json`).

## First-search delay investigation — 26 October 2026

An empty typing-suggestion surface was reproduced after app restart. A temporary
trace recorded only query hashes, request identities, cancellation state and result
counts, not search text or correspondence. The first query completed in 12.296
seconds with five matches, all five present in the mailbox projection. Repeating
the same query after initialization completed in 0.239 seconds with five matches;
another warm query took 0.284 seconds. None of these requests was cancelled or
returned a completed zero-match result. These are observations of this small local
mailbox, not a general performance benchmark or proof about every reported empty
surface (`target/search-investigation-timing.json`).

The numerical helper initializes its model lazily on first inference. During that
initial wait, Swift displays an empty Suggested matches section without a pending
explanation. This reproduces the reported appearance without a ranking change;
the earlier description of a confirmed intermittent cancellation failure was
premature. Initialization and first-result presentation remain work to address.
The temporary diagnostics were removed and the normal canonical app rebuilt and
reinstalled. No search algorithm, threshold, publication, permission or credential
was changed in this investigation. Existing account continuity and the running
normal binary were checked before completion.

## Request-class audit corrections — 26 October 2026

Both Apple interfaces already offered request-class editing. This update corrects
the existing workflow: directory rows and profile refresh retain the exact alias
instead of substituting participant identity, with a distinct Swift alias reference.
An authenticated member read now inspects permission for that alias using the same
protected receiving-route rule as quote issuance. It waits for earlier received
commands to resolve, samples time under protection, and grants no send authority.
Pair-level pending-request limits and history remain separate from alias permission.

The editor displays Rust's published-class quotability and Rust-owned collateral
input feedback. Invalid amounts disable Save with a field-specific explanation;
refresh preserves unfinished writing. Publication checks the current policy again.
A definitive collateral refusal identifies the class and current bounds, preserves
the authored edits and connection, and resolves only its original journaled
operation. Uncertain acknowledgements retain the existing exact-operation retry.
Obsolete implementation-pending pricing documentation was reconciled. No schema,
identity-model source, owner publication or credential change was needed.

Validation: ten client checks passed (`target/request-classes-client.log`), including
exact-operation recovery after a definitive field refusal and accepted-pair versus
selected-alias projection. Six PostgreSQL directory checks and three pricing checks
passed (`target/request-classes-directory-postgres.log`,
`target/request-classes-pricing-postgres.log`). These challenge independent same-owner
alias publications with equal names/class IDs, policy changes and corrected saves,
rollback, replay, publication authority and concurrent quote consistency. The final
protected alias-read rerun passed (`target/request-classes-alias-read-postgres.log`),
including a live lane scoped to A leaving B on the bonded route. Tests used isolated
schemas in the local test database, without adding fixtures to the owner's account.
An initial database URL lacking an explicit host failed before test setup; the
reported successful runs used the configured localhost connection.

Focused client/bridge/service/storage Clippy and final storage Clippy passed
(`target/request-classes-clippy.log`, `target/request-classes-storage-final-clippy.log`),
as did formatting and diff checks. Native SQLite and isolated Keychain checks passed
(`target/request-classes-native-storage.log`); browser enrollment was explicitly
skipped. No full quality gate, iOS, live payment-provider transaction or broad
performance run was performed. Policy-narrowing behavior was challenged in the
isolated database, not by changing the owner's operator policy to display that
warning in the running app.

The canonical signed app was staged and installed; the existing supervised backend
was rebuilt and restarted in place, with its configuration/database and configured
keys preserved (`target/request-classes-apple-build.log`,
`target/request-classes-install.log`, `target/request-classes-service-build.log`).
Running A/B inspection confirmed the formerly failing profile row opens, invalid
collateral feedback, save prevention and recovery on correction. Interface A also
restored a filtered directory search on Back and preserved an invalid unfinished
amount across Refresh. All temporary form edits were restored without saving a
publication. The app remains running in Interface B.

Original Keychain and service-key fingerprints, signing requirement, private writing,
personal mailbox and retained-copy ciphertexts passed preservation checks. App and
backend executable inodes match the installed binaries. Observation-cache and
retained-metadata snapshots changed during shutdown/ordinary refresh; those are
reported separately from durable writing/copies. Evidence is in
`target/request-classes-install-preservation.json`. No enrollment, account conversion,
credential replacement, owner publication or permission grant was performed.

## Typing suggestions and submitted search — 26 October 2026

This is the current search contract, superseding the ordinary-query behavior in
the historical records below. Typing shows up to five suggestions. Enter or
“See full results” shows every eligible retained message containing all entered
whole words, case-insensitively across its subject and message parts, with explicit
25-result pagination and no fixed total cap. A separate “Related by meaning” group
excludes all word matches. Suggestions and related results carry “These are the
closest matches. There may be no relevant match.” No confidence threshold is used.
Quoted queries require an exact case-insensitive phrase within one canonical part
and show no meaning group or caveat. Editing returns to suggestions; Back restores
the submitted intent, scope and expanded groups.

Rust owns parsing, whole-word/phrase matching, group membership, source eligibility
and independent group cursors. Swift owns presentation and navigation state. Word
and phrase search work without the numerical runtime, including retained sources
whose semantic projections are not yet ready. No schema or credential cutover was
needed.

Validation: ten client checks and three search checks passed
(`target/search-modes-client.log`). The named completeness conjecture was challenged
with 110 uniquely encrypted message occurrences, 25-result pages, cross-part words,
phrase separation, semantic exclusion, stale requests and expiry. Those fixtures
were isolated from the owner's mailbox. Focused client/search/bridge Clippy passed
(`target/search-modes-clippy.log`), as did native scheduling and SQLite/isolated
Keychain checks (`target/search-modes-native-scheduling.log` and
`target/search-modes-native-storage.log`). Browser enrollment was explicitly skipped.
The packaged isolated numerical roundtrip was not rerun for this change; actual
semantic queries were inspected in the running app. PostgreSQL, iOS, large-mailbox
latency and energy were not tested.

The canonical app was built, signed, installed and reopened in the original account
(`target/search-modes-apple-build.log`, `target/search-modes-install.log`). Interface
A showed five typing suggestions, complete word results, quoted phrase results,
the exact matched passage and Back restoration. Interface B showed five typing
suggestions, keyboard Enter submission, all-word results separated from related
results, editing back to suggestions and Back restoration. The app was left running
in Interface B. Original Keychain and configured service-key fingerprints and the
original signing requirement were preserved. Encrypted workspace, mailbox,
metadata and copy snapshots were unchanged across installation; normal subsequent
refresh may update encrypted observation caches. Signature and running-executable
identity checks passed (`target/search-modes-install-preservation.json`). No account
conversion, enrollment or credential replacement was performed.

## Search audit corrections — 26 October 2026

The audit's three corrections are implemented in the unified member app. Each
query now captures a distinct Rust request identity before dispatch. Changing or
clearing search supersedes it; the shared worker checks that identity after its
queue wait, before adapter work and before disclosure. Superseding an active
query terminates its subprocess; delayed cancellation cannot terminate a newer
query or a background indexing turn. Pagination is bound to the original query.

Pipe, startup and helper failures now report RetrievalUnavailable separately from
contention and supersession. The shared Swift retention pass interleaves bounded
indexing with downloads, pauses indexing after an encoder failure, and bounds
contention retries to three attempts per stage. Refresh can start another pass.
No spinner, technical status panel or alternate application mode was added.

Quoted queries assemble contiguous, overlapping canonical text-part ranges and
find the literal occurrence independently of individual embedding windows. The
result carries the exact original range, including Unicode lowercase expansion;
its text, source/version, current custody, receiving scope and expiry are still
checked against authenticated original content before disclosure. This changes
query behavior without changing model fingerprints, encrypted formats or schema.

Validation: ten client checks and two search-domain checks passed, including new
cross-window/Unicode, real-subprocess supersession and runtime-failure checks
(`target/search-corrections-client.log`). Focused client/bridge Clippy passed.
The packaged real-model inference/encrypted-store check passed separately
(`target/search-corrections-packaged-runtime.log`). Native scheduling checks
challenged permanent encoder failure, persistent index/download contention and
stale passes (`target/search-corrections-retention.log`). Existing native SQLite
and isolated Keychain checks passed; browser enrollment was explicitly skipped
(`target/search-corrections-native-storage.log`). Sandbox restrictions required
system access for signing, Metal and Keychain checks; successful reruns are the
reported evidence. No PostgreSQL or identity-model sources changed; their suites
and iOS were not run. Full-mailbox latency, energy and crash cleanup remain open.

The canonical signed app was staged, installed and reopened in the original
account. Interface B inspection confirmed quoted subject/body results, exact
matched-passage display, Back restoration and final-query provenance after query
changes. The boundary-crossing case was challenged in an isolated regression,
without inserting synthetic correspondence into the owner's mailbox. Original
Keychain and configured service-key fingerprints, private writing and retained
copy ciphertexts were checked; signing and running-executable evidence is in
`target/search-corrections-install-preservation.json`. Ordinary observation refresh
may rewrite encrypted cache/alias metadata. No account conversion or enrollment
was performed. The staged bundle was consumed by installation.

## Retained-text semantic search in the member app — 26 October 2026

The Mac member client now uses the provisionally selected offline EmbeddingGemma 2
text-only configuration with SQLCipher/FTS5/vec1 exact retrieval. Ordinary searches
combine meaning and words; quoted searches require literal text. Both A and B use
the same Rust client, account, durable source records and source-bound navigation.
Rust owns eligibility, profile compatibility, publication and final disclosure.
Swift preserves query/result provenance, receiving scope, Back navigation and lock
clearing. No spinner or extra application mode was added. A verified canonical
Matched passage is visible beside the original inert HTML presentation; this is
not a claimed DOM-coordinate map.

Each encrypted authoritative index record owns Pending, Building or Ready model
projection state. At most four token-bounded passages are inferred per turn outside
foreground locks. Publication rechecks current source content/version, expiry,
custody and unlocked lifetime before the existing atomic retained transaction.
Rust supervises the offline numerical/storage subprocess using bounded anonymous
pipes. Its SQLCipher session projection uses an HMAC-derived key, a 256 MiB cache
suggestion and memory-only SQLite temporary storage. Lock/source removal interrupts
it; normal parent exit also closes the pipe and removes the encrypted session
cache. Durable vectors remain individually encrypted with their source records.
The pinned Mac resource package is approximately 2.3 GB; iOS model packaging,
full-mailbox end-to-end latency, memory/energy and crash/forensic-erasure guarantees
remain unvalidated. No owner Takeout correspondence was imported into the app.

The explicit v2→v3 derived-index cutover preserved all 15 original copy ciphertexts,
account cache, private writing, personal organization, metadata and the existing
Keychain credential. Rollback was rehearsed before atomic publication. The
maintenance tool has guarded retry behavior; an exact retry was not exercised.
The member runtime has no predecessor word-index reader or fallback implementation.
Canonical builds compare the original designated signing requirement before
replacement. `scripts/apple-build.sh --stage` leaves the installed app alone;
`--install-staged` installs the checked staged bundle at the canonical path.

Validation evidence: all seven enabled quality-gate checks passed, with PostgreSQL
explicitly skipped (`target/semantic-integration-quality-final.log`). The seven
existing Rust client authority/custody scenarios use deterministic numerical
fixtures and do not establish model quality. The separately executed packaged
real-model check challenges actual inference/storage, alias filtering, encrypted
DB/WAL canaries, lock invalidation and reopened worker isolation. Native SQLite
rollback/exclusivity and isolated Keychain checks passed
(`target/semantic-native-storage-final.log`); inert HTML/resources/network/script
and find checks passed (`target/semantic-reader-check.log`). Broader native fixture
enrollment stopped with Offline: the existing mail service was healthy, but its
separate local identity service was unavailable. This is not public/live-provider
delivery evidence. No PostgreSQL schema or identity-model sources changed.

Final canonical build/install and strict verification passed
(`target/semantic-release-build.log`, `target/semantic-release-install.log`). The
running executable matches that installation. Both A and B visibly show the
verified matched-passage area and original email with its header; B is restored.
Final client checks passed 7/7 (`target/semantic-client-final.log`), the real-model
runtime check passed separately (`target/semantic-packaged-runtime-final.log`),
and parent-pipe EOF cleanup passed (`target/semantic-runtime-exit-check.log`).
Final reader checks and focused bridge Clippy passed. The completed encrypted
cutover backup/plan were removed after continuity and source verification; compact
hash evidence remains. Superseded compiler incremental artifacts were reclaimed;
durable application stores were preserved.

The running app walkthrough found the fictional appointment email first for a
semantic paraphrase, verified quoted-phrase restriction, receiving-inbox exclusion,
Back restoration and lock clearing. Original account contacts and aliases reopened;
the existing Keychain fingerprint and configured service key fingerprints remain
unchanged. Strict signing, installed/running executable evidence and preservation
records are under `target/semantic-install-preservation.json`. The source format,
structured filters, identifier-specific precedence, reliable no-answer decisions,
attachment/OCR extraction, encrypted cross-device synchronization/draft conflicts,
backup enforcement and composing recommendations remain separate work.

## Small private-correspondence evaluation — 25 October 2026

The owner authorized a local body-text sample and supplied five searches before
results were shown. Generic tooling/protocol is in `tools/private-mail-evaluation`;
correspondence, owner queries and aggregate run artifacts stay outside the
repository. Temporary indices are SQLCipher-encrypted with ephemeral in-memory
keys and removed after the run; the source archive is unchanged. No app store,
account, signing requirement or Keychain entry was accessed or modified.

Real MIME processing exposed empty plain alternatives with readable HTML bodies.
The explicit fallback repair changes the preprocessing fingerprint and regenerates
vectors. Both exact engines pass numerical/parity, source-passage verification,
wrong-profile/key and owner-query path checks. A local Terminal grading workflow
completed with 41 distinct query/message owner grades: useful first results were
semantic 5/5, lexical 4/5 and hybrid 5/5; useful top-five counts were 17/25, 13/25
and 18/25. Recall and no-answer behavior remain unmeasured. This is body
text only, with attachment/OCR and full-mailbox scale still unvalidated. No message
body, subject, address or ranking is written into repository evidence or agent
tool output. Existing lock/concurrency/custody tests are not relabelled as private
sample validation.

The installed member app remains the previous drafting-appearance build. This
evaluation adds no native semantic-search workflow. No app build or unrelated
Rust/PostgreSQL suites were run for this development-tool-only pass.

## Bounded vector cache and isolated Rust worker — 25 October 2026

The follow-up experiment tests exact encrypted retrieval through a separate Rust
connection-owning worker. It does not change the member app, owner store, account,
Keychain or signing requirement. The installed app remains the previous
drafting-appearance build; there is no new native semantic-search workflow here.

Both engines meet the unchanged 100k warm-query p95 target with a 256 MiB cache
suggestion: vec1 metadata about 26 ms broad / 11 ms filtered; sqlite-vec about
99 / 9 ms. Actual SQLite caches reach about 224–227 MiB, with sampled Rust-process
RSS about 290 MiB without the embedding model. The 2 and 64 MiB settings fail;
512 MiB gives little additional benefit on this generated corpus. These are
synthetic storage measurements, not real-mail semantic quality or cold-start/UI
performance. No production engine is finally selected.

All 192 exhaustive eligible-oracle comparisons and 576 warm timings completed;
rankings match across cache settings and engines. The isolated worker passes
deadline interruption, active-query lock, queued-work rejection, completed-result
suppression, query replacement and old-ticket refusal after close/join/reopen.
Its initial small harness check exposed interruption during prepare; the corrected
worker passed before the full protocol was frozen. Earlier diagnostics and
small-cache failures remain review evidence.

See [the cache/worker report](../../docs/vector-cache-worker-evaluation.md) for frozen
inputs, actual memory costs, individual timings and cancellation limits. All
generated databases were removed. Production Rust/client custody integration,
atomic source mutation, representative corpus, model packaging, energy, iOS and
native interaction validation remain open. No app build or unrelated workspace/
PostgreSQL suite was run for this tool-only pass.

## Gemma 2 with encrypted vector storage — 25 October 2026

Completed an isolated SQLCipher/FTS5/vector-store experiment with Gemma 2's measured
512d vectors, comparing vec1 flat streaming joins, vec1 metadata prefilters with
current-source joins, and sqlite-vec vec0 with eligible-rowid filtering. A fresh
offline Gemma query also completed the path through keyword/vector retrieval to
checksum-verified fixture passages. This is development evidence outside the app:
no owner store, account or Keychain was opened, no store was migrated and the
installed app remains the previous drafting-appearance build.

All arms preserved the same 48 fixture query rankings and verified 434 returned
passages each. Publication rollback, durable removal with failed cleanup/reopen,
stale generation, profile substitution, query-time expiry and sequential lock
counterexamples passed. Wrong/no-key access was refused; text/vector canaries were
absent from database/live WAL, and compile-time memory-only temporary storage with
a forced sort was checked. These are bounded experimental-adapter checks, not a
production custody/concurrency or forensic-erasure audit.

The scale run completed 144 exact oracle comparisons and 432 warm measurements.
Initial plans missed the latency target and remain recorded. An explicit revision
bounded vec1's candidate set before its outer sort and indexed source filters.
All revised 100k plans still missed the unchanged 250 ms p95 target: vec1 metadata
measured about 334 ms broad / 311 ms selectively filtered; sqlite-vec 473 / 351 ms.
Sqlite-vec used about 229 MiB versus vec1's 418 MiB. No engine is finally selected.
See [the measured report](../../docs/encrypted-vector-store-evaluation.md), including
SQL plans, source/build hashes and preserved failures. Temporary test databases
and their keys were removed. Cache-budget/Rust-worker, representative ANN recall if
needed, battery, iOS and native foreground behavior remain unvalidated. No app build
or unrelated Rust/PostgreSQL suite was run for these evaluation-only changes.

## Local embedding-model comparison — 25 October 2026

Measured Qwen3-Embedding-0.6B and, at the owner's direction, Apache-2.0
EmbeddingGemma 2 with its vision/audio encoders disabled. The available M1 Max Mac
with 32 GB is now the reference for this comparison. Both ran offline in the same
pinned Metal/float32 runtime at 512 dimensions, against the same frozen fictional
correspondence and relevance judgments. No owner mail, app store or Keychain was
opened by the experiment, and the installed app is unchanged by this work.

The 35 positive test queries gave embedding Recall@5 of 94.3% for Qwen and 100% for
Gemma 2; both put relevant material first for 74.3%. Fixed BM25/embedding fusion
raised first-result accuracy to 80.0% and 88.6% respectively. Exact identifiers,
opposite meanings and no-answer behavior still need explicit retrieval rules.
The paired MRR interval includes zero; this small agent-authored screen does not
establish a general quality winner. Gemma 2's lower sampled resource cost supports
using its text-only configuration in the next integration experiment.

The [full measured report](../../docs/embedding-model-evaluation.md) records the
protocol, checkpoint/runtime hashes, individual failures, frozen gates and resource
measurements. Vector shape/norm, score-derived rankings, shared runtime versions,
frozen inputs and fixture eligibility were checked against the saved results.
Battery/energy, iOS, native foreground responsiveness, quantization and production
storage/authorization boundaries were not measured. This evaluation does not add
semantic inference or structured search to the app. No unrelated Rust/PostgreSQL
suites or app rebuild were run for evaluation-only files.

## Drafting surface follows appearance — 25 October 2026

The shared A/B ordinary message editor now uses a dark opaque canvas with light
default text in dark appearance, retaining white with dark text in light appearance.
SwiftUI, the native WebKit background and document canvas share one presentation
contract. Theme styles live in the document head, outside captured message HTML;
switching appearance does not reload the draft. Explicit text colors and imported
design content retain their authored formatting.

Canonical build/install, original signing requirement and strict verification passed
(`target/composer-appearance-build.log`). The normally reopened running executable
matches the installation; configured key fingerprints remain unchanged
(`target/composer-appearance-preservation.json`). Running Interface B verified readable
writing in dark/light appearance, preserved text and caret when switching, and
save/reopen with the same subject/body. Restored B dark; the unsent “Draft appearance
review” is saved and open. No message was sent. The existing focused WebKit check
passed (`target/composer-appearance-render.log`). Whitespace checks passed. No new
tests or unrelated Rust/PostgreSQL suites; a separate A walkthrough and iOS runtime
were not exercised for this presentation-only change.

## Source-bound passage navigation and search lifetime — 25 October 2026

The first private-navigation slice replaces the previous lexical-search owner with
authenticated source/version/passage results. A/B open the matching message and
reveal its exact canonical text occurrence; Back restores the query and results.
Result subjects and dates belong to the matched message. The scope explanation
identifies retained subjects/text and explicitly excludes files, pictures and
reference targets. This remains lexical search, not semantic inference.

Rust now owns explicit unlocked search lifetimes. Lock immediately invalidates
dispatched work, including an unlock waiting on custody; old cleanup cannot close
a newer lifetime. Pending signed copy deletion suspends retrieval even after a
checkpoint failure/reopen. Bounded expiry cleanup removes retained copies and index
words together. Background retention yields to foreground work and holds neither
the foreground operation mutex nor device secrets across a network download.
The new search domain owns checked occurrence/version/passage identities,
compatibility fingerprints and lifecycle consequences. Its future extraction/sync
work intents are contracts; their durable adapters are not implemented here.

Validation: all seven enabled quality-gate checks passed on the final source
(`target/semantic-navigation-quality-final.log`); PostgreSQL suites were explicitly
skipped. Focused domain/client counterexamples passed, including late publication,
warm/reopened deletion failure, lock during pending unlock, expiry after rebuild and
a foreground send during a blocked background fetch
(`target/semantic-navigation-domain-client.log`). The 100,000-entry retained-index
experiment passed with an in-memory adapter; it is not a SQLite/device or semantic
performance result (`target/semantic-navigation-retained-scale.log`). The isolated
native SQLite/Keychain/HTTP journey passed against fresh local fixture databases
(`target/semantic-navigation-native-journey.log`); final-build native storage and
isolated Keychain checks also passed
(`target/semantic-navigation-native-storage-final.log`). Public delivery was not tested.

Canonical build, original signing comparison, strict verification and installation
passed (`target/semantic-navigation-apple-build.log`). The app was normally reopened;
its running executable matches the installed final executable. Running B dark
opened and highlighted both a native exact phrase and an HTML body match, and Back
restored the query/results. Lock removed reader/search content; Open account restored
the original account. A also opened/highlighted the native phrase and restored its
query on Back. Restored B dark on Home. No messages were sent and no owner copies
were deleted during this walkthrough. Original signing requirement, configured key
fingerprints, account/persona ownership and all four requested addresses match the
preservation baseline (`target/semantic-navigation-preservation.json`); no original
Keychain credential was replaced or migrated.

Model inference, full file/OCR extraction, original HTML/DOM mapping, encrypted
embedding/draft synchronization, concurrent draft resolution, SQLCipher/vector
retrieval and composing-related retrieval remain unimplemented. Seven days is the
selected backup-removal maximum in the domain; operational backup deletion has not
been implemented or validated. iOS runtime, newest-reference-device performance,
VoiceOver and exhaustive format/navigation cases were not exercised. See
[the implementation record](../../docs/private-semantic-navigation-implementation.md)
for ownership, decisions, remaining work and evidence limits.

## Composer click targets and recipient selection — 25 October 2026

Fixed recipient suggestions disappearing on mouse-down before their buttons could
receive the click. SearchFieldClickAwayScope now scopes the recipient field and
its choices together in A/B; other search fields keep their existing behavior.
One click on a native contact populates To and switches Column B to Source message.
Choose a person, then a retained message; highlight text to enable Quote selection,
Reference selection and Share selected copy. These remain existing native inclusion
operations, with Rust permissions checked through the existing submission boundary.

ComposerControlTarget owns 44-point square icon regions across the shared toolbar
and attached-file removal. ComposerActionStyle supplies the same minimum height for
secondary actions. Recipient choices and native source actions have full-row hit
regions. Glyphs remain compact and the portal hover convention is unchanged.

Validation: canonical build/install passed (`target/composer-hit-target-build.log`).
Running Interface B accepted a native recipient with one click, retained the existing
subject/HTML/picture/file, exposed source actions, and inserted both a quotation and
reference from selected text. Removed the duplicate reference and kept one unsent
review quotation. Clicking the options button near its padded edge opened Import
HTML design. Interface A also accepted a native recipient with one click and switched
to the source picker. Restored Interface B dark. No messages were sent. This is a
presentation-only fix: no new tests or unrelated Rust/PostgreSQL suites were run.
iOS runtime, exhaustive button audit and keyboard/accessibility traversal were not
validated. Existing account, Keychain association and signing requirement remain.

## Formatted composer and imported designs — 25 October 2026

The shared A/B main-space composer now has compact formatting controls, links,
inline raster pictures with size/description controls, retained file attachments,
and subject editing. Column B can open older correspondence alongside writing;
native quotation/reference/sharing continues through its existing consent owner.
Unaddressed/external helper reading grants no permission to reuse native content.
Import a design accepts UTF-8 HTML and explicitly selected local image resources,
previews them without network access, and sends through the existing native or
external submission workflow. Imported designs may be replaced or explicitly
converted to editable text; there is no visual design studio.

Composed HTML, images and attached files are checked together in Rust and retained
in encrypted drafts and committed documents. Subject, presentation and bytes survive
save/reopen and retry. The current combined content limit is 4 MiB, with at most
24 images/files. File and data image URLs are rejected; relative imported resources
must be explicitly supplied from the selected containing folder. Remote pictures
remain unavailable during import preview. Normal received-message approval retains
its existing message/version-specific disclosure and durable retry behavior.

Validation agreed with Marcus: content preservation after reopen, attachment changes
during retry, and unexpected preview network access, affected Rust/PostgreSQL
boundaries, and running A/B inspection. All eight full quality-gate checks passed
(`target/composer-quality-final-all.log`), including production clippy and PostgreSQL.
Focused content checks passed (`target/composer-content-release-tests.log`). The
isolated native SQLite/FFI journey passed with retained 192 KiB files
(`target/composer-native-release-check.log`). Local SMTP multipart/DKIM experiments
passed (`target/composer-smtp-tests.log`); these establish local carrier behavior,
not public/live-provider delivery. The final production WebKit adapter check passed
(`target/composer-render-current.log`), including stale-selection formatting,
normal-weight pasted spans, inert imported content and no canary network connections.

Running-owner walkthroughs exercised B dark formatted drafting, file/picture insertion,
reopen, and Column B reading; A light exercised local-resource design import, styled
preview, subject preservation after recipient selection, save/reopen, and a fresh
blank draft. Review drafts remain unsent. A formatting issue observed during the
walkthrough was fixed and passed the final WebKit check. iOS runtime, VoiceOver, a full security audit and public delivery were not
validated in the walkthrough. Request writing remains text-only; arbitrary incoming
external attachments, CC/BCC and campaigns are outside this slice.

Canonical final build and installation passed (`target/composer-apple-build.log`).
Strict signature verification and baseline comparisons passed for the original
signing requirement, account/alias ownership rows and configured key-file fingerprints
(`target/composer-validation.json`). No Keychain entry was changed or replaced and
no owner database migration was needed. The established owner backend job was
updated and reports readiness. Computer Use temporarily timed out after the earlier
walkthrough; Marcus closed the app and explicitly authorized reopening. The updated
canonical app reopened successfully with the original account. In that running build,
whole-selection bold worked across both lines and the retained subject, picture and
file reopened intact. Interface B dark appearance was restored. The contextual picture controls were corrected to use an explicit popover and
verified in the final installed build: Small visibly resizes the retained picture,
and Apply description updates its accessible content. The text appearance menu
opens with font, size, color, indentation and alignment choices. The review draft
was saved without sending. Installed and running executable UUIDs match, and the
original signing requirement and strict signature verification still pass.

## Remove the Finances section — 24 October 2026

Removed Finances from the shared area/destination types, both Column A navigation
interfaces, the Column B financial organizer and layout preview, and the main-space
record/Q&A destinations. Removed their dedicated views, selection/filter checkpoint
state, record-order type and finance-only presentation colors. There is no hidden
Finances route or replacement navigation alias. Existing request and settlement
facts remain with their existing Rust owners and request workflows.

Canonical build/check compilation, original signing comparison, strict verification
and installation passed (`target/remove-finances-build.log`). Normally reopened with
the original account. Running Interface B shows Home, Communications, Relationships,
Aliases and Express Lanes, and Notes, with no Finances icon. Communications still
opens its populated Column B and main inbox. Source checks find no remaining finance
area, organizer or main-space destination in either interface. Whitespace checks
passed. Presentation-only: no new tests or unrelated Rust/PostgreSQL suites. A separate
Interface A runtime walkthrough and iOS runtime were not exercised.

## Remove the plain-text format toggle — 24 October 2026

Removed Show plain text/Show formatted email and their toggle state from the shared
A/B email reader. Supported HTML displays directly. Originally text-only mail and
text presentation after a renderer failure retain their existing handling. The
external-picture review action remains available only when pictures are missing.

Canonical build/check compilation, original signing comparison, strict verification
and installation passed (`target/remove-plain-text-toggle-build.log`). Normally
reopened with the original account and inspected the running Interface B HTML
notification: formatting and retained picture remain visible, and the format toggle
is absent. Whitespace checks passed. Presentation-only: no new tests or unrelated
Rust/database suites. A separate A/iOS walkthrough and renderer-failure injection
were not performed for this change.

## Quiet conversation actions — 24 October 2026

Removed Latest from native and external readers in both interfaces. Manage contact
now belongs to each reader's existing conversation-actions menu alongside private
organization: native opens the existing management sheet; external has a contextual
submenu. The unused permanent action row is gone. Explicit Command-F still reveals
local find and Escape dismisses it. Menu help no longer incorrectly promises that
all included actions are private organization with no permission consequences.

Canonical build/check compilation, original signing comparison, strict verification
and installation passed (`target/reading-actions-menu-build.log`). Normally reopened
with the original account. Inspected running Interface B native and HTML readers:
Latest and the prominent management button are absent, both existing menus include
Manage contact, and the native menu action opens the original management sheet.
Closed it without changes. Verified external Command-F/Escape still works.
Whitespace checks passed. Presentation-only: no new tests or unrelated Rust/database
suites. A separate Interface A runtime walkthrough and iOS runtime were not exercised.

## Full external-picture explanation — 24 October 2026

The shared picture-review sheet now wraps its complete disclosure explanation,
picture addresses and retry notes without a line limit. Explanation and resources
share a bounded scroll area; Cancel and Load remain outside it. The sheet has an
explicit usable width/height range rather than compressing the explanation above
a competing scroll view.

Canonical build/check compilation, original signing comparison, strict verification
and installation passed (`target/external-picture-sheet-build.log`). Normally
reopened the app with the original account. Inspected the running Interface B
popup: every sentence of the explanation and the full sample address were visible,
with both actions accessible. Cancelled without fetching any picture. Whitespace
checks passed. Presentation-only: no new tests or unrelated Rust/PostgreSQL suites.
Many-resource overflow, enlarged accessibility text and iOS runtime were not
separately exercised.

## Conversation find is hidden by default — 24 October 2026

Removed the persistent find bar from native and external readers in A and B.
Command-F explicitly reveals and focuses local find; Escape dismisses it and
clears its query. The normal reader shows only its existing conversation actions.
The header geometry remains shared when find is invoked.

Canonical build/check compilation, signing comparison, strict verification and
installation passed (`target/conversation-find-hidden-build.log`). Normally
reopened with the original account. Verified in the running Interface B HTML
reader that no search field appears on opening, Command-F reveals/focuses it,
and Escape removes it. Whitespace checks passed. Presentation-only: no new tests
or unrelated Rust/PostgreSQL suites. Native/A keyboard walkthroughs and iOS runtime
were not separately exercised for this correction.

## Reading hierarchy and outgoing email authorization — 24 October 2026

Native and external readers now share ConversationReaderHeader and portal geometry:
search is the first fixed row within the reading surface. Subject, sender/date,
notices and permission controls cannot move that row. Email subject is the heading;
mailbox identity is quiet message metadata. The external contact-level subject picker,
large sender heading, permanent transport explanation and prominent permission grid
are removed. Opening a mailbox row selects that exact conversation, including two
notifications from the same sender with identical subjects. Missing selected content
does not select a different conversation.

Manage contact owns blocking and current incoming-permission revocation, plus
outgoing consent withdrawal when applicable. The no-standing-permission example
offers Block and omits Revoke. Reply no longer requires a separate disclosure grant.
Explicit Send supplies outgoing authorization in its signed command: the Rust
application commits the grant and selected queued reply together. Failed commit
leaves neither effect; replay remains terminal/idempotent, withdrawal still cancels
waiting messages, and a new grant does not revive cancelled mail. Opening, drafting
and saving issue no grant. Block enforcement and picture permissions are separate.

Canonical build/install, original signing comparison and strict verification passed
(`target/reading-layout-build.log`). The matching simulation-enabled service was
built (`target/reading-layout-service-build.log`) and restarted through its existing
`com.csqd.cs-mail.owner-readable-aliases` launchd job, preserving database/config/keys.
The first manual start found the supervisor already listening; the established job
was then explicitly restarted on the updated binary. No second service remains.
The native inspector initially timed out while the old app was running; Marcus
normally closed it, and the installed app was reopened with the original account.

Running Interface B observations: native and HTML search share the same top position;
HTML subject heading and secondary sender/date; no subject dropdown or action grid;
contextual Manage contact menu; native find returns/highlights its match; selecting
either same-subject notification opens the selected message's timestamp. Retained
pictures remain available. Original account/persona associations, full alias records,
backend-key digests and signing requirement match the pre-update baseline, and the
canonical installed executable is running (`target/reading-layout-validation.json`).
No real mail was sent, external picture fetched, or contact permission changed.

Validation: 8 SMTP/PostgreSQL experiments passed in an isolated database
(`target/reading-layout-smtp-tests.log`), including the extended existing experiment
that injects a failed explicit Send to challenge atomic grant/message persistence,
then retries, replays and withdraws. Affected application/client library checks
passed (1 application, 7 client; FFI has no unit cases) in
`target/reading-layout-unit-tests.log`. Affected crates' all-target/all-feature lint
passed (`target/reading-layout-clippy.log`) after documenting the single extended
causal experiment's length. Native WebKit inert rendering/local find/embedded image
and long-body reflow checks passed (`target/reading-layout-render-check.log`).
Whitespace checks passed. No full workspace/production-feature quality gate,
separate Interface A runtime walkthrough, iOS runtime, real-send or live-provider
delivery validation was performed for this change.

## Remove the contact-conversations return button — 24 October 2026

Removed the “Conversations with <person>” chevron button from the opened native
conversation in both A and B, as requested. The existing mailbox-origin return
and contact controls retain their roles.

Canonical build/check compilation, signing-requirement comparison, strict signature
verification and installation passed (`target/remove-conversations-back-build.log`).
Normally reopened the app with the original account and inspected Elena Park’s
conversation in Interface B: the requested button is absent and the reading surface
remains intact. Whitespace checks passed. Presentation-only; no new tests or
unrelated Rust/PostgreSQL suites. A separate A runtime walkthrough was not performed.

## Email scrolling has one vertical owner — 24 October 2026

The HTML reader previously placed a fixed-height WebKit scroll pane inside the
conversation scroll view. The email body now fits its measured content height;
local app-owned observation updates that height after image layout and width
changes. Vertical wheel gestures over the HTML body pass to the conversation.
The same shared reader serves A and B. Sender scripting and external-request
restrictions retain their existing owners. Explicit search can reveal its selected
text through the enclosing scroll view.

Canonical build/install, original signing-requirement comparison and strict
signature verification passed (`target/email-scroll-build.log`). Normally
reopened the installed app with the original account. In Interface B, verified
scrolling down over formatted content reaches the footer, reversing over the
body returns to the header, retained pictures remain visible, and local search
reveals a lower-body match. The focused native rendering experiment also passed
(`target/email-scroll-render-check.log`, check compilation in
`target/email-scroll-check-build.log`): a long body exceeds the former height
cap, narrow reflow expands it, widening shrinks it again, and it has no remaining
vertical HTML overflow. Existing layout, embedded-image, local-find, blocked
network and disabled-sender-script observations passed in that same experiment.
Whitespace checks passed. No Rust/domain/database behavior changed; unrelated
suites were not rerun. Physical trackpad momentum, iOS runtime and a separate
Interface A live walkthrough were not exercised for this correction.

## Preserved HTML email with deliberate picture retrieval — 24 October 2026

Incoming email now retains its original MIME in the encrypted message, including
supported plain/HTML alternatives and embedded raster pictures. Rust owns the
accepted format, inert presentation, extracted text, message-bound approvals and
encrypted retrieval records. Both native interfaces use the same Swift reader.
Headings, tables, emphasis, sender styling and embedded pictures are retained;
plain text is available explicitly. Opening or searching correspondence does not
authorize external requests. The replaced plain-only SMTP selector is removed.

External pictures are reviewed by exact URL and selected for this message. An
attempt is saved before I/O. Retrieval permits public HTTPS raster pictures with
bounded size/time, refuses redirects and automatic retries, and stores successful
bytes separately from the original. A failed picture does not mark the member
service offline. Scripts, frames, external CSS/fonts/backgrounds and ambient
network requests are blocked. Deliberate hyperlinks open the external browser.

The canonical app was built, installed with the original signing requirement,
normally reopened and inspected using the existing account. Verified healthcare,
government, workplace and professional-network fictitious email; formatted and
plain views; table-text find; retained-search results and return position; both
interfaces; an explicitly selected public picture; redirect refusal; and retained
picture display after reopening. The remaining unapproved picture remains blocked.
The fictitious incoming messages used existing local backend admission/encryption
contracts and receiving permissions. No real email was sent or permission changed.
Account/persona associations, complete alias records and backend-key digests match
the pre-update baseline. Original Keychain credentials were neither replaced nor
rewritten. The running executable is the canonical installed app.

Validation: workspace tests, PostgreSQL suites, identity manifest, dependency
direction, formatting and application-binary checks passed in
`target/html-quality-gate-release.log`. That run reported two lint failures;
those cosmetic issues were corrected and both complete lint configurations passed
in `target/html-release-clippy.log` and
`target/html-release-production-clippy.log`. This is individual-check evidence,
not a claim that the release gate completed cleanly in one invocation. SMTP retry
checks passed in `target/html-smtp-tests.log`. Native WebKit checks passed in
`target/html-render-check-release.log`: layout, embedded raster display, disabled
sender script, blocked canary network and local find/selection. Named Rust
experiments challenge MIME fidelity/replay identity and durable explicit retrieval,
including failed-save and no-refetch cases. Build/install evidence:
`target/html-apple-build-release.log`; continuity and running-app evidence:
`target/html-validation-final.json`. Whitespace checks passed.

Limits: incoming attachments, SVG/animation, external styles/fonts/backgrounds and
structured native reuse of HTML selections are outside this slice. Extracted-text
search can differ from rendered matching for hidden text and whitespace. No iOS
runtime, full VoiceOver/decoder-abuse audit or live-provider email delivery was
validated. Resource expiry is enforced when read; physical cache removal is
opportunistic and also accompanies permanent native-copy deletion, not a scheduled
cleanup service. Outgoing composition remains text-only.

## Compact Interface B mailboxes — 24 October 2026

Inbox, Sent, receiving inboxes, retained-search results and Drafts now share a
compact row composition: aligned correspondent, subject and short date, a stacked
fallback for constrained widths, and contextual secondary text. Quiet separators
and perimeter feedback replace spacious portal treatment. Read/unread emphasis
and flags remain facts supplied by the existing mailbox projection. Selection
checkboxes, opening a row and its organization menu remain separate actions.
The header groups title, search and options; routine refresh timestamps and
repeated transport explanations are removed. Offline and uncertain delivery
outcomes remain visible. Outbox language distinguishes queued email delivery
from confirmed native delivery and unknown sending outcomes.

Canonical build/check compilation, original signing-requirement comparison,
strict signature verification and installation passed. Reopened the canonical
app with the existing account. Inspected populated Inbox in dark and light,
receiving inbox without redundant receiving captions, Sent, compact Drafts, and
the empty Outbox. Verified checkbox selection exposes bulk actions, opening an
organization menu does not open the row, returning from a conversation preserves
selection, and opening a search match and returning preserves its query/results.
Cleared the inspection selection/query and restored dark appearance and All
inboxes / Inbox. No sending, deletion or permission actions were exercised.
Evidence: target/compact-mailboxes-build.log. Whitespace checks passed.

Presentation-only: no new tests or unrelated Rust/PostgreSQL suites. Constrained
window fallback, keyboard traversal, populated unread/flagged and external-email
rows, offline warning, and nonempty Outbox outcome variants were not exercised
in the running app. No live-provider delivery claim is made.

## Remove Column A’s transient solid blue focus perimeter — 24 October 2026

Clearing focus after pointer activation still allowed a transient blue ring before
the action executed. Column A now explicitly excludes the shared solid teal focus
perimeter from rendering, even during transient focus. An unselected keyboard-focused
navigation icon instead uses a small neutral underline; selected icons keep cove
lighting. Home contact focus rings keep their separate role. Native navigation
focus effects remain disabled.

Canonical build/check compilation, original signing comparison, strict verification,
installation and whitespace checks passed. Normally quit/reopened the canonical app.
Inspected focused Home and keyboard traversal to Communications, followed by pointer
activation and repeated selection of Notes. Captured Column A after repeated clicks:
Notes retains its cove spill without a solid blue outline. The blue perimeter branch
is excluded for the navigation role rather than removed after a click. Original
dark appearance, profiles and account/credential association retained.
Evidence: target/column-a-no-blue-flash-build.log.

Presentation-only: no new tests or unrelated Rust/PostgreSQL suites. No frame-by-frame
mouse-down recording, light/iOS or full accessibility recheck in this pass.

## Column A pointer activation no longer assigns keyboard focus — 24 October 2026

The solid blue outline reported after clicking was the custom keyboard-focus ring:
Column A’s activation action unconditionally assigned focusedIcon. Pointer/native
button activation now clears that focus; explicit Space/Return activation retains
it. Navigation selection continues to own the steady area-colored cove outline.
Hover and preview state do not substitute for selection.

Canonical build/check compilation, original signing comparison, strict verification,
installation and whitespace checks passed. Normally quit/reopened the canonical app.
In the running dark Interface B app, clicking Notes and clicking it again retained
the purple cove perimeter with the window (not icon) focused. Tab focused Home;
Space activated it and retained keyboard focus. Clicking Notes afterward cleared
icon focus again. Screenshot inspection confirmed cove spill without a solid teal
focus ring after pointer activation. Left Notes selected in the original dark
appearance. Account, credentials, profiles and texture settings were retained.
Evidence: target/column-a-pointer-focus-build.log.

No new tests or unrelated Rust/PostgreSQL suites for this focused presentation fix.
Light mode, compact/iOS, VoiceOver and every pointer-preview path were not rechecked.

## Column A selected perimeter and cove light — 24 October 2026

Interface B Column A selection now uses one thin rounded perimeter and soft
exterior cove lighting instead of the teal side marker. The shared cove renderer
has a navigation-selection role using each area's reflection color for dark-mode
legibility; existing surface/preview lighting keeps its original role. No icon fill,
raised treatment or stacked bezel is introduced. Unselected hover remains a quiet
perimeter, and keyboard focus retains its separate crisp outer ring. Increased
contrast/Reduce Transparency selects a solid legible outline instead of glow.
Home contact rows do not acquire persistent selection.

Canonical build/check compilation, original signing comparison, strict signature
verification, installation and whitespace checks passed. Normally quit/reopened
the canonical app with original account association. The first dark Home inspection
found selection too faint; the final installed role refinement was inspected in
both dark and light Home screenshots. Tab moved focus to Communications without
changing Home selection; Space selected Communications, Right Arrow opened its
matching preview, and Escape dismissed it. Returned to Home and original dark
appearance, then cleared keyboard focus so the steady selected outline is visible.
Saved profiles, texture settings and credentials were retained.
Evidence: target/column-a-selected-cove-build.log.

Presentation-only: no new tests or unrelated Rust/PostgreSQL suites. Compact/iOS,
all area colors, full VoiceOver and accessibility override runtime checks, and
exhaustive pointer hover/press paths were not re-exercised.

## Quiet Column A and Home contact rows — 24 October 2026

Interface B Column A icons and Home contacts now share a quiet workspace-button
style. Removed Column A’s glass/embossed frames and Home’s filled, stacked-border
contact capsules. Navigation keeps its 48-point hit areas and teal leading active
marker. Contacts use full-width padded rows, names/status, trailing arrows and
subtle separators, retaining the existing Home arrangement choices. Hover is
perimeter-only, pressing gives distinct perimeter/content feedback, and keyboard
focus has a separate teal ring. Main-space lighting and texture settings remain.
The replaced navigation style and its unused private rendering branch are removed;
action controls and the separate relationship map keep their existing styles.

Canonical Apple build/check compilation, original signing comparison, strict
signature verification, installation and whitespace checks passed. Normally
quit/reopened the canonical app with the original account. Dark/light screenshots
verified the changed composition. Keyboard inspection found focused contact rows
did not activate with Space/Return; the final installed correction explicitly routes
both keys to the same action as click. Native Tab traversal focused Daniel with
a visible ring; Space and Return each opened his conversation list, and Back to
Home restored the overview. Column A click navigation, selected marker, focus
ring, Right Arrow preview for Communications and Escape dismissal were inspected.
A captured Home screenshot also showed restrained perimeter hover on a history row.
Left Home in Marcus’s original dark appearance with texture/profile values retained.
Evidence: target/quiet-home-navigation-build.log.

Presentation-only: no new tests or unrelated Rust/PostgreSQL suites. Compact/iOS,
all Home arrangements, full VoiceOver/increased-contrast checks, pointer-preview
exit paths and exhaustive hit-region/press-state coverage were not re-exercised.
No sends, permission mutations or credential replacement.

## Focused profile list and naming — 24 October 2026

Interface B’s palette now lists selectable profiles with an active checkmark.
Save as new profile switches only the Profiles section to Name/Save/Cancel;
rename uses the same focused form. Rename/Delete live in each row’s menu and
operate on that exact profile without selecting/applying it. Modified and Update
profile appear only when needed. Failed saves retain the form and entered name.
The replaced picker, permanent name box and always-exposed management row are
removed. Local profile storage format and existing appearance snapshots remain.

Canonical build/check compilation, original signing requirement comparison, strict
signature verification, installation and whitespace checks passed. Native inspection
of the older process timed out; its verified PID was stopped with SIGTERM and the
canonical update reopened successfully with the original account. Runtime Interface B
inspection covered list/checkmark rendering, blank new-name form with focused field
and disabled empty Save, Cancel without creation, successful Save returning to the
list, selecting Light Terrazzo, renaming an inactive temporary profile from its menu
without changing selection, and deleting only that temporary profile. Final list
contains Dark Green and Light Terrazzo, with Light Terrazzo still selected and its
original appearance intact. Screenshot inspection confirmed the reduced controls.
Evidence: target/profile-list-build.log.

Presentation-only validation: no new tests or unrelated Rust/PostgreSQL suites.
Storage-failure and duplicate-name form behavior, Modified/Update visibility, iOS,
full keyboard/VoiceOver coverage and another restart were not re-exercised.

## Named appearance profiles — 24 October 2026

Interface B’s palette now offers named appearance profiles, with Save as new,
Update profile, Rename and Delete. A snapshot records resolved light/dark mode,
default/custom texture color and both optional opacity overrides. Live changes
mark the selected profile Modified without overwriting it; selecting it restores
the snapshot, including when reselecting the same profile. Deleting a profile
leaves current appearance untouched. A scrollable palette keeps color/opacity
controls reachable below profiles. One local encoded document owns profiles and
selection, with restoration validation and refusal to overwrite unreadable data.
First adoption saved Marcus’s current dark-green appearance as Dark Green.

Validation challenged two claims: live edits must not silently overwrite a saved
appearance, and named snapshots must survive reopening and restore the complete
appearance. Canonical Apple build/check compilation, original signing-requirement
comparison, strict verification, installation and whitespace checks passed. Normal
quit/reopen retained the original account. Native Interface B interaction saved a
second profile, changed hue (Modified appeared), updated it, renamed it, restored
Dark Green’s original hue, changed to light mode (Modified appeared), and reselected
Dark Green to restore dark mode. Both profiles and the selected Dark Green survived
another quit/reopen. Deleted only the temporary check profile and restored Dark
Green. A preference comparison confirmed the saved Dark Green snapshot and restored
current values exactly match all original mode/color/opacity settings.
Screenshot inspection covered the running profile controls and green workspace.
Evidence: target/appearance-profiles-build.log.

No new automated tests or unrelated Rust/PostgreSQL suites for this presentation
change. Corrupt-storage recovery, duplicate-name refusals, delete-all/reopen,
system-appearance changes, a full keyboard/VoiceOver audit and iOS runtime were not
exercised. Profiles are local device preferences, with no cloud sync or export.
The installed canonical app is running with Dark Green selected.

## Separate live terrazzo opacity — 24 October 2026

The Interface B palette now includes Main-space texture and Surrounding texture
opacity sliders. Each applies immediately to its own surface scope; surrounding
also drives Column B at one-quarter strength. Opacity changes preserve the current
color choice. Intensity is relabeled Color saturation; Use defaults restores the
original raster and removes both overrides, including scheme-specific opacity
defaults. Optional presentation overrides are persisted as local preferences.

Canonical Apple build/check compilation, original signing requirement comparison,
strict verification, canonical installation and whitespace check passed. Normally
quit/reopened the signed app with the original account. In the running light-mode
Interface B alias workspace, incrementing main-space opacity from 18.7% to 28.7%
left surrounding at 34%; reducing surrounding to 24% left main at 28.7%. Screenshot
inspection showed corresponding live changes and quieter Column B. Use defaults
restored 18.7%/34% and neutral rendering; unchecking restored Marcus's existing blue
choice, with hue/brightness/saturation unchanged. Left original light appearance
and blue color active, with no opacity overrides. No account or credential changes.
Evidence: target/terrazzo-opacity-build.log.

Presentation-only scope: no new tests or unrelated Rust/PostgreSQL suites. Dark-mode
opacity overrides, override persistence after another reopen, pointer dragging,
iOS and full accessibility checks were not exercised in this pass. Opaque writing
surfaces and accessibility suppression retain their existing rendering paths.

## Live terrazzo color controls — 24 October 2026

Interface B has a palette toolbar popover beside light/dark appearance. Its
continuous hue/brightness field, native Hue/Brightness alternatives and Intensity
(saturation) slider update shared B backgrounds immediately. Use default colors
restores the original raster, retaining the last custom selection. Device-local
preferences persist across reopening; no account/client state is involved.

One deterministic generator produces the original raster, binder and neutral fleck
layers once. Custom rendering multiplies the neutral flecks by the chosen color,
retaining alpha and mineral variation; binder/noise remain independent. Changes
reuse cached layers rather than regenerating geometry. Window anchoring, existing
texture strengths, opaque reading/writing surfaces and accessibility suppression
retain their owners.

Validation: canonical build and CsMailAppleCheck compilation, original designated
signing-requirement comparison, strict signature verification, installation and
whitespace check passed. Normally quit/reopened the canonical app with the original
account. Interface B inspection verified palette access, live hue/intensity and
brightness changes through native adjustable controls, automatic default deselection,
custom persistence across reopening, and default restoration. Light/dark screenshot
inspection verified shared colored flecks and the original neutral default.
The original dark appearance and default colors were restored after checks.
Evidence: target/terrazzo-color-build.log.

Presentation-only scope: no new tests or unrelated Rust/PostgreSQL suites. No
quantitative frame-rate benchmark, pointer-drag automation, complete keyboard or
VoiceOver audit, increased-contrast/Reduce Transparency runtime check, iOS runtime
or Interface A appearance change. App-window captures crop portions of native
popover content outside the window; they do not establish full-screen placement.

## Main-space alias creation — 24 October 2026

Aliases and Express Lanes now defaults to New alias in the main space in A/B.
Column B and existing-alias New Alias actions use that same destination; the
creation sheet and modal-only state are removed. The guide remains an explicit
Column B choice. Unfinished inputs and submission/refusal state belong to the
shared session draft and survive navigation; successful creation opens the new
alias only while the creation destination is still active. Existing Rust creation
and receiving-policy contracts are unchanged.

Validation: canonical Apple build and CsMailAppleCheck compilation passed, with
original designated-requirement comparison, strict signature verification and
canonical installation. Normally quit/reopened the app and inspected Interface B:
Column A defaults to the inline form; guide and existing-alias detail navigation
work; Column B and detail New Alias open the main space without a popup; purpose
and name survive guide navigation and leaving/re-entering the area. Screenshot
inspection found the form within the existing main-space surface. Temporary form
text was cleared, with no alias created or permission changed. Original account
and credential association were retained. Evidence: target/main-space-alias-build.log.
No new tests or unrelated Rust/PostgreSQL suites for this presentation change.
Interface A compiled but was not inspected interactively; iOS, VoiceOver, actual
creation and in-flight navigation/failure races were not re-exercised.

## Column A/B divider top alignment — 24 October 2026

The wide-layout vertical divider now shares Column B’s top inset through
InterfaceBWorkspaceLayout.verticalInset. Its existing bottom and compact-layout
horizontal separator are unchanged. Canonical build/check compilation, original
signing comparison, strict verification, installation and whitespace checks passed.
Quit/reopened the canonical app and verified the aligned top edges in the running
Interface B Communications screenshot. Presentation-only: no new tests or backend
suites; compact/iOS layout not rechecked. Evidence:
target/column-divider-alignment-build.log.

## Light composer surround texture — 24 October 2026

Marcus clarified that the email body must remain solid white. Retained its native
opaque background and increased the shared light main-space terrazzo strength to
55% of outside strength. Dark remains at 25%, sidebar at 25%, outside unchanged.
The proposed transparent-writing modifier was removed; it is not part of this
implementation. No separate composer texture or replacement white surface added.

Canonical build/check compilation, original signing requirement comparison,
strict signature verification, installation and whitespace checks passed. Normally
quit/reopened the canonical app and inspected the light Write an email screen:
the body remains opaque white and header/margins/footer show stronger terrazzo,
subordinate to outside. Dark rendering code is unchanged. Left that screen visible.
Presentation-only: no new tests or unrelated backend suites; iOS/accessibility
variants were not rechecked. Evidence: target/light-composer-surround-build.log.

## Light main-space texture — 24 October 2026

Raised Interface B main-space terrazzo from one-quarter to one-third of outside
workspace strength in light appearance. Dark main space stays at one-quarter;
subdued sidebar and outside strength are unchanged. The shared environment-owned
role still reaches nested backgrounds and preserves accessibility suppression.
Canonical build/check compilation, original signing comparison, strict verification,
installation and diff whitespace passed. Normally quit/reopened the canonical app;
inspected Communications in both appearances. Left light appearance visible for
assessment. No new tests or unrelated backend suites; iOS and accessibility overrides
were not re-exercised. Evidence: target/light-main-texture-build.log.

## Quiet autosave — 24 October 2026

Removed routine Saving draft / Draft saved text from the A/B shared draft-status
surfaces. Automatic saving is unchanged; actionable save failures and content
limits remain. Canonical build/check compilation, original signing requirement
comparison, strict signature verification and installation passed. Quit/reopened
the canonical app and verified Interface B typing/clearing changes only the body,
with no routine save text. No emails sent. Presentation-only: no new tests or Rust
suites; A runtime, iOS and simulated save-failure behavior were not rechecked.
Build evidence: target/quiet-autosave-build.log.

## Inline recipient field and quiet composer actions — 24 October 2026

Replaced the recipient button/full-screen chooser with an editable To field and
inline recipient suggestions in A/B new writing. The message remains visible.
The workspace owns recipient query state across native/external editor transitions;
editing an unresolved recipient disables Send until Rust resolves and commits the
selection. Fixed conversation replies retain their existing recipient. Removed the
replaced chooser and navigation state. Save/close, discard and relationship entry
use shared text-only ComposerActionStyle; Send retains its interface primary style.

Validation: canonical build and CsMailAppleCheck compilation passed, original
signing requirement comparison and strict signature verification passed, and the
canonical app was normally quit/reopened. Interface B screenshot/interaction
inspection verified the plain editable To row and quiet secondary actions,
external inline suggestion/Return selection, contact suggestion selection, message
preservation through both editor transitions, and immediate Send disabling while
editing a changed recipient. A first runtime check found a missing draft observation
that left To disabled; the installed correction observes DraftEditor directly.
Validation writing remains saved and unsent. A runtime, iOS, VoiceOver, cancellation
races and incompatible-source confirmation were not rechecked in this pass.
This changes presentation only; no new tests, Rust/domain rules, credentials or
disclosure semantics changed. No emails were sent or unrelated backend tests run.
Build evidence: target/inline-recipient-build.log.

## Write before addressing — 24 October 2026

Marcus rejected mandatory recipient-first composition. Write an email now opens
an empty editor immediately in A/B. The To control opens the existing recipient
chooser when wanted. No recipient is required to write, save, close or reopen.
Rust owns the distinct Unaddressed target in the encrypted workspace, with no
submission path; assignment uses the same checked Retarget transition and keeps
the draft identity/text. Column B stays clear until native source tools apply.
No transport guessing, disclosure checkbox or explanatory copy was added.

Validation: six existing client tests passed. The recipient-change scenario now
starts with unaddressed writing, reopens its stored text, challenges submission
without a recipient, and exercises the existing atomic recipient changes. No new
test cases were added. Targeted client/FFI Clippy, formatting, identity manifest
and whitespace checks passed. The canonical app was installed with its original
signer after designated-requirement comparison, then normally quit/reopened.
Interface B runtime and screenshot inspection verified the blank editor opens
immediately; typed text saved and reopened without a recipient, Send stayed
unavailable, and later assigning write-first@example.com preserved the text.
The unsent check draft is saved; the app is left on a fresh blank editor.
No messages were sent, credentials replaced or permissions granted. Interface A
compiled but was not re-inspected interactively; keyboard/VoiceOver remain untested.
Evidence: target/write-first-build.log, target/write-first-tests.log and
target/write-first-clippy.log. No PostgreSQL, iOS or actual sending checks.

## Recipient-first composer installed and inspected — 24 October 2026

A/B Write an email now opens a main-space recipient picker. Known contacts,
directory names and exact addresses feed the existing native or external editor.
The selected native publication's exact alias crosses the Rust/Swift boundary and
is retained in a distinct NativeAddress draft, submitted through the existing
signed SendToAlias path. Current native namespace membership comes from the
authenticated owner alias portfolio. Missing native publications, empty namespace
evidence and transport/custody failures cannot silently select external email.
This is the existing single-deployment namespace, not federated discovery.

Change recipient is available for new writing only. Rust Retarget preserves the
same draft identity and message text in one encrypted workspace replacement,
checks the expected revision and refuses fixed replies/request targets. Native
subject/source removal for an external destination requires an explicit choice;
native-to-native changes retain their sources for normal send-time checks. The
picker ignores obsolete reads and prevents duplicate submit actions. Replies and
saved writing reopen their existing targets. Eligible native contexts retain the
existing relationship-request entry. Column B renders native source tools or
stays clear; no replacement explanatory panel or relay/status copy was added.

Validation: the canonical Apple build, including regenerated bindings and
CsMailAppleCheck compilation, passed. Signing verification and original designated
requirement comparison passed before canonical installation. The identity-source
manifest, formatting, diff whitespace and targeted client/FFI Clippy checks passed.
Six client tests passed with member-runtime enabled. Existing scenarios were
extended, with no new test cases: exact-alias submission is observed in the signed
command; native-address/transport failures refuse plaintext fallback; recipient
changes preserve identity/text, reject stale revisions and failed writes, preserve
native inclusions, require approval for their removal and refuse reply redirection.
These are local transport/store fixtures, not PostgreSQL or provider evidence.

The final build is installed at the canonical path with the original signer and
account association; no credentials, account or schema were replaced. After the
Mac became accessible, the app was normally quit and reopened from that path.
Interface B runtime inspection verified the main-space recipient picker, native
Daniel Reed composition with Column B source tools, external address resolution
for workspace-check@example.com with Column B clear, and message-text preservation
in both directions. Save/close and Drafts reopening preserved the text. The unsent
validation draft remains saved; no messages were sent or permissions granted.

Keyboard/VoiceOver, stale-query interaction, Interface A runtime transitions, iOS,
source-removal confirmation and send refusals were not exercised in this runtime
pass. Actual sending/public-provider delivery remains untested. No unrelated
PostgreSQL/full gate or live-provider work was run.

Evidence: `target/recipient-composer-build.log`, `target/recipient-client-tests.log`,
`target/recipient-core-clippy.log`, `target/recipient-client-clippy.log` and
`target/recipient-identity-check.log`. This entry records the earlier recipient-first
interaction; the write-before-addressing entry below supersedes that entry flow.

## Send label — 24 October 2026

Renamed Queue email to Send in A/B external composers. No behavior change or new
interface copy. Canonical build/check compilation, strict signing and original
requirement comparison, installation and whitespace checks passed. Quit/reopened
the original account app and verified B/dark Send in accessibility and screenshot.
Left the empty composer open. No message sent, new tests or backend suites; A runtime
and iOS not rechecked. Build: `target/email-send-label-build.log`.

## Relay footer removed — 24 October 2026

Removed the local-development-relay footer from A/B external composers at Marcus’s
request. Added no replacement copy. Canonical build/check-product compilation,
signing requirement comparison, installation and whitespace checks passed.
Quit/reopened the original account app; B/dark accessibility and screenshot confirm
the footer is absent. Left the new-email composer open. No new tests or unrelated
backend suites; A runtime, iOS and sending were not checked. Build evidence:
`target/email-composer-relay-copy-build.log`.

## External composer sentence removed — 23 October 2026

Removed “Write a text email. Native messages and references cannot be included.”
from the main-space external composer in A and B. Canonical build and check-product
compilation passed, with original signing-requirement comparison and installation.
Quit/reopened the canonical app with the existing account. B / new external email /
dark screenshot and accessibility confirmed the sentence is absent; left that
composer open. No sending behavior changed. No new tests or backend suites; A runtime,
iOS and actual sending were not rechecked. Diff whitespace checks passed. Build:
`target/email-composer-copy-build.log`.

## Checkbox removal verified after reopening — 23 October 2026

At Marcus’s request, quit cs-mail normally and reopened the canonical installed
bundle with the existing account. Opened B / Communications / Write an email.
Accessibility and a dark screenshot confirm the main-space composer has no
relay/plaintext consent checkbox or consent paragraph. Existing recipient/body,
save/close, discard and Queue email controls remain. Left the app on that empty
composer. No email was sent. This resolves the preceding native inspection and
reopening blocker; A, actual sending and iOS were not rechecked in this reopening.

## External composer without separate consent checkbox — 23 October 2026

Removed the relay/plaintext consent checkbox and its accompanying consent and
recipient-reminder paragraphs from new/reply external composers in A and B.
Marcus explicitly confirmed “Yes, send directly” when asked whether Queue email
should send ordinary email without ticking a separate box. That explicit action
now passes the existing disclosure intent for a new conversation. Replies retain
their existing conversation-authority checks; editing and saving do not authorize
or send anything. Rust authorization and disclosure-withdrawal contracts remain.

Canonical build and CsMailAppleCheck compilation passed, including strict signing
verification and comparison to the original designated requirement before canonical
installation. Diff whitespace checks passed. No new tests, unrelated backend suites
or actual email submission were performed. Evidence:
`target/email-consent-presentation-build.log`.

Native inspection repeatedly returned timeoutReached for accessibility state,
bundle-ID/path app access and screenshot capture. App inventory reports cs-mail
running, but the updated build could not be quit/reopened or visually verified.
The last observed running composer predates this installation. This is a native
inspection blocker, not evidence about application/backend performance. Reopen the
canonical app and inspect the composer before treating the removal as visibly
validated. Original account and signing association were not replaced.

## Main-space email and message writing — 23 October 2026

A/B new native conversations, native replies and external emails now compose in
main space. Reopening those saved drafts uses the same workspace; the replaced
composer sheets and internal composer split view are removed. Column B previews
retained native sources and offers explicit passage quotation, reference and
shared-copy actions against the same draft state. External email retains authored
text only and its existing disclosure-consent boundary. Request and follow-up
forms retain their existing presentation.

WorkspaceComposition owns the active DraftEditor, source selection and inclusions.
Navigation freezes editing while the latest body saves, respects the latest
navigation request, and retains the workspace on failure with Save/Discard/Stay.
Explicit discard can abandon unsaved edits after a write failure while Rust checks
the saved revision. Save & close restores the recorded origin. Native draft
reopening reads the full retained conversation, rather than the mailbox summary,
so the existing reply subject remains visible without moving the reading context.
The Swift principles and AGENTS composer convention record the owner's change.

Canonical Apple build and CsMailAppleCheck compilation passed. The build compared
the original designated signing requirement before installing at the canonical
path; strict installed-signature verification passed with local trust-store
access. Quit/reopened the final app with the existing account, aliases and drafts.
The identity source manifest and diff whitespace checks passed. No new tests or
unrelated Rust/PostgreSQL suites were run.

Runtime challenges: B new external composition appears in main space; entering an
unsent check recipient/body, navigating Home and reopening from Drafts preserved
both fields. Cleared that check's fields and closed it. B native Reply restored
existing writing; opening and highlighting a retained source changed no inclusion
until an explicit action. Quote, Reference and Share each produced their distinct
main-space inclusion, and Remove removed it. Removed all check inclusions and
preserved the original draft text. Save & close restored Daniel's conversation.
A new external composition and native draft reopening also used main space; final
quit/reopen verified the restored reply subject. Dark screenshots inspected A's
native composer and B's external/native/source layouts at the shared 236-point
Column B minimum. No message was sent, consent granted or permission changed.

Not exercised: actual submission, a native new-conversation entry under current
broader permission, external reply reopening, injected save failures, image reuse,
iOS, light appearance, smaller windows, alternate text sizes or VoiceOver. This
establishes local presentation, not public-provider delivery. Build evidence:
`target/main-space-composer-build.log`. Final app is left in B / new external email
composition / original dark appearance.

## Straight-sided Column B with matching rounded corners — 23 October 2026

Clarified rectangular treatment: Column B keeps straight sides with 24-point
corner rounding matching the main space. InterfaceBWorkspaceLayout now owns the
shared surfaceCornerRadius used by the sidebar, general main-space clipping,
backdrop/perimeter geometry and correspondence surface. No bow was restored.
Texture, selection, width limits, cove lighting and insets retain their behavior.
Swift principles record the clarified geometry.

Canonical build, CsMailAppleCheck compilation, strict signing verification,
original signing-requirement comparison and installation passed. Reopened the
canonical bundle with the original account and inspected B / Communications /
All inboxes in light appearance: Column B has straight sides and matching rounded
corners, with selection markers and main-space cove lighting preserved. Diff
checks passed. No new tests or unrelated backend suites. Other destinations,
dark appearance, iOS, alternate sizes and accessibility overrides were not
individually rechecked. Build: `target/rounded-rectangular-column-b-build.log`.

## Rectangular Column B panel — 23 October 2026

InterfaceBSidebarSurface now uses Rectangle for its clipping and quiet outline.
Removed the replaced bowed-shape path implementation. Texture, outline strength,
three-point divider clearance, content insets, shared measured width and selection
markers retain their values. The minimum-width comment now describes the retained
trailing breathing room; no width behavior changed. Main-space rounded geometry
and cove lighting are untouched. Swift principles record the rectangular panel.

Canonical Apple build, CsMailAppleCheck compilation, strict signing verification,
original signing-requirement comparison and installation passed. Reopened the
canonical bundle with the existing account and inspected B / Communications in
light appearance: Column B has straight sides and square corners, its measured
236-point minimum and teal selection markers remain, and main-space cove lighting
is preserved. Diff checks passed. No new tests or unrelated backend suites. Other
areas share this surface but were not individually inspected; dark appearance,
iOS, alternate sizes and accessibility overrides were not rechecked. Build:
`target/rectangular-column-b-build.log`.

## Shared Column B selection marker — 23 October 2026

A/B organizers now scope navigation selection through columnBNavigation and the
shared ColumnBNavigationSelection modifier: a decorative 3-by-18-point teal
leading marker and semibold label, with no persistent gray selection fill.
Mailbox, receiving alias, alias-detail, permissions and financial-record rows
use the same existing region styles within that scope. Relationships management
buttons and the alias guide now expose their active destination as selection.
Other content selections retain their existing style. Hit regions, sidebar width,
main-space cove lighting and native keyboard behavior retain their owners;
B’s transient hover/press perimeter and A’s hover feedback remain separate.

Canonical build, CsMailAppleCheck compilation, strict signing verification,
original signing-requirement comparison and installation passed. Reopened the
canonical bundle with the original account. Native B inspection verified Inbox
and All inboxes without fills, selection transfer to Unread, Relationships’ active
overview, the alias guide’s active marker and the permissions destination. Light
screenshots and a dark permissions screenshot confirmed marker/text legibility
and preserved reading/diagram surfaces. Returned to B / Communications / Inbox /
All inboxes / original light appearance. Diff checks passed. No new tests or
unrelated backend suites. Populated financial selection, individual alias rows,
A runtime, keyboard/VoiceOver and accessibility overrides were not individually
rechecked. Build: `target/column-b-selection-build.log`.

## One measured, noncollapsing Column B minimum — 23 October 2026

Replaced the Communications-only width conditional in A/B with workspaceColumn,
shared by every instantiated organizer. WorkspaceColumnLayout owns the heading
font and measures Communications using CoreText, with scaled heading size plus
44-point content insets and 24-point bowed-edge clearance. At the inspected text
size the resulting minimum is 236 points. SwiftUI receives the same minimum;
the macOS adapter also sets the containing NSSplitViewItem minimumThickness and
canCollapse=false, recovering a previously undersized/collapsed item. It observes
native resize and view lifecycle, without replacing the navigation owner or split
delegate; observation is removed on teardown. The existing maximum remains 360
unless enlarged text needs more space. Other long headings may wrap normally.
The earlier per-area 280/230 conditional is removed. Home and Notes keep their
intentional no-organizer layouts. No account/domain behavior changed.

Canonical build, CsMailAppleCheck compilation, strict signing verification,
original designated-requirement comparison and install passed. Reopened the
canonical bundle with the original account. Runtime conjecture: dragging through
the minimum must not collapse Column B, and normal resizing must still work.
In B Communications, widened the actual divider from 236 to 323 points, then
attempted to drag it near Column A: it clamped at 236 with the heading visible.
Repeated shrink-past-minimum attempts in Relationships, Aliases and Express Lanes,
and Finances all retained 236. Switched to A and verified its Communications
minimum and drag clamp at 236, then restored B / Communications / light. Native
screenshots confirmed the single-line heading and retained main-space styling.
Diff checks passed. No new automated tests or unrelated backend suites. A’s other
areas use the same modifier but were not individually dragged; whole-window
shrinking, alternate text sizes, iOS, dark mode, keyboard and VoiceOver resizing
were not individually checked. Build: `target/shared-column-width-build.log`.

## Compact mailbox sidebar reopening verified — 23 October 2026

Native app inspection reconnected. At Marcus’s request, closed cs-mail normally
and reopened the canonical installed bundle with the existing account. Opened
Communications / All inboxes in Interface B and inspected the light screenshot:
mailbox rows show tighter spacing and Communications stays on one line. Left the
app on that screen. This resolves the preceding reopening/visible-layout blocker.
Minimum-width dragging, A, iOS and accessibility text sizes were not tested in this
reopening. No build or account changes were needed.

## Compact mailbox rows and single-line Communications heading — 23 October 2026

A/B Communications mailbox sections now own a four-point row gap, with eight-point
vertical and unchanged twelve-point horizontal label padding and a 36-point
minimum row height. Full-width hit regions, icons, labels, selected/hover states
and navigation actions remain. Receiving-inbox rows retain their prior spacing.
Communications alone now uses a 280-point minimum Column B width (previously 230);
its ideal remains 280 and maximum 360. Its heading keeps its intrinsic single-line
width. Other areas retain the previous width limits.

Canonical build, CsMailAppleCheck compilation, strict signing verification,
original signing-requirement comparison and installation passed. Diff checks
passed. No new automated tests or unrelated backend suites for this presentation
change. Native app access repeatedly returned timeoutReached using bundle ID and
canonical path, including after inventory reconnect and tool-session reset.
Inventory still listed the existing cs-mail process. Reopening the running copy
and native verification were not completed; installed output is current, but the
running process is not verified to have loaded it. Rendered spacing, minimum-width
resizing, pointer/keyboard behavior, A, iOS, dark/light and accessibility sizes
remain unverified in this pass. Build: `target/compact-mailbox-sidebar-build.log`.

## Main-space vertical edges align with Column B — 23 October 2026

InterfaceBWorkspaceLayout now owns the shared 8-point vertical inset for Column B
and the general main-space surface. Main-space horizontal spacing remains 20
points. Previously its uniform 20-point padding placed its top 12 points below
Column B and its bottom 12 points above. Rounded geometry, cove-light overlay,
subdued interior and workspace-strength outer backdrop remain. Specific contact
portal composition was not altered. Swift principles record shared alignment.

Canonical build, CsMailAppleCheck compilation, strict signing verification,
original signing-requirement comparison and installation passed. Reopened the
canonical app with the original account and inspected Communications / All inboxes
in dark appearance: the top and bottom rounded surface extents align with the
bowed Column B panel, and cove lighting remains. Diff checks passed. No new tests
or unrelated backend suites. Other shared destinations, contact portals, iOS,
alternate window sizes and accessibility overrides were not individually checked.
Build: `target/main-space-column-alignment-build.log`.

## Outer backdrop matches workspace; cove lighting preserved — 23 October 2026

The backdrop after workspaceDetail’s 20-point padding now explicitly uses the
workspace texture role. Previously it inherited mainSpace’s quarter strength,
producing a rectangular texture transition outside the cove lighting. The rounded
interior and nested content still inherit the reduced mainSpace strength. Rounded
clipping, cove-light overlay, embossed shoulder, padding and sidebar were not
changed. Swift principles now distinguish the outer backdrop from the interior.

Canonical Apple build, CsMailAppleCheck compilation, strict signing verification,
original requirement comparison and installation passed. Reopened the canonical
bundle with the existing account and inspected Communications / All inboxes in
dark and light: outer terrazzo continues into the surrounding workspace, and the
rounded surface and colored cove lighting remain. Left in the original dark
appearance. Diff checks passed. No new tests or unrelated backend suites. Other
destinations inherit the shared composition but were not individually inspected;
iOS, alternate sizes and accessibility overrides were not rechecked. Build:
`target/main-space-outer-backdrop-build.log`.

## Main-space appearance change reverted — 23 October 2026

At Marcus’s explicit REVERT request, restored workspaceDetail’s previous rounded
backdrop and InterfaceBMainSpacePerimeter overlay exactly, removing the replacement
black shadow. Removed the new no-outline rule from the Swift principles. All
preceding alias, sender-permission and sidebar changes remain. Canonical build,
CsMailAppleCheck compilation, strict signature verification, original requirement
comparison and installation passed. Reopened the canonical app with the existing
account and verified the restored colored cove lighting and rounded boundary in
Communications / All inboxes, dark appearance. No new tests or unrelated backend
suites were run. Other screens and accessibility overrides were not individually
rechecked. Build: `target/revert-main-space-outline-change-build.log`.

## Standing permission in both directions — 23 October 2026

The Rust navigation projection and its generated Swift bridge now expose each
contact’s incoming and outgoing PermissionRoute independently, using the existing
current relationship interpretation. Both A/B permission views list standing
permission alongside sender-scoped lanes in either direction. Standing rows now
open a directional read-only detail with an Open correspondence action and the
existing return path. Outgoing detail offers no grant/revoke controls. A refresh
that removes standing permission replaces the active-grant description. Receiving
alias names are not supplied by RelationshipView; details explicitly state that
limitation rather than inferring account-wide scope. No wire/cache schema, domain
permission, durable grant or account credential changed.

Extended the existing client experiment to challenge directional isolation:
incoming acceptance cannot invent outgoing standing permission, an accepted
outgoing-only relationship is visible, and outgoing revocation does not remove
independent incoming standing permission. The targeted member-runtime test passed
(1 test); the initial invocation without member-runtime selected zero tests and
is not evidence. Canonical build, CsMailAppleCheck compilation, regenerated bridge,
strict signing verification, original signing-requirement comparison and install
passed. Reopened the final canonical bundle with the existing account and checked
B’s incoming list and outgoing standing empty state. The preceding build’s native
inspection also covered the new incoming standing detail and Back to Permissions,
and the outgoing light composition. Left the final app on Outgoing. Diff checks
passed. No new test cases or unrelated Rust/PostgreSQL suites were run. Account
has no outgoing standing grants, so populated outgoing detail and live revocation
were not exercised in the native app; scopes, A, iOS, dark/compact layouts and
keyboard/VoiceOver interactions were not individually verified. Logs:
`target/outgoing-standing-permission-build.log`, `target/directional-standing-check.log`.

## Alias sidebar separators and action alignment — 23 October 2026

Removed horizontal section dividers from the Aliases and Express Lanes Column B
organizer in A and B. New alias now uses the same padded full-width Label content
as the guide and sender-permissions buttons. Existing generous hit regions,
section order and individual alias entry points remain. Canonical build,
CsMailAppleCheck compilation, strict signing verification, original requirement
comparison and installation passed. Reopened the canonical bundle with the
existing account and inspected B’s light sidebar screenshot: section lines are
absent and the New alias action uses the same horizontal inset as the actions
above. Diff checks passed. No new automated tests or unrelated backend suites;
A, iOS, other section orders and dark/compact layouts were not individually
inspected. Build: `target/alias-sidebar-no-dividers-build.log`.

## Alias sidebar actions simplified — 23 October 2026

Removed Manage all aliases from the A/B Aliases and Express Lanes sidebar,
including its unused callback. New alias now sits directly beneath Permissions
for particular senders in either section-order arrangement. Individual aliases
retain their settings entry points. Canonical Apple build, CsMailAppleCheck
compilation, strict signing verification, original requirement comparison and
installation passed. Reopened the canonical app with the existing account and
verified B’s sidebar in native accessibility state and a light screenshot: no
Manage all aliases button, and New alias immediately follows sender permissions.
Diff checks passed. No new tests or unrelated backend suites were run for this
presentation-only change. A, iOS, alternate section order and dark/compact layouts
were not individually inspected. Build: `target/remove-manage-aliases-build.log`.

## Sender permissions open in main space — 23 October 2026

In both interfaces, Column B now contains a generous selected-state button for
Permissions for particular senders. Sender names, direction, sender search and
the express-lane receiving-address filter moved to a dedicated main-space route.
Standing permission and sender-scoped lanes have separate sections; outgoing
shows the available outgoing lane projection. Alias-scoped access remains under
My aliases and is no longer mixed into sender-lane records. The receiving-address
filter explicitly applies only to express lanes. Sidebar alias search and main
sender search have separate state; the new search uses the shared focus-dismissal
contract. Existing navigation checkpoints retain sender filters and return paths.
Removed the superseded alias-lane sidebar projection. No grant, account, domain
rule or communication authority changed.

Canonical build and CsMailAppleCheck compilation passed. macOS signing initially
waited for protected Keychain authorization; Marcus approved the signing request,
after which strict signature verification, original requirement comparison and
installation passed. The superseded build’s pending signing request was stopped.
Reopened the canonical app with the existing account, verified B sidebar/button,
main-space light composition, Daniel Reed under standing permission, contact
opening and Back to Permissions, and incoming/outgoing empty-lane views. Left on
incoming permissions. Diff checks passed. No new automated tests or unrelated
backend suites. Existing account has no sender-scoped lane records, so populated
lane selection/filter results were not verified at runtime; A, iOS, dark mode,
compact sizes and keyboard/accessibility interaction were not individually
rechecked. Build: `target/sender-permissions-main-space-build.log`.

## Alias guide wording correction — 23 October 2026

Removed the address/identity-proof sentence from the shared guide’s alias-scoped
lane result explanation. No permission or domain behavior changed. Canonical Apple
build, CsMailAppleCheck compilation, strict signing verification, original signing
requirement comparison and installation passed. Reopened the canonical bundle
with the existing account and inspected Interface B’s guide, including selecting
the alias-scoped branch: the result retains its first explanatory sentence and
omits the requested sentence. Native screenshot inspection and diff checks passed.
No new tests or unrelated backend suites were run for this text-only correction;
Interface A, iOS and other sizes were not individually rechecked. Build:
`target/alias-guide-wording-build.log`.

## Subdued terrazzo throughout main space — 23 October 2026

Interface B’s workspaceDetail composition now supplies a mainSpace texture role
through the SwiftUI environment. All nested default backdrops inherit it, including
the guide, inbox, Home, empty detail surfaces, Q&A and Notes. Main-column texture
uses one-quarter workspace opacity: 0.085 light and 0.01625 dark. The base fill
remains opaque. The surrounding workspace keeps 0.34/0.065; the sidebar keeps its
explicit subdued setting. Perimeters, texture anchoring/cache and opaque reading
interiors retain their current owners. Contrast and Reduce Transparency still
omit texture. Interface A is unchanged by this correction. The Swift principles
document records the shared main-space convention.

Canonical build (including CsMailAppleCheck compilation), strict signature
verification, original designated-requirement comparison, installation and
reopening passed. The existing account reopened without credential replacement.
Native B inspection confirmed visibly softer guide texture in light/dark and the
same reduced strength in Communications / All inboxes in dark, with stronger
surrounding workspace and existing sidebar/perimeter treatment. App left in B /
Aliases and Express Lanes / guide / light. No messages, drafts, aliases, permissions
or finances changed. Diff checks passed. No new tests or unrelated backend suites
were run. Other main-space screens inherit the rule but were not individually
inspected; iOS, alternate sizes/scales and runtime accessibility overrides were
not rechecked. Build: `target/main-space-subdued-terrazzo-build.log`.

## Guide reopening verified — 23 October 2026

At Marcus’s request, native inspection reconnected successfully. Closed the running
app normally and reopened the canonical installed bundle. The existing account
opened without credential replacement. Interface B / Aliases and Express Lanes
now visibly shows the new branching alias-policy diagram in light appearance;
its accessibility text includes both receiving policies, optional sender grants,
sender-scoped lanes and standing permission. The removed refunds section is
absent and the permission-only request rule is present. This resolves the earlier
reopening/visibility blockers for the wording and flow updates. The app is left
on the guide. No account permissions, aliases or messages changed. Individual
branch interactions, other sizes, A and iOS were not rechecked in this reopening.

## Alias policy and optional sender-grant flow — 23 October 2026

The shared guide’s opening example is now a top-down interactive flow:
Create alias → require a bonded request / alias-scoped express lane → optional
sender-specific permissions → no grants / selected senders → sender-scoped express
lane / standing permission. Drawn branching and joining connectors show both
receiving policies feeding the same optional grant decision. Selectable cards
show their selected state with an outline and checkmark and expose native button
and selected accessibility traits. The result explanation covers bonded access
without a grant, direct sender-lane/standing access, and broad alias admission
without a sender grant. Text explicitly permits different grants for different
senders, names alias scope, and preserves blocks and authentication requirements.
The former sender/alias/inbox illustration and its example pickers/toggle are
removed. No spinner or refund explanation is present. Selections remain local
educational state with no account/model authority calls.

Canonical Apple build (including CsMailAppleCheck compilation), strict signature
verification, original signing-requirement comparison and installation passed.
Diff/source checks passed. No new automated tests or unrelated backend suites.
Native app inspection again returned timeoutReached, continuing the previous
wording-update inspection blocker. The installed build is current; reopening and
visual verification of the running copy were not completed. No evidence is claimed
for rendered geometry, interactive runtime, compact layouts, keyboard/VoiceOver
behavior or iOS in this pass. No account, credential association, alias or grant
was changed. Build: `target/alias-policy-flow-build.log`.

## Alias guide keeps financial explanations separate — 23 October 2026

Removed the Requests and refunds section from the shared guide. Its sender-access
section now states that accepting a request can grant standing permission or an
express lane, and that later expiry/revocation does not reopen the original
request. The guide no longer discusses refunds, payment or settlement; financial
explanations remain separate. This is presentation text, with no domain change.

Canonical Apple build (including CsMailAppleCheck compilation), strict signature
verification, original signing-requirement comparison and installation passed.
Source inspection confirms the removed section and the new permission paragraph;
diff checks passed. No new automated tests or unrelated backend suites were run.
Native inspection repeatedly returned timeoutReached, including after reconnecting
the tool. Inventory reports cs-mail Development running, but reopening the updated
copy and visual verification were not completed. Do not treat the installed build
as evidence that the running process has loaded this change. No credentials,
account association or durable state were modified by this update. Neither
interface’s runtime layout nor iOS was rechecked. Build evidence:
`target/aliases-guide-permission-only-build.log`.

## Aliases and Express Lanes workspace and guide — 23 October 2026

Column A’s former Express lanes area is now Aliases and Express Lanes, with one
at-sign/badge icon. Interface A wraps the full rail label onto two lines. Column B
contains the guide action, Manage all aliases, New alias, all account aliases
(including bonded-request aliases), incoming standing permission and existing
incoming/outgoing sender express lanes. Existing section-order preference remains,
with renamed choices. Standing permission is separate from the express-lane
receiving-address filter; no alias scope is inferred from contact identity.

Alias management now belongs to this area; the Relationships duplicate entry is
removed. Communications retains its useful Manage aliases shortcut. Selecting an
alias opens the existing managed-address screen with policy and receiving
permission controls. The replaced lane-only alias-detail implementations are
removed. Selecting a standing-permission contact reaches existing correspondence
and Manage contact. Sender express-lane details retain their existing path. The
new guide destination participates in normal checkpoint/back navigation and alias
refresh. No domain grants or receiving policies changed.

`AliasesGuideView` supplies one shared product explanation, with fonts inherited
from each interface. Its interactive sender → alias → inbox diagram compares
bonded and address-wide admission, sender lanes and standing permission, and a
second bonded alias that does not inherit the first alias’s permission. It is
educational state only, not a client adapter, account mode or permission editor.
Text covers independent directory visibility, authentication, blocks, explicit
scope, refunds, lane/address lifecycle, disclosure consent and open product
contracts. No spinner is introduced.

Canonical Apple build (including CsMailAppleCheck compilation), strict signature
verification, original designated-requirement comparison, installation and
reopening passed. The original account reopened without credential replacement;
requested addresses remain listed. Native checks covered B in dark/light and A
in light at the existing 219.5-point sidebar width: new label/icon, populated alias
and standing sections, readable guide and diagram; B example choices for sender
lane, standing permission, open-alias admission without a sender grant, and the
other-alias refusal; bonded alias detail with policy and permission controls;
standing contact’s existing permission-management sheet opened/closed without
changes; guide action and checkpoint return navigation. Final installed build was
reopened after the A-label correction. The app is left in B / Aliases and Express
Lanes / guide / light. No messages, drafts, aliases, grants or financial state
changed. Normal correspondence-opening semantics apply.

Diff and removed-reference checks passed. No new automated tests or unrelated
Rust/PostgreSQL suites were run under the presentation scope. Actual alias creation,
grant mutation, outgoing/sender-lane runtime (no active sender lane in the current
list), alternate sizes/scales, compact diagram fallback, runtime accessibility
overrides, full VoiceOver audit and iOS were not exercised. This adds no public or
live-provider delivery evidence. Build: `target/aliases-lanes-workspace-build.log`.

## One reply action per native thread — 23 October 2026

Removed the per-message “Reply to message” buttons from native correspondence in
A and B. The existing thread-level Reply action remains with its current composer
and permission checks. No message-specific quotation/targeting behavior was added.
Canonical build, strict signature verification, original signing-requirement
comparison, installation and reopening passed with the existing account and no
credential replacement. Native B/dark inspection of Jules’s thread confirmed one
Reply button and no per-message reply actions, including both retained messages
in the accessibility tree. The app is left on the thread. No composer opened or
message sent. Diff checks passed; Apple build includes CsMailAppleCheck compilation.
No new automated tests or unrelated backend suites were run. A runtime, light
appearance, iOS and composer sending were not separately exercised. Build evidence:
`target/thread-reply-only-build.log`.

## No spinners without explicit owner request — 23 October 2026

Removed every remaining ProgressView from A and B: global busy/sign-in overlay,
initial member details, Directory overlays, approach-page loading, alias creation
and address generation, draft opening, composer source selection, express lanes
and mailbox pagination. Existing labeled states use quiet text; unlabeled busy
overlays are removed. Cancel sign-in remains available. The pagination trigger
uses an invisible one-point boundary with its existing task, preserving automatic
page loading independently of a visual indicator. The Swift principles document
now prohibits spinners anywhere unless Marcus explicitly requests a particular
circumstance. No exceptions are currently requested.

Canonical build, strict signature verification, original requirement comparison,
installation and reopening passed. The existing account reopened without
credential replacement. Source audit found no ProgressView, NSProgressIndicator,
UIActivityIndicator, spinner or explicit indeterminate animation implementation
in Apple Swift/Objective-C application sources. Native B/dark inspection covered
Home, populated Express lanes and Communications / All inboxes; the app is left
in All inboxes. No messages, drafts, aliases, permissions or finances changed.
Diff checks passed; Apple build includes CsMailAppleCheck compilation. No new
tests or unrelated backend suites were run. Each transient loading branch,
large-result pagination, sign-in cancellation, A runtime, light appearance and
iOS were not individually exercised in this pass. Build evidence:
`target/no-spinners-build.log`.

## Stable live mailbox search — 23 October 2026

MemberModel retains the last completed match projection and its query while the
next query runs. The first page for the current request generation replaces the
matches rather than appending to the previous query; subsequent pages append.
Existing generation/lifetime guards remain. Empty query, receiving-scope changes,
lock and explicit copy deletion discard result provenance with the matches. A and
B show the query associated with displayed results, and remove the per-keystroke
spinner that inserted/removes layout space. Completed empty results also remain
stable pending their replacement. Search failures remain visible; existing
pagination and expiry filtering remain.

Canonical build, strict signature verification, signing-requirement comparison,
installation and reopening passed with the existing account and no credential
replacement. Native B/dark checks typed J, Ju and Jules through empty and matching
results, confirmed no per-keystroke loading indicator, cleared directly back to
inbox, searched again and opened the correct Jules thread from the results. The
app is left there. Normal opening/displayed behavior applies; no message sent,
draft, alias, permission or financial state changed. Apple build includes
CsMailAppleCheck compilation; diff checks passed. No new tests or unrelated
backend suites were run. A uses the same model and corresponding view change but
was not separately inspected. Deliberately delayed/out-of-order responses,
large-result pagination, failures, expiry races, light appearance and iOS were not
runtime-tested in this pass. Build: `target/search-stable-results-build.log`.

## Shared search click-away behavior — 23 October 2026

All twelve search-field instances across A and B use `SearchFieldInteraction`:
mailbox, conversation find (including external email), alias-inbox filtering,
Directory, express lanes and financial records. Each retains its existing query
owner and interface styling. A field-scoped native boundary observes same-window
pointer-down events, clears that field’s focus only outside its bounds, ends native
editing and returns the event for normal dispatch. Boundaries do not intercept hit
testing; event monitors are removed with their views. No global text-field styling,
query reset or domain authority is introduced. The Swift principles document records
the applied interaction contract.

Canonical build, strict signature verification, original designated-requirement
comparison, installation and reopening passed. The existing account reopened
without credential replacement. Native checks covered B mailbox blank-space
click-away with Jules query/results retained, Command-F returning to search,
opening a result while focused, B conversation search retaining Marcus and its
match while the caret and strong border disappear on outside click, B Finances
click-away across the sidebar/main-space boundary with a filter retained, and A
Express lanes focus/dismissal. Temporary conversation/financial filters were
cleared; mailbox search was used for the retained result journey. No messages,
drafts, aliases, permission or financial state changed. Apple build includes
CsMailAppleCheck compilation; diff checks passed. No new automated tests or
unrelated Rust/PostgreSQL suites were run. Conditional alias search (>6 inboxes),
Directory and external-email search were wired and compiled but not individually
exercised. Light appearance, right/middle clicks, multi-window cases and runtime
accessibility overrides were not rechecked. iOS pointer/touch dismissal is not
implemented by the macOS event adapter. Build: `target/search-click-away-build.log`.

## Interface B neutral search focus — 23 October 2026

Conversation find and mailbox search now share `InterfaceBSearchFieldSurface`:
plain native text editing with an opaque neutral interior, eight-point corners
and a neutral outline (0.75 points at rest, 1.5 while focused). Focus increases
outline opacity rather than adding the native blue bezel. Increased Contrast
keeps the stronger width at rest. Existing focus bindings, search and submit
behavior remain with their current owners; Interface A is unchanged by this work.

Canonical build, signing, installation and reopening passed with the original
designated requirement preserved. The existing account reopened without credential
replacement. Native inspection confirmed a neutral focused border in mailbox
search in light and Jules’s conversation find in light/dark. Typing Marcus found
one match and highlighted the matching text; the query was cleared. The app is
left in Jules’s thread in dark with the empty field focused. No message, draft,
alias, permission or financial state changed. Apple build includes
CsMailAppleCheck compilation; diff checks passed. No new automated tests or
unrelated Rust/PostgreSQL suites were run. iOS, external-email find, runtime
accessibility overrides, other search fields and narrow windows were not
separately inspected. Build evidence: `target/interface-b-search-focus-build.log`.

## Interface B redundant contact status removed — 23 October 2026

Removed the “Contact from this person” line from the correspondence header at
Marcus’s request. Manage contact remains, and the request outcome stays in its
relationship-request section. Canonical build, signature verification and
signing-requirement preservation passed; installed app reopened with the existing
account and no credential replacement. Native light-appearance inspection of
Jules’s thread confirmed the line absent and request outcome retained. The app is
left there. No message sent or authority changed. Diff checks passed; Apple build
includes CsMailAppleCheck compilation. No new tests or unrelated backend suites.
Dark appearance, other screens and iOS were not rechecked. Build evidence:
`target/interface-b-remove-contact-status-build.log`.

## Interface B thread information hierarchy — 23 October 2026

Current incoming-contact permission now sits with Manage contact in the thread
header instead of floating above the back navigation. Request class and historical
outcome form a compact, lightly tinted relationship-request section. Each message
has one left-aligned sender/date/receiving-route group aligned with its readable
body; the timestamp no longer floats at the opposite edge. Current permission and
historical request outcome retain their separate supplied meanings. The portal no
longer owns a second permission-text input. Interface A is unchanged by this work.

Canonical build, signed installation and reopening passed with the original
signing requirement preserved. The existing account reopened without credential
replacement. Native inspection covered Jules Morgan’s retained thread from All
inboxes in dark and light appearance at the existing window and sidebar size:
request grouping, current permission placement and aligned message metadata were
visible and readable. The app is left on that thread in light. No message sent,
draft edited, permission, alias or financial state changed; opening follows normal
reading/displayed behavior. Apple build includes CsMailAppleCheck compilation;
diff checks passed. No new automated tests or unrelated Rust/PostgreSQL suites
were run. Other request outcomes, long metadata/narrow windows, external-email
screens, iOS and accessibility overrides were not separately inspected. Evidence:
`target/interface-b-thread-hierarchy-build.log`.

## Interface B consistent return navigation — 23 October 2026

All Interface B chevron-left return controls now use `InterfaceBBackButton`:
unboxed secondary text, a shared 13-point font, ten-point horizontal padding and
a minimum 44-point click height. This replaces the mixture of plain, native boxed,
region and decorated action styles across portal returns, contact conversations,
workspace returns, external email, Directory, alias management and composer source
selection. Interface A is unchanged by this correction.

The canonical app built, installed and reopened with its original signing
requirement preserved; the existing account opened without credential replacement.
Native dark-appearance inspection of Jules Morgan’s thread from All inboxes
confirmed matching unboxed outer and inner back controls. Clicking the inner
control returned to Jules’s conversations; clicking the outer control returned to
All inboxes. The thread was reopened and left visible. No message was sent or
permission, alias or financial state changed; opening uses the normal reading workflow.
Strict signature verification and diff checks passed. The Apple build compiled
CsMailAppleCheck. No new tests or unrelated Rust/PostgreSQL suites were run.
Light appearance, other return screens, narrow windows, iOS, keyboard focus and
accessibility overrides were not separately inspected. Build evidence:
`target/interface-b-back-navigation-build.log`.

## Interface B continuous bowed contour — 23 October 2026

The bowed surface reached the native divider mask at mid-height, which erased
part of the fine contour. Its background and contour now reserve three trailing
points, clearing that mask even at the narrower sidebar width. Native content
layout, splitter geometry, contour width/opacity and texture strength are unchanged.
No lighting or embossing is restored to column B.

Canonical app built, installed and reopened, UUID
`B1711E65-68D4-3C23-81D6-60F8B9196E5D`. Native inspection of Communications /
All inboxes at the current 236.5-point sidebar split confirmed the contour remains
continuous through its bowed midpoint in light/dark. The app is left there in light.
The existing account reopened without credential replacement. No messages, drafts,
permissions or aliases changed. Strict signature verification passed with the
original designated requirement. A-source comparison passed after normalizing
only the prior Finances heading rename; no A change in this pass. Diff checks
passed. Apple build includes CsMailAppleCheck compilation. No new automated tests
or unrelated Rust/PostgreSQL suites were run. Other sidebar/window widths, display
scales, iOS and runtime accessibility overrides were not rechecked. Build evidence:
`target/interface-b-continuous-bow-build.log`.

## Interface B legible bowed organizer outline — 23 October 2026

The bowed geometry survived the lighting transfer, but its pale textured fill
blended into the workspace and made the shape hard to read. Column B now has a
neutral 0.75-point contour, with ink opacity 0.18 in light and 0.24 in dark. The
existing high-contrast 1.5-point outline remains. No shadow, embossing or cove is
restored; quarter-strength terrazzo and main-space emphasis remain.

Canonical app built, installed and reopened, UUID
`7D751BDA-A97B-39CA-94A4-48CE73F92E0F`. Native inspection of Communications /
All inboxes in light/dark confirmed the bowed silhouette is visibly outlined,
column B remains unlit and the main perimeter retains its lighting. The app is
left there in light appearance. The existing account reopened without credential
replacement. No messages, drafts, permissions or aliases changed. Strict signature
verification passed with the original designated requirement. A-source comparison
passed after normalizing only the prior Finances heading rename; no A change in
this pass. Diff checks passed. Apple build includes CsMailAppleCheck compilation.
No new automated tests or unrelated Rust/PostgreSQL suites were run. Other window
sizes/scales, iOS, runtime accessibility overrides and interactive resizing were
not rechecked. Build evidence: `target/interface-b-sidebar-contour-build.log`.

## Interface B main-space visual hierarchy — 23 October 2026

Column B retains its bowed silhouette but has no embossed edge, shadow or cove
light. Its interior uses the cached, window-anchored terrazzo at one-quarter of
workspace opacity: light 0.085, dark 0.01625. The surrounding workspace remains at
0.34/0.065. `InterfaceBSidebarSurface` replaces the embossed sidebar surface; its
old cove implementation is removed.

The active main-space surface now receives the soft embossed shoulder and area
cove through `InterfaceBMainSpacePerimeter`. Normal destinations have a rounded
24-point perimeter and 20-point inset. Opening correspondence omits that outer
perimeter and lights the rounded reading container using the supplied route edge;
there is one illuminated content frame at a time. Reading interiors stay opaque.
Increased Contrast retains solid outlines and suppresses texture/cove; Reduce
Transparency suppresses texture/cove. Sidebar top/bottom gutters and native
splitter/divider masking remain.

Canonical app built, installed and reopened, UUID
`99EF9E4A-1FED-3CB5-BE4C-D296C235A0D3`. Native inspection covered Communications /
All inboxes in light, Jules's open thread in light/dark, and a Finances record in
dark: quiet faintly textured organizer, lit main perimeter, matching section hue,
opaque thread interior and no duplicate illuminated frame. The app is left in
Interface B / Finances / Daniel's record / light. No composer opened or message
sent; no permissions, aliases or financial state changed. The existing account
reopened without credential replacement. Opening the thread uses the normal
reading/displayed workflow. Strict signature verification passed with the original
designated requirement. A-source comparison passed after normalizing only its
previously requested Finances heading rename; no A code changed in this pass.
Diff checks passed. Apple build includes CsMailAppleCheck compilation. No new
automated tests or unrelated Rust/PostgreSQL suites were run. Other destinations,
permission-route shapes, narrow windows, other scales, iOS, runtime accessibility
overrides, interactive resizing and performance were not inspected. Build evidence:
`target/interface-b-main-space-hierarchy-build.log`.

## Finances visible-name rename — 23 October 2026

The visible Economics name is now Finances in both interfaces: shared navigation
title/help/accessibility name and each column B heading. B's preview title follows
the same shared name. Internal area identity, navigation, record order, icon,
palette, stored preferences and domain behavior remain unchanged. Interface A's
only screen-source change is the requested heading text.

Canonical app built, installed and reopened, UUID
`91193259-B063-35A9-887A-EBEEA959E415`. Native inspection confirmed B's Finances
navigation name/help, column B heading and full-title preview; Escape dismissed
it. The app is left in Interface B / Finances / light. The existing account reopened
without credential replacement. Strict signature verification passed with the
original designated requirement. Interface A source comparison passed after
normalizing only the intentional heading rename. Diff checks passed. Apple build
includes CsMailAppleCheck compilation. No new automated tests or unrelated
Rust/PostgreSQL suites were run. A runtime, dark appearance, other sizes/scales,
iOS and accessibility overrides were not rechecked. Build evidence:
`target/finances-label-build.log`.

## Interface B mini Reply and resting action rims — 23 October 2026

Reply now uses mini control size for both native and external email: a 26-point
visible height, 11-point text, eight-point horizontal padding and 13-point corners.
The outer click target remains 44 points high. Small action controls retain their
36-point treatment. Action rims are two points at rest and three while pressed;
prominent label weight no longer implies a permanently emphasized rim. Actions
omit the separate inset reflection line and use a six-point shadow with three-point
offset. Navigation selected rims, inner reflections and portal treatment remain.

Canonical app built, installed and reopened, UUID
`C9F5272F-F447-3E45-B2BC-CAF6132DEA73`. Native light-appearance inspection of
Jules's retained thread confirmed substantially smaller Reply, readable icon/label
and a single thin resting colored rim. The app is left on that thread in Interface
B, light. No composer opened or message sent, and no permissions or aliases changed.
The existing account reopened without credential replacement. Strict signature
verification passed with the original designated requirement. All 29 Interface A
source hashes and diff checks passed. Apple build includes CsMailAppleCheck
compilation. No new automated tests or unrelated Rust/PostgreSQL suites were run.
Dark appearance, press/hover timing, other action screens, external-email runtime,
iOS, other window sizes/scales and runtime accessibility overrides were not
rechecked. Build evidence: `target/interface-b-mini-reply-build.log`.

## Interface B action-button rim optics correction — 23 October 2026

The previous change reduced the drawn rim to two/three points but retained native
glass beneath action buttons, whose wider bevel still made Reply look heavy.
Action buttons now use the neutral opaque center already used by column A,
leaving the shared drawn two/three-point rim as their visible colored edge.
Explicit portal/navigation/action roles replace the former control/navigation
roles; native glass remains on Home portals. Compact Reply dimensions, inner
reflections and hover/press outlines are retained.

Canonical app built, installed and reopened, UUID
`0966757F-2B8E-3D3F-95C5-770EBE6703FC`. Native inspection of Jules's retained
thread confirmed the visibly thinner colored Reply edge and neutral center in
light/dark, alongside column A. The app is left on that thread in Interface B,
light. No composer was opened or message sent, and no permissions or aliases
changed. The existing account reopened without credential replacement. Strict
signature verification passed with the original designated requirement. All 29
Interface A source hashes and diff checks passed. Apple build includes
CsMailAppleCheck compilation. No new automated tests or unrelated Rust/PostgreSQL
suites were run. Other action screens, external-email runtime, iOS, other window
sizes/scales, runtime accessibility overrides and pointer timing were not
rechecked. Build evidence: `target/interface-b-action-rim-optics-build.log`.

## Interface B divider toolbar-cap repair — 23 October 2026

The divider mask previously covered only the detail content height, exposing the
native vertical separator through the toolbar above the restored column B top
gap. It now extends upward by the measured toolbar safe-area inset. The backdrop
remains aligned to the workspace texture and does not intercept pointer events;
native splitter geometry and resizing code are unchanged.

Canonical app built, installed and reopened, UUID
`69F1CF98-7588-39A9-B670-F7C50A3130FC`. Native inspection confirmed the top divider
segment is absent in Communications / All inboxes in light/dark, with the top gap
and cove light retained. The app is left there in light appearance. The existing
account reopened without credential replacement. No messages, drafts, aliases or
permissions were changed. Strict signature verification passed with the original
designated requirement. All 29 Interface A source hashes and diff checks passed.
Apple build includes CsMailAppleCheck compilation. No new automated tests or
unrelated Rust/PostgreSQL suites were run. Drag resizing, other window sizes/scales,
iOS and runtime accessibility overrides were not rechecked. Build evidence:
`target/interface-b-divider-cap-build.log`.

## Interface B compact Reply and unified thin rims — 23 October 2026

The shared decorated rim now uses two points normally and three when emphasized
or selected, across action buttons, navigation and preview rims. The former
role-specific rim width branch is removed; material roles still own interior
behavior. Inner reflections and hover/press widths are retained. Native and
external-email Reply actions request small control size: a 36-point visible
surface, 12-point horizontal padding, 18-point corners and a 44-point click target.
Other action buttons retain their size while adopting the thinner rim.

Canonical app built, installed and reopened, UUID
`EB985AC6-B5A2-3FEA-A44D-59BF5568AAB3`. Native light-appearance inspection of
Maya's retained thread confirmed the smaller Reply and thinner emphasized rim,
with readable label and existing adjacent explanation. The app is left in
Interface B / Maya thread from All inboxes / light. No composer was opened or
message sent, and no permissions or aliases changed. The existing account reopened
without credential replacement. Strict signature verification passed with the
original designated requirement. All 29 Interface A source hashes and diff checks
passed. Apple build includes CsMailAppleCheck compilation. No new automated tests
or unrelated Rust/PostgreSQL suites were run. Dark appearance, external-email
runtime, other window sizes/scales, iOS, runtime accessibility overrides and
pointer/press timing were not rechecked. Build evidence:
`target/interface-b-compact-reply-rims-build.log`.

## Interface B softly edged reading container — 23 October 2026

The opened correspondence view keeps its rounded 24-point container with an opaque
neutral interior, but removes the full rectangular canvas behind the view. Terrazzo
is visible around the container and behind return navigation. The former sharp
one-point boundary is replaced by a blurred six-point neutral gradient shoulder
and a soft shadow. Content clips to the rounded interior; the supplied route and
portal anchor remain the edge geometry owners. Increased Contrast retains a clear
solid route outline. No glass or area-colored cove is added to the reader.

Canonical app built, installed and reopened, UUID
`3AD1E2C8-E92C-3C12-9171-7E966AB1A179`. Native inspection of Maya's retained native
thread from All inboxes covered light/dark: square backing absent, terrazzo visible
outside the rounded surface, soft edge and readable opaque interior. The app is
left with that thread open in Interface B, light. No draft opened, message sent,
permission or alias changed; the existing account reopened without credential
replacement. Opening the thread uses the normal reading/displayed workflow.
Strict signature verification passed with the original designated requirement.
All 29 Interface A source hashes and diff checks passed. Apple build includes
CsMailAppleCheck compilation. No new automated tests or unrelated Rust/PostgreSQL
suites were run. Other window sizes/scales, iOS, runtime accessibility overrides,
other permission-route geometries and external-email-specific screens were not
inspected. Build evidence: `target/interface-b-soft-reading-surface-build.log`.

## Readable interface selector — 23 October 2026

The shared toolbar selector had compressed to arrows without visible selection
text, including at Home. Its 124-point fixed width now keeps the current interface
label readable. Existing Home/editor/busy switching guards remain unchanged.

Canonical app built, installed and reopened, UUID
`BFE4AF72-5D8C-31BB-AD92-E3BF9F283159`. Native inspection confirmed the visible
Interface B label at Home, both menu options, switching to A and back to B. The
app is left in Interface B / Home / light. The existing account remained open
without credential replacement. Strict signature verification passed with the
original designated requirement. All 29 Interface A screen-source hashes remain
unchanged; the fix is in the shared selector owner. Diff checks passed. Apple build
includes CsMailAppleCheck compilation. No new automated tests or unrelated
Rust/PostgreSQL suites were run. Narrow window widths, other scales, iOS and
runtime accessibility overrides were not rechecked. Build evidence:
`target/interface-selector-visible-build.log`.

## Interface B thinner navigation rims — 23 October 2026

Column A's main colored rim is now two points normally and three points when
selected. The navigation role carries this setting; previews and other B surface
rims retain their existing widths. Inner reflections, fine outlines and hover/press
lighting are unchanged.

Canonical app built, installed and reopened, UUID
`EEDDC438-E108-3CC2-A073-1287AD8F9F92`. Native light-appearance inspection confirmed
the selected Communications rim and unselected rail rims. The app remains in
Interface B / Communications / All inboxes, light. The existing account reopened
without credential replacement. Strict signature verification passed with the
original designated requirement. All 29 Interface A source hashes and diff checks
passed. Apple build includes CsMailAppleCheck compilation. No new automated tests
or unrelated Rust/PostgreSQL suites were run. Dark appearance, other sizes/scales,
iOS, runtime accessibility overrides and exact hover/press timing were not
rechecked. Build evidence: `target/interface-b-thinner-navigation-rims-build.log`.

## Interface B full-title preview headings — 23 October 2026

Column A previews now show the destination's full title in the 23-point heading
style, replacing the small title-plus-layout caption. The header exposes a heading
accessibility trait; the icon-only rail and preview opening/dismissal rules remain.

Canonical app built, installed and reopened, UUID
`7A283A47-4B7A-3129-B6D7-56036F6289AF`. Native light-appearance inspection confirmed
the full Communications heading fits, is larger than the controls and appears as
an accessibility heading. Right Arrow opening and Escape dismissal passed. The
app is left in Interface B / Communications / All inboxes, light, preview closed.
The existing account reopened without credential replacement. Strict signature
verification passed with the original designated requirement. All 29 Interface A
source hashes and diff checks passed. Apple build includes CsMailAppleCheck
compilation. No new automated tests or unrelated Rust/PostgreSQL suites were run.
Other preview titles, dark appearance, other sizes/scales, iOS, runtime accessibility
overrides and exact hover timing were not rechecked. Build evidence:
`target/interface-b-preview-headings-build.log`.

## Interface B smaller rail and restored top buffer — 23 October 2026

Column A buttons are now 48-point squares with centered 20-point icons, retaining
the rail width, lowered starting position, six-point spacing, names and tooltips.
Column B no longer extends through the toolbar. Eight-point vertical gutters
keep its bowed panel below the toolbar safe area and above the window bottom;
cove lighting follows its measured bounds. A workspace backdrop covers the native
gray toolbar cap exposed by restoring the top buffer.

Canonical app built, installed and reopened, UUID
`25C910A7-C123-3159-965D-80A2B4EEF856`. Native light-appearance inspection confirmed
all six smaller buttons, Communications selection, visible terrazzo top buffer,
neutral bowed panel and matching cove light. An initial inspection exposed the
gray native toolbar cap; the final installed build covers it with the backdrop.
The app is left in Interface B / Communications / All inboxes, light. The existing
account and aliases reopened without credential replacement. Strict signature
verification passed with the original designated requirement. All 29 Interface A
source hashes and diff checks passed. Apple build includes CsMailAppleCheck
compilation. No new automated tests or unrelated Rust/PostgreSQL suites were run.
Dark appearance, other window sizes/scales, compact/iOS layout, runtime accessibility
overrides and exact hover timing were not rechecked for this change. Build evidence:
`target/interface-b-smaller-rail-top-gap-build.log`.

## Interface B icon-only rail — 23 October 2026

The six column A navigation buttons now show centered icons at 24 points without
visible text labels. Accessible names, tooltips, full button bounds, colored rims
and navigation/preview interactions remain. Economics uses the banknote symbol
in B rather than the shared chart symbol; Interface A remains unchanged.

Canonical app built, installed and reopened, UUID
`CA0B0ADE-DC72-3E4C-8D1F-79873BBC9479`. Native inspection confirmed all six
label-free buttons, the banknote icon, and Economics selection without a preview.
Accessibility inspection confirmed all six names and help descriptions remain.
The app is left in Interface B / Economics / light. The existing account reopened
without credential replacement. Strict signature verification passed with the
original designated requirement. All 29 Interface A source hashes and diff checks
passed. Apple build includes CsMailAppleCheck compilation. No new automated tests
or unrelated Rust/PostgreSQL suites were run. Dark appearance, other sizes/scales,
iOS, runtime accessibility overrides and exact hover timing were not rechecked
for this change. Build evidence: `target/interface-b-icon-rail-build.log`.

## Interface B lower rail and visible cove light — 23 October 2026

The vertical rail starts 64 points lower, placing the Home button below the
Communications heading. Compact horizontal navigation is unchanged. Column B's
cove now has a wider reflected-color halo and a brighter near-edge spill. Its
canvas extends 20 points beyond the split-view drawing bounds, making the left
and bottom spill visible as well as the bowed right edge. The panel interior
remains neutral; accessibility contrast/transparency suppression remains.

Canonical app built, installed and reopened, UUID
`3CDF7663-A9D9-3C4B-8D90-B91D31EC21FD`. Native inspection confirmed Home starts
below Communications and the teal cove is visible along the bowed perimeter in
light/dark. The app is left in Interface B / Communications / All inboxes, light.
The existing account and aliases reopened without credential replacement. Strict
signature verification passed with the original designated requirement. All 29
Interface A source hashes are unchanged; diff checks passed. Apple build includes
CsMailAppleCheck compilation. No new automated tests or unrelated Rust/PostgreSQL
suites were run. Other window sizes/scales, compact/iOS layout, runtime
accessibility overrides and performance remain unvalidated. Build evidence:
`target/interface-b-rail-cove-build.log`.

## Interface B hover-only pointer previews — 23 October 2026

Column A click actions now navigate only; they no longer assign a pointer-preview
session. Hover opens a closed preview after 1,000 ms instead of 750 ms. Moving
between icons while a preview is already open retains the existing immediate
handover. Right Arrow/accessibility preview access and pointer-exit dismissal
remain available.

The canonical app is built, installed and reopened, UUID
`72035F28-92E3-32B2-960C-0BC34362331D`. Strict signature verification passed with
the original designated signing requirement. The existing account reopened with
its aliases, without credential replacement. Native inspection confirmed clicking
Relationships selected its workspace without opening a preview; the screen remains
in Interface B / Relationships / light. The one-second delay was checked in source;
precise pointer dwell timing was not measured. All 29 Interface A source hashes
remain unchanged. No unrelated Rust/PostgreSQL suites or new automated tests were
run. Build evidence: `target/interface-b-hover-only-build.log`.

## Interface B embossed sidebar and navigation focus — 23 October 2026

Column B retains its bowed silhouette with a neutral interior and narrow embossed
edge. A subtle cove light uses the open workspace's area color and is clipped to
the exterior of the outline. The previous native sidebar glass, broad optical
bands and reflections are removed. Increased Contrast uses a solid edge;
Increased Contrast and Reduce Transparency omit the cove light. Column A disables
the system focus effect, removing the blue rectangle after a click while retaining
its selected rim and keyboard focus/Right Arrow preview action.

The canonical app is built, installed and reopened, UUID
`F6204CBA-DA28-327E-B590-6EE7FFC11D76`. Strict signature verification passed in
system context with the original designated signing requirement preserved. The
existing account reopened without credential replacement. All 29 Interface A
source checksums remain unchanged. Native inspection covered Communications in
light/dark, Relationships in light, column A click selection without a blue
rectangle, and the Communications keyboard preview and Escape dismissal. The app
is left in Interface B / Relationships, light appearance. No message was sent,
no draft opened and no alias or permission changed.

Validation: Apple build including CsMailAppleCheck compilation, native screen
inspection, strict signature, A-source comparison and diff checks. No new automated
tests or unrelated Rust/PostgreSQL suites were run for this presentation change.
Other sizes/scales, iOS/older OS, runtime accessibility overrides, exact pointer
animation timing and performance measurements remain unvalidated. Evidence:
`target/interface-b-embossed-sidebar-build.log` and
`target/interface-b-embossed-sidebar-validation.json`.

## Interface B inky rim palette — 23 October 2026

The owner's darker palette is applied through the existing area-palette owner:
Home graphite, Communications petrol teal, Relationships dark bronze, Express
lanes midnight blue, Economics black olive and Notes aubergine. Colored rim bands
are stronger; midtone reflections and fine white glints preserve their glass-like
depth. Shared palettes coordinate rail, preview/sidebar rims and perimeter light.
Terrazzo texture/opacity and neutral interiors are retained.

The first native inspection showed dark rims reflecting into the system glass
and graying navigation-button interiors. Navigation material now uses a flat
opaque neutral center, with the existing polished rim/hover/press treatments;
native material remains on the other supported glass roles. This keeps color
on navigation edges and avoids tinted or darkened centers.

The canonical app is built, installed and reopened, UUID
`FD067125-9E6C-3B5E-BFFB-B1854C13F540`. Strict signature verification passed in
system context and the original designated signing requirement is preserved.
The existing account reopened with no credential replacement. All 29 Interface A
source checksums remain unchanged. Native inspection covered all six rail buttons
at Home in light/dark, a dark Notes preview and Escape dismissal, appearance
switches and the Communications sidebar in light. The app is left in Interface B
/ Communications / All inboxes, light appearance. No message was sent, no draft
opened and no alias or permission changed.

Validation: Apple build including CsMailAppleCheck compilation, native screen
inspection, strict signature, A-source comparison and diff checks. No new automated
tests or unrelated Rust/PostgreSQL suites were run. Other sizes/scales, iOS/older
OS, runtime accessibility overrides, exact pointer/press animation timing and
performance measurements remain unvalidated. Evidence:
`target/interface-b-inky-rims-build.log` and
`target/interface-b-inky-rims-validation.json`.

## Interface B varied, lower-density mineral texture — 23 October 2026

The owner found the fragments too uniformly sized and dense. The periodic fracture
field now thins its sites before forming cells, producing genuinely different
cell areas. A larger fraction of cells is omitted, and three inset size ranges
mix tiny flecks, medium fragments and occasional larger pieces. Split shards,
chipped edges, translucent aggregate and mineral mottling remain. Light/dark layer
opacities stay at 0.34/0.065; this changes size and coverage rather than hiding the
same dense pattern under a lower opacity. The fixed cache and window anchor remain.
It is still a procedural interpretation, not a pixel reproduction of the photo.

The canonical signed app is installed and reopened, UUID
`47C86058-7689-3A41-8079-574CDEA495BF`. Strict signature verification passed in
system context and the original designated signing requirement is preserved.
The existing account reopened without credential replacement. All 29 Interface A
source checksums remain unchanged. Native inspection covered Home in light/dark,
Relationships in light, a flat white Relationships hover box and a flat charcoal
Communications hover box, plus Escape dismissal and appearance switching.
The app is left in Interface B / Relationships / light appearance. No message was
sent, no draft was opened, and no alias or permission changed.

This installed build supersedes the pending mineral and flat-preview candidates
recorded below. Their source changes are included: preview interiors are opaque
and flat, with colored rims and no native glass gradient. No signing/install step
remains pending for the current revision. Intermediate staging bundles have been
removed by the canonical install or explicit cancellation cleanup.

Validation: Apple build including CsMailAppleCheck compilation, native inspection,
A-source comparison, strict signature and diff checks. No new automated tests or
unrelated Rust/PostgreSQL suites were run. Other window sizes/scales, iOS, older OS,
runtime accessibility overrides and cold-render/memory/power/frame-rate measurements
remain unvalidated. Evidence: `target/interface-b-sparse-mineral-build.log` and
`target/interface-b-sparse-mineral-validation.json`.

## Interface B flat navigation-preview interior — 23 October 2026 (signing pending)

The owner observed a blue gradient inside column A's hover box despite its neutral
tint. Its native glass fill is now replaced with an opaque, flat canvas (white in
light appearance, charcoal in dark), with the existing colored perimeter rim.
The unused preview role was removed from the generic native glass material.
Pointer hit regions, dismissal, controls and preview ownership are unchanged.

The canonical Apple compilation, including CsMailAppleCheck, passed. The combined
candidate also contains the previously requested mineral-texture refinement.
Installation is waiting at macOS codesign/Keychain approval in
`target/apple/.CsMail-build.R6cVDn/CsMail.app`; the open app still has the old preview
fill and the first mineral revision. The prior waiting build was cancelled and
its staging bundle removed, so it cannot overwrite this candidate. Complete
signing, installation, reopening and native light/dark inspection before claiming
this fill or the final mineral refinement available.

All 29 Interface A source checksums and diff checks passed. No new automated tests
or unrelated Rust/PostgreSQL suites were run. The changed preview's runtime
appearance, final texture, iOS/older OS, runtime accessibility settings and other
window sizes remain unvalidated. No message, draft, alias or permission changed.
Log: `target/interface-b-flat-preview-build.log`.

## Interface B mineral texture refinement — 23 October 2026 (signing pending)

The owner's request to approach the tile photograph replaces the independent
scattered polygons with periodic Voronoi fracture cells, split slivers and chipped
notches. Translucent white aggregate, graded chip interiors and three scales of
mineral mottling soften flat fills and cloud the matrix. The existing 1536-pixel
cache, fixed seed, window anchoring, accessibility branches and opacity values
(light 0.34, dark 0.065) remain. No texture timer or backend change is introduced.

Native inspection of the first installed revision (UUID
`087C91D4-9EF4-340A-B066-E258018A49C0`) at Home showed too much fitted-mosaic
structure. The final source reduces coverage, adds more split shards, makes more
aggregate near-white, varies inset/opacity and opens white space. That refinement
compiled with the canonical Apple build including CsMailAppleCheck, but is waiting
at codesign/Keychain approval in `target/apple/.CsMail-build.iX7MS5/CsMail.app`.
The running app is still the first mosaic-like revision, not evidence for the
final refined source. Complete signing, installation, reopening and light/dark
native inspection before reporting that refinement available.

Diff checks and all 29 Interface A source checksums passed. No new automated tests
or unrelated Rust/PostgreSQL suites were run. The final rendering, iOS, older OS,
runtime accessibility overrides, other display sizes/scales and cold-render/memory/
power performance remain unvalidated. No message was sent, draft opened, or alias
or permission changed. Build log: `target/interface-b-mineral-texture-build.log`.
The cancelled earlier staging bundle was removed before the current candidate.

## Interface B procedural terrazzo background — 23 October 2026

B now uses a procedural interpretation of the owner's Terra Mia tile reference:
pale irregular mineral chips, predominantly gray/white with faint beige and muted
blue-green, over the pure-white canvas. It is lower contrast than the reference.
One deterministic 1536-pixel transparent image is generated and cached per process;
backgrounds reuse it in a shared window coordinate space. Edge chips wrap to avoid
repeat seams. There is fine grain, no animation, timer or continuous regeneration.
The large raster repeat is not a unique infinite surface. Dark appearance uses a
much lower-opacity pattern over charcoal. Reduce Transparency and Increased
Contrast omit the decorative pattern; these branches compile but were not forced
on Marcus's system. Navigation/button/sidebar interiors remain neutral and their
area-colored rims remain; reading pages retain their opaque canvas. The previous
uniform backdrop implementation has been replaced, not retained as a second path.

The canonical app is installed and reopened, UUID
`05B73546-16C3-3E1A-9444-70C6076852BC`. Strict signature verification passed in
system context, with the original designated signing requirement preserved. The
existing account reopened without replacing its credential. All 29 Interface A
source checksums remain unchanged. Native inspection covered Home in light,
Relationships in light/dark, and Communications / All inboxes in light appearance.
The texture remained stationary across inspected navigation and text stayed
readable at this window size. Glass interiors and clipped sidebar corners retain
their previous treatment. The app is left at Interface B / Communications / All
inboxes, light appearance. No message was sent, no draft was opened and no alias
or permission was changed during this inspection.

Validation: canonical Apple build (including CsMailAppleCheck compilation), native
screen inspection, A-source comparison, strict signature and diff checks. No new
automated tests or unrelated Rust/PostgreSQL suites were run. Older OS and iOS
runtime, runtime accessibility overrides, other window sizes, display scales,
long-session memory/power/frame-rate, and every native workflow remain unvalidated.
The cached pixel allocation is about 9 MiB; no measured performance claim follows.
Evidence: `target/interface-b-terrazzo-build.log` and
`target/interface-b-terrazzo-validation.json`.

## Interface B white canvas and rim-only navigation color — 23 October 2026

The owner clarified that column A button interiors should also be neutral.
Navigation now has its own glass role: its base and native tint are neutral in
both appearances, and its older-platform material has no area wash. Area colors
remain on rims and perimeter hover light. Primary-action material retains its
existing role. The light canvas is now pure white (`#FFFFFF`), replacing off-white;
dark charcoal is unchanged. Column B and navigation-preview interiors remain
neutral, and the clipped sidebar still has no external rectangular shadow.

The canonical app is built, installed and reopened with the same account. UUID:
`73853B74-1D2E-3031-9182-EDBB97B72F86`. System-context strict signature verification
passed and the original designated signing requirement is preserved. All 29
Interface A source checksums remain unchanged. Native inspection covered all six
rail buttons at Home in light/dark, switching back to light, and Relationships
with its neutral sidebar and colored rim. The app is left in B / Relationships,
light appearance. No message, draft, alias or permission was changed.

Validation: Apple build including CsMailAppleCheck compilation, native inspection,
signature verification, A-source comparison and diff check. No new automated tests
or unrelated Rust/PostgreSQL suites were run. Older OS fallback, iOS, runtime
accessibility overrides, other window sizes and exact hover/press timing remain
unvalidated. Evidence: `target/interface-b-white-rim-only-build.log` and
`target/interface-b-white-rim-only-validation.json`.

## Interface B neutral interiors and clipped sidebar — 23 October 2026

The current installed B keeps color on column A buttons and on workspace rims.
Column B and navigation-preview interiors now use neutral material, without the
area-color fill or preview tint. The older-platform preview branch also omits its
area wash. Existing selected-row indicators remain neutral and visible.

This installation also completes the pending gray-surround correction below.
Covering the sidebar backing and replacing the composite shadow were insufficient:
native inspection still showed a square corner. The final implementation removes
the external sidebar shadow and clips all glass rendering to the bowed lens.
The bevel and inner reflections remain; no rectangular shadow plate is retained.
The previously waiting candidate was cancelled and its staging directory removed
before this combined build, so it cannot subsequently overwrite this installation.

The canonical app is installed and reopened, UUID
`AC6E58E6-881D-3324-926A-4B3B0691D00D`. System-context strict signature verification
passed, retaining the original designated signing requirement. The existing account
reopened with no credential replacement. All 29 Interface A source checksums match
the pre-B-glass baseline. Native inspection verified Communications in light/dark,
Relationships in light, a neutral Relationships preview in light and a neutral
Notes preview in dark. Their colored rims remain distinct. The exposed sidebar
corners no longer show the rectangular gray surround at this window size. Escape
preview dismissal and appearance changes were exercised. The app is left in
Interface B / Relationships, light appearance. No message was sent, no draft was
opened, and no alias or permission was changed.

The Apple build (including CsMailAppleCheck compilation), diff check, signature
check and A-source comparison passed. No new tests or unrelated Rust/PostgreSQL
suites were run. Other window sizes, iOS, older OS fallback, runtime accessibility
overrides and exact pointer/press animation timing remain outside this validation.
Evidence: `target/interface-b-neutral-interiors-build.log` and
`target/interface-b-neutral-interiors-validation.json`.

## Remaining Interface B gray surround — 23 October 2026

The owner still observed the gray rectangle after the neutral-canvas change.
That earlier inspection did not establish complete removal. A follow-up replacing
the composite shadow with a lens-only blur also retained a square corner in native
inspection. The current source removes the external shadow and clips the entire
glass composite to the curved lens, preserving its bevel and inner reflections.

The source compiles with the canonical Apple build, including CsMailAppleCheck.
Installation is currently waiting at macOS codesign/Keychain approval; the final
clipped candidate has not yet been reopened or visually verified. The running app
is the intermediate lens-shadow build, not evidence for the final correction.
Complete signing, installation and native light/dark corner inspection before
reporting the rectangle resolved. Interface A's 29 source checksums are unchanged;
diff check passed. No new tests or unrelated backend suites were run. iOS, older
OS, runtime accessibility overrides and other window sizes remain unvalidated.
Build log: `target/interface-b-sidebar-shadow-build.log`.

## Interface B neutral canvas and sidebar backing — 23 October 2026

The owner found the ivory too warm. B's light canvas is now neutral off-white
`#F8F8F8` (equal RGB channels), replacing `#F8F8F6`. Dark charcoal is unchanged.
The system sidebar's rectangular backing is covered with the same canvas behind
the curved glass lens. The lens, area tint, bevel, soft shadow and native splitter
remain; a separate gray rectangular surround serves no member purpose.

Canonical build and installation passed. UUID:
`55A4773F-7A95-36D7-8D68-ED421450574E`. Strict signature verification passed in
system context, retaining the original designated signing requirement. The app
reopened with the existing account. Native inspection covered Relationships in
light and dark appearance and Communications in light appearance. The canvas and
backing match at the inspected window size; glass edges and labels remain visible.
The app is left at Interface B / Communications / All inboxes, light appearance.
No message, draft, alias or permission was changed. All 29 Interface A source
checksums are unchanged.

Validation was the Apple build (including CsMailAppleCheck compilation), native
screen inspection, signature and A-source checks, and diff check. No new tests or
unrelated Rust/PostgreSQL suites were run. Other window sizes, iOS, older OS and
runtime accessibility overrides remain unvalidated. Evidence:
`target/interface-b-neutral-canvas-build.log` and
`target/interface-b-neutral-canvas-validation.json`.

## Interface B workspace palettes — 23 October 2026

B now has a uniform, very slightly warm ivory canvas (`#F8F8F6`) in light
appearance and charcoal (`#1F2020`) in dark appearance. This supersedes the
geometric teal/gold background and sidebar color field described below.
`InterfaceBEditorialAreaPalette` owns six presentation palettes: Home pearl,
Communications teal, Relationships champagne, Express lanes slate blue, Economics
olive and Notes lavender. Each column A button and its navigation preview use
that button's palette; column B uses the current workspace's palette. Glass
shapes, broad sidebar bevel, generous click regions and perimeter hover feedback
are retained. No new sidebar is added to Home or Notes.

Native dark tint adaptation initially bleached the lavender button. The material
now uses an explicit dark tint and refreshes its native glass identity on an
appearance change. Final native inspection verified light-to-dark and dark-to-light
switches with all six rail buttons readable. Inspection across this revision also
covered Relationships and Notes previews, an Express lanes preview over Economics
in dark appearance, Relationships/Communications sidebars in both appearances,
the slate-blue Express lanes sidebar, and lower Communications alias/Drafts labels.
Preview palettes followed their own icon while another workspace remained open;
Escape dismissed them. Hovered conversation rows retained perimeter-only light.
Precise rail hover/press animation timing and pointer preview exit were not
separately measured; their implementation is retained.

The final canonical signed build is installed and running, UUID
`0058BCD4-1262-3B40-8CDB-A42AE5528B69`. Strict signature verification passed in the
system context, with the original designated signing requirement preserved. The
original account reopened without replacing its credential. All 29 Interface A
source checksums match the pre-B-glass baseline. The app is left in Interface B,
Communications / All inboxes, light appearance. No message was sent, no draft was
opened and no alias or permission was changed during this revision's inspection.

The Apple build (including CsMailAppleCheck compilation), diff check and A-source
checksum check passed. No new automated tests or unrelated Rust/PostgreSQL suites
were run for this presentation change. iOS, older OS fallback, runtime accessibility
overrides, other window sizes, frame-rate/power and every workflow remain outside
this validation. Evidence: `target/interface-b-area-palettes-build.log` and
`target/interface-b-area-palettes-validation.json`.

## Interface B sidebar lens revision — 23 October 2026

The first decorated B sidebar remained too frosted and rectangular compared with
the supplied reference. Column B now uses a dedicated clear native-glass lens,
a bowed right edge, a broad nine-point teal/gold bevel and separate fine inner
reflections. A stationary color field sits behind the native glass. This replaces
the sidebar's regular frosted plate and old rectangular cove outline; navigation
previews retain their separate regular-glass surface. It adapts the reference's
material appearance while retaining a functional sidebar, rather than reproducing
the reference's large circular boundary or changing its navigation layout.

The material is decorative background only: sidebar contents, click regions,
native splitter and eight-point bottom gap retain their existing layout. The
opaque accessibility branches are retained; older supported systems use ultra-thin
system material. No custom-refraction shader or Interface C is introduced.

The canonical signed build is installed and running, UUID
`3578D4FF-1E00-34CE-B5B7-7CE5CC8527B6`. System-context strict signature verification
passed and the original designated signing requirement is preserved. The original
account reopened with no credential replacement. Interface A's 29 source files
still match the pre-B-glass checksums.

Native inspection covered Communications in light/dark appearance, scrolling to
all receiving inboxes and Drafts, selecting NYC-Hospital and returning All inboxes,
and Relationships with the revised material. Labels and controls remained readable
inside the bevel at the inspected window size. The app is left at Interface B,
Communications / All inboxes, light appearance. No message was sent, no draft was
opened and no alias or permission was changed during this revision's inspection.

The native build (including CsMailAppleCheck compilation), diff checks and A-source
checksum check passed. No new tests or unrelated Rust/PostgreSQL suites were run.
iOS, older OS fallback, runtime accessibility overrides, resizing at other window
sizes, power/frame-rate measurements and every workflow remain unvalidated.
Evidence: `target/interface-b-sidebar-lens-build.log` and
`target/interface-b-sidebar-lens-validation.json`.

## Interface B: decorated native glass — 23 October 2026

Interface B now has decorated native Liquid Glass for its navigation buttons,
Home portals, sidebar/navigation previews and styled primary actions. The system
material is surrounded by polished teal/gold rim bands, inner reflections and
stationary shadows. A static teal/gold color field supplies the Home/workspace
environment. The navigation layout and opaque conversation-reading surfaces are
retained. Interface A's 29 screen/component source files match the pre-change
checksums; switching back to A was also inspected in the running app.

B deliberately does not enable native interactive flexing, pointer-following
reflections or idle animations. Hover feedback lights the perimeter without
shading the portal interior. Press scaling is finite (1–1.5 percent), and disabled
by Reduce Motion. Reduce Transparency and Increased Contrast select opaque
surfaces and solid borders; the backdrop also loses its translucent color field.
The supported older OS range uses regular system material where native glass
is unavailable. These accessibility/older-platform branches compile and were
reviewed, but were not forced on Marcus's system for visual validation.

The first inspection exposed blurred rail labels inside a native glass container.
The container was removed; each material stays behind its control label, and the
corrected/final build was inspected with sharp navigation text. No superseded
container implementation remains. Interface C's custom refraction is a separately
requested future experiment; it has no placeholder selector entry or shader yet.

The canonical signed app is installed and running with UUID
`A1667823-0A81-3DFE-BA60-67B64B5486BB`. Strict signature verification passed in the
system context; the original designated signing requirement and Keychain/account
association are preserved. The app reopened on the same account in Interface B.
All five active aliases, including all four requested addresses, were inspected.

Native inspection covered B's light/dark Home glass, sharp rail labels, navigation,
a Relationships preview and Escape dismissal, sidebar and opaque reading surfaces,
the existing popup composer and decorated Send control, A/B switching and B
selection/account reopening. An existing Daniel draft was opened, then saved and
closed with its text unchanged; this refreshed its saved timestamp. No message
was sent and no alias or permission was changed. The app is left at Home in
Interface B, light appearance.

The native build (including CsMailAppleCheck compilation), A-source checksum and
diff checks passed. Logs: `target/interface-b-decorated-glass-final-build.log`,
`target/interface-b-glass-source-check.log`; evidence:
`target/interface-b-glass-a-baseline.json` and
`target/interface-b-decorated-glass-validation.json`. Earlier build logs are
repair evidence, not the final installation record. Superseded app bundles were
removed by the canonical build script.

No new automated tests or unrelated Rust/PostgreSQL suites were run. iOS runtime,
older OS material, runtime Reduce Motion/Transparency/Increased Contrast settings,
a full window-size/accessibility matrix, frame-rate/power measurements and every
native workflow remain unvalidated. Press-scale/reduction behavior was reviewed in
code; no frame-by-frame press capture was made. Pointer preview exit behavior is
retained but was not separately exercised in this pass. No public/live-provider
claim follows from this local presentation work.

## Organize menu contrast — 23 October 2026

The per-conversation Organize menu in interfaces A and B now supplies an explicit
appearance-aware label and chevron, uses the macOS borderless menu style, and keeps
its intrinsic width at the right of the row. This replaces the expanding default
native menu whose text could render white on the light canvas. Existing menu
actions, confirmation, authority and storage paths are unchanged.

The canonical signed app is installed and reopened on the original account,
UUID `1C342186-3AC6-3CAF-9A7F-9480C5C24E3C`. System-context strict signature
verification passed and the designated signing requirement matches the original.
No credential was replaced. Native inspection verified readable labels on the
previously affected Jules and Elena rows, scrolling to the final Maya row in A,
light/dark appearance in both interfaces, and readable active/inactive window
states in A. Jules's A menu and Elena's B menu opened with Mark read, Mark unread,
Flag, Archive and Move to Trash. No organization action was executed during
inspection. The app is left in Interface A, light appearance, All inboxes.

The native build and diff checks passed; the A/B mailbox sources match after type
name normalization. No new tests or unrelated Rust/PostgreSQL suites were run.
iOS, other window sizes, a full accessibility matrix and execution of the existing
organization mutations were not revalidated. Build log:
`target/organize-contrast-build.log`; validation:
`target/organize-contrast-validation.json`.

## Selectable interfaces A and B — 23 October 2026

The canonical installed member application now offers an Interface A / Interface B
selector in the toolbar. Both begin with the same screens and visual components,
with independent source trees for future design work. One shared MemberView host
owns the existing MemberModel/client session and account. No backend, database,
credential, generated binding or client workflow was duplicated. Shared appearance
and layout preferences initially apply to both interfaces. Additional interfaces
can be registered in MemberView's MemberInterface enum.

Screen trees: `Sources/CsMailApple/Interfaces/A/` and `B/`.
Visual components: `Sources/CsMailPresentation/Interfaces/A/` and `B/`.
These remain source groups in the existing Swift modules. Shared identifiers,
view data, editing state and platform services retain single owners. See
`Sources/CsMailApple/Interfaces/README.md` for development boundaries.

Switching is available at Home (Main space) when no editor/dialog or busy operation
is open. Elsewhere the disabled selector explains how to switch. Existing unsaved
approach-page guards apply before returning Home. Switching recreates transient
navigation at Home, ends the old reader, preserves the single unlocked client and
saved work, and remembers the selection after quit/reopen. Inactive trees are not
mounted and do not run background tasks.

The native build passed, including compilation of CsMailAppleCheck. All 29 A/B
source pairs matched after normalizing only concrete visual type names. The signed
app is installed at `target/apple/CsMail-continuity-20261012.app`, UUID
`E6CACD08-88BB-3999-BFB8-9ECE52460979`. System-context strict signature verification
passed; the designated signing requirement matches the original. The first sandbox
signing attempt could not access the identity and left the installation untouched.
The system-context retry used the original identity and completed installation.
Temporary failed build bundles were removed.

Running-app inspection verified A-to-B and B-to-A switching, the matching Home
presentation, disabled switching in Communications, the shared inbox, the same three
existing saved drafts in both interfaces, all five existing active aliases, and
B selection/account reopening after quit. All four requested addresses remain
present. No draft or alias was created, edited or deleted during inspection; no
Keychain credential was replaced. The app is left running at Home in Interface A,
with the selector available.

Evidence: `target/interface-selector-build.log`,
`target/interface-selector-source-equivalence.json`,
`target/interface-selector-final-check.log` and
`target/interface-selector-validation.json`. No new automated tests were added and
unrelated Rust/PostgreSQL suites were not run for this presentation change.
iOS runtime, dark appearance, a full accessibility/window-size matrix, every native
workflow, injected interruption during switching and public/live-provider delivery
were not revalidated. Existing edit guards were retained; unsaved-edit loss was
prevented structurally by limiting switching to Home rather than challenged by a
new automated experiment.

## Superseded build state removed — 22 October 2026

Principle 11 now requires removal of superseded build state. Obsolete compiler
caches, copied executables, app bundles, generated bindings and source snapshots
were removed without creating another build archive. `target` decreased from
approximately 92 GiB to approximately 173 MiB. The next build will regenerate
its compiler outputs. Audit: `target/build-cleanup-20261022.json`.

The canonical app remains `target/apple/CsMail-continuity-20261012.app`, UUID
`C89FEC96-BC24-3B06-BDBB-A835671F33C9`. Its executable and Info.plist, the running
backend at `target/unified-aliases-owner-b-20261021/cs-maild`, and protected account
configuration/key files are unchanged by checksum. The existing app and backend
processes remained running; refreshing Communications completed successfully and
the existing aliases remained visible. The current device database passed a
read-only integrity check. No Keychain entry was written or replaced.

Durable development state still resides under `target/apple-local-state/` and
`target/apple-continuity-state-20261012/`; signing material remains under
`target/apple-signing/`. Preserve these paths and the current deployment during
future cleanup. Database/conversion evidence, validation records and the small
encrypted removed-design writing store were retained. Do not treat all of
`target` as disposable. Earlier dated sections record historical deployments;
references to superseded binaries, app bundles and source backups are not an
inventory of retained files.

No runtime source or build settings changed in this cleanup. No rebuild or
unrelated Rust/PostgreSQL suites were run; validation covered file preservation,
device-database integrity and a live app refresh, not every application workflow.

## Native column divider line removed — 21 October 2026

The native macOS split-view divider is covered with the canvas before the custom
perimeter and spill are drawn. This changes its visible line only: the native
resize hit region, measured column bounds and eight-point bottom gutter remain.

The canonical signed app is installed and running on the original account,
UUID `C89FEC96-BC24-3B06-BDBB-A835671F33C9`. Strict signature verification passed
and the designated signing requirement matches the original. Running-app
inspection confirmed the extra line is absent at the top, alongside the column
and below its rounded bottom edge, while the custom rim and eight-point gutter
remain. Jules’s conversation was reopened afterward. Build and diff checks passed;
no new tests or unrelated Rust/PostgreSQL suites were run. Dark appearance,
other areas/window sizes, resizing interaction and iOS were not rechecked.
Build log: `target/column-b-divider-20261021.log`.

## Eight-point bottom gap restored — 21 October 2026

At Marcus’s explicit direction, column B’s bottom gap is restored to exactly
8 points. The canvas beneath that gutter remains; the renderer’s lighting falloff
settings are unchanged. This supersedes the 24-point clearance described below.

The canonical signed app is installed and reopened on the original account,
UUID `74338BA3-E653-342F-8C59-313784AC357B`. Strict signature verification and
original designated signing requirement checks passed. Running-app visual
inspection confirmed the smaller gap in Communications, and Jules’s previously
open conversation was restored. Build and diff checks passed. No new tests or
unrelated Rust/PostgreSQL suites were run for this presentation change. Dark
appearance, other areas/window sizes and iOS were not rechecked.
Build log: `target/column-b-eight-point-20261021.log`.

## Column B bottom gutter — 21 October 2026

The initial eight-point gutter below did not preserve the full bottom lighting
falloff. The correction uses `EditorialCoveLight.spillExtent` (24 points), shared
with the renderer, for bottom clearance. The gutter uses the app canvas beneath
the glow, replacing the exposed native sidebar background strip.

The final canonical signed app is installed and running with the original account
and designated signing requirement, UUID `CFEF10C0-65A1-3D70-9DC7-7DCC441A625F`.
Strict signature verification passed. Running-app visual inspection confirmed
bottom spill for Relationships in light and dark appearance and Communications
in light appearance. Light appearance was restored. The build and diff checks
passed; no new automated tests or unrelated Rust/PostgreSQL suites were run.
iOS, other window sizes and the remaining area/appearance combinations were not
inspected. Evidence: `target/column-b-bottom-spill-20261021.log` and
`target/column-b-bottom-spill-20261021.json`.

The following records the superseded first gutter attempt.

Column B now reserves eight points below its content, glass surface and measured
perimeter. Content is clipped inside the column before its glass background is
applied; the top toolbar cap and existing perimeter lighting remain in place.

The canonical signed app is installed and reopened on the original account with
UUID `1CCCED80-6BDD-39F8-8F0B-4F57BEB1F0E2`. Strict signature verification passed
and the designated signing requirement matches the original. Running-app visual
inspection confirmed the bottom gutter in Relationships and Communications,
scrolling to the end of the inbox list within the perimeter, the retained Manage
aliases action, and successful navigation to My aliases. The five active aliases,
including Aetna 2026, remain present; the deleted old Aetna alias remains absent.
Build evidence: `target/column-b-bottom-gutter-20261021.log`.
Validation record: `target/column-b-bottom-gutter-20261021.json`.
No automated tests were added and unrelated Rust/PostgreSQL suites were not run
for this presentation change. iOS, other window sizes and dark appearance were
not inspected in this pass.

## Readable alias previews — 21 October 2026

The final signed app is installed and running with UUID
`37AEFFA6-5A4F-399E-BEEC-8B9DFF328D1F`. Signature verification passed and the
designated signing requirement matches the original. It reopened the original
account, displayed the final shorter name prompt, and generated a fresh preview.
Build log: `target/readable-alias-apple-final.log`; validation record:
`target/readable-alias-validation-20261021.json`.

Generated addresses now combine a member-approved name fragment (2–8 lowercase
Latin letters) with six random digits, including leading zeroes. The local part is
8–14 characters. Name input is explicit and independent of purpose, identity
records and receiving policy. The member can regenerate or choose a preferred
fragment. The same controls support alias replacement.

Rust owns generation in `crates/cs-mail-application/src/aliases/generation.rs`.
Reserved addresses inform prefix occupancy and candidate rejection; the soft
crowding threshold is 10,000 per prefix/domain and selection is bounded at 32
attempts. Preview is authenticated but reserves nothing. Creation signs the exact
reviewed address; atomic uniqueness resolves competing claims. A known collision
requires another reviewed preview, while an uncertain result retains the original
operation for recovery. Existing aliases are never regenerated or renamed.

The owner backend remains on port 9693 with the same database, keys and account.
Its pinned binary and checksum are in `target/unified-aliases-owner-b-20261021/`;
the previous binary is `cs-maild.before-readable-generation`. The current Aqua
session launchd job is `com.csqd.cs-mail.owner-readable-aliases`, invoking the
existing `start.sh`; logs are `server-readable-aliases.log` and
`server-readable-aliases.err`. Check that job before starting another server.
At Marcus’s subsequent explicit request, the unused old Aetna Insurance alias
was completely deleted, including its address reservation and two command
receipts, without creating a backup. A transaction rechecked every other public
table for dependencies before deleting that exact retired development fixture.
This is an explicitly authorized development-data exception, not a change to
normal retirement semantics. The four requested addresses and the newer
`thoma973837@c-sqdmail.com` (Aetna 2026) remain active. Running-app inspection
confirmed that the old entry is absent and the newer Aetna alias remains.
No Keychain credential was written or replaced.

Validation adapted existing experiments without adding automated test entry cases:
required name fragments, bounded collision exhaustion, crowded-prefix selection,
non-reserving previews, concurrent claims, and exact retry/reopen recovery.
Workspace all-target checking and strict all-feature lint passed, as did four
PostgreSQL directory tests, six portable-client runtime tests, six affected SMTP
integration checks, and the full native SQLite/FFI journey. Native checks create
the exact preview and exercise replacement. Evidence is in
`target/readable-alias-{check-final,clippy-next,directory,client-runtime,smtp,native-check}.log`.
Running-owner inspection verified regeneration, fragment selection, initials,
preview invalidation, creation readiness under both policies, and draft retention
across closing/reopening; no owner alias was created during inspection.
The full quality gate, iOS and public/live-provider delivery were not run for this
change. These local checks do not establish public email delivery.

## Main-space return navigation — 21 October 2026

The return button now occupies a fixed layout row above the detail viewport.
Scrolling content is clipped to that viewport, so titles and controls cannot draw
over the return action or enter the window-toolbar region. This replaces the
transparent safe-area inset; navigation history and return actions are unchanged.

The canonical signed app is installed and running with its original signing
requirement and account association; build/installed UUID
`EB8995BF-B4C3-3E03-A271-23B33B1A91B5`. Running-app inspection reproduced the overlap,
then verified scrolling in both directions and successful return navigation after
the fix. All five current owner aliases, including the newly created Aetna Insurance
alias, remain present. Evidence: `target/navigation-scroll-build-20261021.log` and
`target/navigation-scroll-verified-20261021.json`. No automated tests were added;
Rust/PostgreSQL suites were not rerun for this presentation change. iOS was not
inspected. Generated address length and readable format remain under discussion;
the generator and all existing addresses are unchanged.

## Unified aliases — 21 October 2026

The Rust model, signed commands, PostgreSQL schema 44, portable client and unified
Apple application now use one alias type. Address choice, owner-visible purpose,
receiving policy and directory visibility are independent. Creation requires an
explicit choice between Bonded and an alias-scoped express lane. Each bonded alias
owns its profile and request classes. First publication requires an explicit
visibility choice. Multiple bonded aliases are supported.

Sender lanes and standing permission apply to named receiving aliases. Explicit
renewal retains that selection; creating another alias never expands it. Request
terms freeze the recipient alias and the sender's selected return alias. Choosing
a correspondence address supplies no receiving authority. Runtime readers accept
only the current signed and encrypted representations; there is no legacy fallback.
Policy switching, alias fees, privacy-preserving correspondent references and held
mail remain deferred. Local simulation does not establish public email delivery.

The preservation-checked owner deployment is
`target/unified-aliases-owner-b-20261021/`, started by its `start.sh` on port 9693.
The previous `target/required-inboxes-owner-20261019/` database is an untouched
checkpoint. Backend and device conversion rehearsed rollback and exact retry,
archived original evidence, and retained the original account association and
Keychain entry without credential writes. Device/app checkpoints are under
`target/unified-aliases-device-20261021/`. Rollback must account for activity since
cutover and pair the backend, device store and signed app.

All four requested addresses are retained: `mtallis62831@c-sqdmail.com`,
`marcus.thomas@c-sqdmail.com`, `mat7771@c-sqdmail.com` (NYC-Hospital), and
`m11a33t@c-sqdmail.com` (Brooklyn-Lab-Co). The existing publication remains attached
to its exact `mtallis62831` receiving address; it was not copied onto the distinct
`marcus.thomas` alias. Existing sender permissions retain their explicit scopes.

The final corrected signed build (UUID `90D3C886-B1CB-3BCD-B85D-7B749B4AAE2B`)
is installed at the canonical path and has reopened with the original account.
Signature verification passed and its designated signing requirement matches the
original app. Running-app inspection verified all four retained aliases, successful
live refresh, both receiving-policy choices, independent generated/chosen address
selection, the preserved original profile/class, and the separate bonded alias's
empty profile/classes with an explicit first visibility choice. No extra owner
alias was created and no publication was changed during inspection. The alias list
is left open. Evidence: `target/alias-build-install-status.json`,
`target/alias-validation-final.json` and `target/alias-apple-build-installed.log`.

The full native SQLite/FFI journey passed in `target/alias-native-check-h.log`,
including creation's atomic lane grant, per-alias publication, explicit quote/payment
review, request/reply routes, source selection, lane renewal, multiple receiving
inboxes, scoped unread/search, retirement, blocking and same-device reopening.
Strict all-feature and production lint rechecks passed in
`target/alias-clippy-verified.log` and `target/alias-production-clippy-verified.log`.
The final owner backend is pinned and running (`server-final.log`); its predecessor
binary is retained alongside it. No new automated test cases were added. PostgreSQL
suites and workspace tests passed in `target/alias-quality-gate-verified.log`.
That gate run recorded the earlier lint failure for the expanded publication
experiment; both subsequent strict lint rechecks passed after the correction.
It must not be presented as a single clean gate exit.
iOS, public/live-provider email delivery, and the optional native external-email
journey were not validated in this run; Rust external/SMTP integration is covered
by the local database suites. Do not treat the historical results below as evidence
for this cutover.
See the [domain model](../../DOMAIN_MODEL.md#aliases-and-express-lane-scope).

## Conversation find — 19 October 2026

Native and external readers keep messages visible, highlight literal
case-insensitive text matches, distinguish the current occurrence and provide
previous/next controls with a count. Navigation wraps. Matching includes displayed
quotations and shared copies, respects the receiving-inbox scope and never follows
references. Clear restores the unhighlighted text. Find state is local presentation
state; encrypted storage and mailbox indexing are unchanged.

The canonical build passed and is installed and running with the original signing
requirement; build/installed UUID `69C62FD7-A6D6-3FB7-9060-EE719A948F1B`. Focused experiments
using the actual matcher and find model passed Unicode range fidelity, formatted
excerpts, scope removal, occurrence counts, wrapping and clear/restart. Evidence is
in `target/conversation-find-20261019/`. Rust/database suites were not rerun for this
presentation change. iOS and live external-reader inspection are not established.
Running-app inspection verified all-match highlighting, the distinct current match,
counts, navigation within/across messages, scrolling, wrapping in both directions,
no-match controls and clearing. Inspection exposed and corrected native scrolling
that initially changed the current match without bringing it into view. Daniel's
conversation is left open with find cleared.

## Required receiving inboxes — 19 October 2026

Every received native or external message now carries a checked receiving-inbox
assignment. Rust delivery effects keep that assignment with the message and its
content; checked restoration and database constraints preserve the binding.
Receiving history is derived from those records. The aggregate mailbox derives
received correspondence from alias inboxes, with sent copies represented separately.
The Inbox screen excludes sent-only conversations.

Ordinary native sends and initial requests sign the destination's public alias.
Explicit alias sends retain their selected destination. External conversations
record their receiving alias at creation, including for later token replies.
Alias selection establishes a destination; existing permission rules still decide
whether delivery is allowed. Development assignments cannot grant alias permission.

Marcus approved assigning the existing received placeholders to
`marcus.thomas@c-sqdmail.com`. The preservation-checked cutover records that decision
explicitly, archives original evidence and verifies authenticated retained content,
rollback and exact retry. The original account, Keychain entry, addresses, message
content and admission history are preserved. The signed helper read the existing
credential once and made no Keychain writes.

The current owner service/config is `target/required-inboxes-owner-20261019/`,
started by its `start.sh` at the existing 9693 endpoint. The previous
`target/subject-search-owner-20261019/` database remains an untouched checkpoint.
The previous signed app and encrypted device backup are under
`target/required-inboxes-device-owner-20261019/`. Use the new owner copy for subsequent
work; rollback must pair the app, device store and backend, and must account for
any activity committed since cutover.

The canonical signed app is installed and running on the original account;
built/installed UUID `82036424-B0FD-3AE1-95A9-479FF3904405` and the original signing
requirement match (`target/required-inboxes-20261019/apple-final.log`,
`running-app.json`). Running-app checks verified all five received placeholders in
the public inbox, an empty private inbox, reading and device-scoped body search
with the service stopped, and successful refresh after restoring the owner service.
The final presentation also loads current receiving metadata when connected,
without requiring a successful cached projection first. The public inbox is left open.

The complete native SQLite/FFI journey passed, including native and external
delivery, mixed-alias membership, aggregate/alias message-ID agreement, scoped
search, read state, retirement history and reopening (`native-complete.log`).
All eight local quality-gate checks passed, including PostgreSQL integration
suites (`quality-complete.log`); logs are in `target/required-inboxes-20261019/`.
Existing experiments were extended; no automated test cases were added. Earlier
attempts exposed missing fixture inboxes, a fixture-domain mismatch and the cached
projection ordering issue; their logs remain separate. This work does not establish
iOS behavior, real-provider delivery, real power-loss recovery or new performance
measurements. Cloud indexing and WASM remain deferred.

The earlier checkpoints below retain their original validation and deployment
evidence; they do not identify the current server or app build.

## Incremental encrypted search storage — 19 October 2026

Retained copies and per-message index records are independently encrypted SQLite
rows. One transaction commits the affected copy, index record and progress marker;
only committed changes enter the session index. A failed or uncertain commit
invalidates that cached index so the next operation reloads durable state. Rebuilding
updates index records without rewriting copies. The replaced whole-mailbox reader
and writer are removed; the 64 MiB limit now applies per record.

Unlock builds the in-memory word lookup from index records without opening message
copies. Queries select candidate IDs directly; snippets and readers retrieve selected
copies. Result pages load as they come into view. Known expiry/removal, query/scope
binding and reader-return context remain enforced. Downloads release the index lock
before network waits, and cannot repopulate a session cleared while they were waiting.

Validation extended existing experiments; no automated test cases were added.
Seven local quality-gate checks passed; PostgreSQL suites were explicitly skipped
because no server contract changed (`target/incremental-search-20261019/quality-ready.log`).
Final focused client and lint checks passed (`client-installed.log`, `lint-installed.log`).
The native SQLite/FFI journey passed transaction-failure rollback, stale revisions,
aggregate storage beyond 64 MiB, native/external indexing, alias search and session
clearing (`native-ready.log`). Earlier native attempts exposed assumptions about
scan cursors and cleared alias sessions; those logs are retained separately.

The optimized synthetic experiment reached 100,000 records: selective warm search
about 0.04 ms, no-match search about 0.004 ms and cold index opening about 0.9 s,
with no message-body reads during cold opening. The largest checkpoint in that
fixture stayed 2,438 bytes (`scale-release.log`). These are component measurements
using a memory-backed store and simple 1 KiB text records, not SQLite or full-app
latencies. Cold opening and memory use still grow with the index; phrase checks
may retrieve word-matching candidates. Images and realistic mixed-mailbox workloads
were not benchmarked. Other account/receiving-metadata snapshots retain their bounds.

The canonical signed app is installed and running on the original account; the
built/installed UUID is `8F765FCA-0B91-3C09-94CB-BA9B25CAA346`, with the original
signing requirement preserved (`apple-complete.log`). Running-app checks with the
service offline verified subject/body search over the ten retained copies, reading,
return to results, lock/unlock and saved receiving-inbox scope. Search layout was
visually inspected. The existing owner service was restored at 9693 and All inboxes
left open. SwiftPM products now explicitly relink the external Rust archive.

The one-time conversion verified authenticated copies, exact text, account binding,
rollback and exact retry. Account cache, drafts and personal-state ciphertext were
unchanged by conversion; Keychain was never written. The helper made one credential
read using the original app signing requirement. The former app and device backup
are in `target/incremental-search-owner-20261019/`; conversion evidence is
`owner-upgrade.log`. Backend schema, keys and addresses were unchanged.

Not covered: iOS, real power loss, a full appearance/accessibility matrix or public
provider delivery. Cloud indexing and WASM remain separate design discussions.

## Earlier subjects and offline-search checkpoint — 19 October 2026

New native conversations and initial requests accept a shared subject, fixed for
both participants. Blank input receives `Default subject: October-19-2026, 3:42 PM`
using the sender's local date/time. Replies inherit it. Subjects are encrypted;
conversation lists, readers and device-local search display them alongside the
correspondent's name. Existing development placeholders received date/time subjects.

Rust retains authenticated copies and a rebuildable word index in one encrypted
SQLite record. After unlock, search and readers use local copies without the
service. Connected background downloads extend coverage in bounded batches.
Results distinguish not-downloaded, unavailable and remaining content; known expiry
and removal apply before matches or snippets appear. Saved receiving evidence
restores alias inboxes offline. Lock clears transient results.

Validation: all eight local quality-gate checks passed, including PostgreSQL
(`target/search-subjects-20261019/quality.log`). Extended existing experiments
challenged subject substitution, retry, offline reopening, expiry/removal, failed
index checkpoints and plaintext leakage; no automated test cases were added.
The complete native SQLite/FFI journey passed, including shared subjects through
replies and reopening, subject search, caller-only removal and retained-store
transaction failure (`native-final.log` in the same directory).

The canonical signed app is installed and running on the preserved owner account.
Built/installed executable UUIDs match `F0004EA1-A501-3F53-85E4-D1767F3B0288`;
the original designated signing requirement is retained. Running-app checks with
the service stopped verified quit/reopen, lock/unlock, saved alias inboxes, subject
search over all ten retained messages, local reading and return to search results.
Screenshots checked subject/correspondent rows and the reader heading. The service
was restored and All inboxes left open (`apple-ready.log` records the final build).

The pinned owner service is `target/subject-search-owner-20261019/start.sh`, using
`cs_mail_subject_search_owner_20261019` at the unchanged 9693 endpoint. The verified
rehearsal copy was adopted without a second credential conversion. The schema-41
source remains offline at `cs_mail_decoupled_owner_20261016`; original device backup
and encrypted upgrade artifacts remain in
`target/subject-search-preservation-check-20261019/`. Rollback, exact retry, original
evidence archival and unrelated table/key fingerprints passed. The original
Keychain entry, account association and four requested addresses were preserved.

Limits: the initial word/phrase index has a 64 MiB encrypted-record bound. Large
mailboxes and the proposed FTS5/SQLCipher comparison remain unevaluated; this does
not settle the long-term engine choice. iOS, graphical subject creation, real power
loss, a full appearance/accessibility matrix and public/live-provider delivery
were not validated. External mail does not gain native shared subjects.

## Communications heading — 16 October 2026

The aggregate Inbox main-space heading is “All inboxes”, without the redundant
“· Inbox”. Other mailbox/address headings keep their scope. The canonical signed
build was installed and reopened on the preserved account, and the running
accessibility tree confirmed the heading. Build and diff checks passed; no new
tests, unrelated Rust/database checks or screenshots were used. Build log:
`/tmp/cs-mail-all-inboxes-heading-build.log`.

## Navigation glass surfaces — 16 October 2026

Column B and the Column A hover box use native regular-material frosting beneath
their original background colors at 82% opacity. Column B retains its canvas
color; the hover box retains its original area-specific background and inverted
appearance for text and controls. No extra area wash is applied to Column B.
Reduce Transparency restores each original opaque background. Interior gradients
remain absent; perimeter palette values and lighting rendering are unchanged.
The canonical signed app was built, installed and reopened on the preserved
account with Relationships restored for owner assessment. Build and diff checks
passed. No screenshot capture was used, so rendered glass/contrast, Reduce
Transparency, dark and compact/iOS layouts were not visually checked.
No automated tests or unrelated Rust/database checks were run. Build log:
`/tmp/cs-mail-original-glass-colors-build.log`.

## Relationships sidebar — 16 October 2026

Relationships' Column B contains a Your relationships link above the Manage links,
returning to the main relationships page through the existing guarded area
navigation. This resets return history, so selecting the root does not add a
redundant Back button there. Management pages retain their return controls.
It omits the people search,
permission groups or contact list. Their unused grouping preference and search
checkpoint state were removed. Selecting Relationships in Column A still opens
the main overview, whose Map/List layout control remains available. The canonical
signed build was installed and reopened on the preserved account, and the running
accessibility tree confirmed the sidebar links and return from My aliases to the
main relationships page without a Back button, including repeated root selection.
No screenshot capture or unrelated Rust/database tests were used. Build log:
`/tmp/cs-mail-relationships-root-navigation-build.log`.

## Column A label — 16 October 2026

The “cs” label and its header spacing were removed from Column A. The canonical
signed app was built, installed and reopened on the preserved account; its
accessibility tree confirms the rail starts with Home. No screenshot was taken,
to avoid reintroducing the system capture indicator. Build log:
`/tmp/cs-mail-remove-rail-mark-build.log`. No automated tests or unrelated
Rust/database checks were run for this presentation change.

## Column A divider spacing — 16 October 2026

Column B and the Column A layout preview retain the reduced gap from
the Column A divider, with a four-point rightward adjustment to clear the Mac
title-bar control area. The shared spacer is 10.75 points, alongside the native
sidebar's existing eight-point gutter. The signed canonical build was installed
and reopened on the preserved account; screenshot inspection confirmed both edges
remain aligned and clear the visible purple system sharing indicator. The ordinary
traffic-light buttons were not visually verified: macOS displayed its sharing
indicator in their place during inspection. Strict signature verification passed and
built/installed UUIDs match `28B5F797-62DF-3419-BD0E-831EC76BD1E6`.
Build log: `/tmp/cs-mail-window-clearance-build.log`. Validation was the native build,
changed-screen inspection and diff checks; no new tests or unrelated Rust/database
suites were run. Compact/iOS and dark layouts were not separately exercised.

## Column A transient layout preview — 16 October 2026

Selecting a Column A icon navigates without pinning its layout preview. Pointer
previews remain only while the pointer is over the icon or popup; interacting with
the popup does not pin it. The redundant Close button and pinned session state
were removed. Right Arrow still opens keyboard controls and Escape dismisses them.
Initial pointer dwell is now 750 milliseconds.
The signed canonical app was rebuilt, installed and reopened on the preserved
account for this timing change. Build log:
`/tmp/cs-mail-hover-750ms-build.log`. Build and diff checks passed; exact pointer
timing was not measured, and no screenshot capture or new tests were used.

The canonical signed app was built, installed and reopened on the preserved owner
account. Strict signature verification passed, the original signing requirement
was retained, and built/installed UUIDs match
`E0446B58-9A03-318F-89A7-3966C29A7029`. Running-app checks confirmed navigation
without a pinned box, clicking an icon dismisses its keyboard-opened preview,
Right Arrow opens controls without an X, and Escape dismisses them. Screenshot
inspection checked the simplified popup. Build log:
`/tmp/cs-mail-hover-unpin-build.log`. No automated tests were added or Rust/database
suites run for this presentation change. Pointer transit timing, iOS and compact/dark
layouts were not separately exercised.

## Decoupled receiving permission — 16 October 2026

Private alias creation now establishes an address without lane authority. The
separately validated `ReceivingPermission` has a distinct `PermissionRevision`.
Signed v3 management supports opening, restricting and removing a broad lane and
atomic replacement with selected signed sender grants. Restrictions freeze exact
admitted conversations; removal and sender replacement do not grandfather them.
Address lifecycle remains separate. Ordinary relationship permission remains an
independently checked authority for its explicitly named receiving scope.

Sender-restricted receiving uses existing `SenderScopedLane` validity and allowance
rules. Native evidence binds the known sender; SMTP binds verified author-domain
evidence to selected legacy-domain scopes while preserving the actual mailbox
identity and its blocks. Chosen grant state, receiving evidence, message copies,
allowance and receipts commit under the existing receipt-order barrier. Historical
retry never reactivates a superseded permission, and private refusal has no bonded
fallback. Swift separates permission from address status and groups lanes by their
actual scope. New aliases start unpaired; the existing two private owner routes keep
their recorded broad authority through explicit preservation conversion.

Validation: the full eight-check local quality gate passed, including all 106
PostgreSQL checks (`/tmp/cs-mail-decouple-quality-final.log`). The focused real-SMTP
experiment also passed preparation/removal fencing, authenticated domain mismatch,
allowance and exact redelivery (`/tmp/cs-mail-decouple-smtp-final.log`); targeted strict
lint passed (`/tmp/cs-mail-decouple-stale-preparation-lint.log`). The complete native
SQLite/FFI/isolated-Keychain journey passed, including unpaired creation, restricted
existing versus new conversations, removal and native sender replacement, read/search
scope, retained-copy removal, request/reuse/recovery and external consent
(`/tmp/cs-mail-decouple-native-complete.log`). Existing experiments were extended;
no automated test cases were added. Earlier attempts retain their logs: one had a
superseded replacement-lane expectation; another launched a production-only helper
after a shared build overwrote it. Final native validation pins both helper binaries
under `target/decoupled-native-tools-20261016/` and uses separate databases on
10073–10075. The isolated test services were stopped after the checks.

The current owner server is pinned at `target/decoupled-owner-20261016/start.sh`,
serving the unchanged 9693 endpoint against `cs_mail_decoupled_owner_20261016`.
The schema-40 source `cs_mail_alias_assessment_owner_20261016` remains offline and
untouched as the rollback checkpoint. Rehearsal, complete rollback and exact rerun
checks passed. All 111 unrelated table/sequence fingerprints and configured key
files matched. Original address records, command receipts and route evidence were
archived verbatim before explicit conversion; no runtime predecessor reader was
added. Owner account/persona/registered keys and all four requested addresses were
preserved. The original encrypted device files were backed up and verified unchanged
before reopening; the maintenance tool never accesses Keychain.

The final signed build is installed at the canonical app path and running on the
preserved owner account. Built and installed executable UUIDs match
`F2C3D959-DCB9-3F18-9716-49DD22F081B2`; strict signature verification passed and the
original designated requirement is retained. Running-app inspection verified both
private alias lanes, separate address and permission controls, restrict/remove
confirmation meanings, sender replacement contacts/domains/expiry/daily limit,
and the new-alias explanation that creation grants no lane. Changed controls were
visually inspected; owner permission confirmations were cancelled. Brooklyn-Lab-Co
inbox was restored after inspection. Build log:
`/tmp/cs-mail-decouple-apple-ready.log`.

Not covered: permission mutations on the actual owner account (covered in isolated
native fixtures), iOS, a full accessibility or
appearance/layout matrix, public/live-provider delivery, held-mail release,
outbound alias selection, real power loss or large-mailbox performance.

## Address and lane management boundary

Rust separates address lifecycle from independently revisioned receiving permission.
Private creation grants no lane. Explicit signed actions open, restrict or remove
an alias-scoped lane, or atomically replace access with selected signed sender
grants. The existing alias transaction commits permission and its exact-operation
receipt together. Native and SMTP admission recheck current permission; domain
matching uses authenticated evidence while correspondence and blocks retain the
actual sender identity. Swift groups lanes by permission and exposes pairing
controls. The schema-41 cutover preserves original evidence and the owner account
in an offline checked copy, with no runtime predecessor reader.

## Popup close focus appearance — 16 October 2026

The shared Close button disables its system focus effect, removing the blue
perimeter around its enlarged target while retaining keyboard focus, accessibility
and Escape. The canonical signed build installed and reopened on the original
account; strict verification passed and built/installed UUIDs match
`58B6EA72-CF46-33CF-B4DA-D855BC48CFA6`. In the running Comms popup, keyboard traversal
focused Close and screenshot inspection confirmed no blue outline. An offset click
inside the 44-point target still dismissed it. Brooklyn-Lab-Co/Inbox was restored.
Build log: `/tmp/cs-mail-popup-close-focus-build.log`. Diff checks pass. Validation
was limited to the native build and changed macOS control; no new tests or
unrelated Rust/PostgreSQL suites were run, and iOS/dark/compact layouts were not
separately exercised.

## Popup close target — 16 October 2026

The shared layout popup Close button now places its small xmark inside a
44 × 44-point label with an explicit rectangular content shape. The clickable
space belongs to the button, rather than padding outside it. Header insets retain
the icon's position and compact header. All six area popups use this one control;
the accessible Close name and Escape shortcut remain.

The canonical build installed with its original signing requirement, passed strict
signature verification, and matches the built executable UUID
`4F0BCB6C-3FF1-3817-8D1A-3414EDBAAB6B`. After quit/reopen through the original
account, the Comms popup dismissed when clicked 15 points right and below the
glyph center, inside the enlarged target and outside the former target. Screenshot
inspection confirmed the click location and retained inbox. Build log:
`/tmp/cs-mail-popup-close-target-build.log`. Diff checks pass. No new tests or
unrelated Rust/PostgreSQL suites were run; iOS, compact/dark layouts and every
popup instance were not separately exercised for this shared presentation change.

## All-area layout controls — 16 October 2026

Every column-A popup now contains presentation controls: Home cards/list within
the existing permission sections; Comms receiving-inbox/mailbox section order;
Relationships grouping and map/list; Express lanes private-alias/sender section
order; Economics newest/oldest/name record order; Notes reading width for its
current informational page. Notes creation/saving remains unimplemented. Choices
are non-sensitive device-local AppStorage values; no domain facts or authorities
are changed. All icons now share configuration pinning, Escape/Close/outside-click
dismissal and preservation of current detail when clicking the selected area.

After owner signing approval, the canonical build installed with the original
designated signing requirement. Strict signature verification passed; built and
installed executable UUIDs match `D35BC58E-4365-335A-9786-2620C874098D`.
The prior process was quit and the canonical app reopened through its existing
account/Keychain association, without enrollment or credential replacement.

Running macOS inspection verified all six popup control sets. Home changed to a
single-column list while retaining permission sections; Comms reordered sections
while retaining Brooklyn-Lab-Co/Inbox; Express lanes reordered while keeping the
Brooklyn alias detail open; Economics changed newest/oldest/name order while
retaining Daniel's financial record. Notes focused width visibly changed the
current page. Relationships retained Name/List. Escape dismissal and the accessible
preview action were also exercised. A second quit/reopen verified Home List,
Comms Mailboxes first, lanes People first, Economics Name and Notes Focused.
The app is left in Comms/All inboxes with its layout popup open for assessment.

Build log: `/tmp/cs-mail-all-layout-controls-build.log`; pre-pass source snapshots:
`target/all-layout-controls-baseline-20261016/`. Diff checks pass. No new tests or
unrelated Rust/PostgreSQL suites were run. This presentation pass covered the
Apple build, signature/executable checks and affected running screens; iOS,
compact/dark layouts, large datasets, exact hover timing, every keyboard traversal
and every filtered navigation permutation remain unexercised.

## Relationships layout controls — 16 October 2026

The Relationships column-A popup now changes organization rather than repeating
the workspace description. People in column B can be grouped by Permission,
ordered by Name, or ordered by Recent correspondence. The main overview switches
between Map and List using the same existing relationship summaries. Clicking
Relationships opens its workspace and pins the controls; clicking it while already
there preserves the current detail and search. Escape, Close and outside content
click dismiss the controls. Other area popups remain orientation aids in this pass.

The two non-sensitive layout preferences persist locally in AppStorage. They do
not alter permissions or member records and are not reverted by history Back.
Recent ordering uses available message/request timestamps with stable name and
identity tie-breakers. No message content or search query is added to preferences.

Validation: canonical Apple build, strict signature verification, matching built
and installed executable UUID `ACF9C70A-47A4-3337-9913-155C000F2DDC`, diff checks
and running macOS inspection. All three organizer choices and Map/List changed
immediately. With Daniel Reed open and a Daniel search active, changing both
preferences retained the detail and query. Escape dismissed the popup. Quit/reopen
through the original account retained Name/List; the app is left there with the
popup open and the assessment search cleared. The original signing requirement
was preserved; no enrollment or credential replacement occurred.

Evidence: `/tmp/cs-mail-relationship-layout-build.log`; pre-pass source snapshots:
`target/relationship-layout-baseline-20261016/`. No new tests or unrelated
Rust/PostgreSQL suites were run. iOS, compact/dark layouts, keyboard traversal of
every control, large datasets and all navigation permutations were not exercised
in this presentation-only pass.

## Workspace-linked interior top tint — 16 October 2026

Every actual column-B organizer now uses one background wrapper, including Comms.
A neutral, opaque canvas prevents the native sidebar surface from supplying an
unrelated hue beneath the subtle tint. The existing EditorialColumnTopTint uses
the same EditorialAreaPalette as the perimeter: turquoise Comms, dark-gold
Relationships, silver-blue Express lanes and olive Economics. Its 120-point
height, gradient stops and appearance-dependent opacity are unchanged. The middle
and lower background are neutral. Hover previews keep their separate contrasting
surface; navigation and domain behavior are unchanged.

The canonical build installed with its original signing requirement; strict
signature verification passed. Installed/built executable UUIDs match
`C6B6F143-2F8F-3692-B80A-39A135DC78A3`. After quit/reopen on the original account,
running macOS screenshots verified all four top tints in light appearance and
Comms in dark appearance. The gradient follows the workspace palette and fades to
the neutral column interior. Light appearance was restored, and the app was left
in NYC-Hospital/Inbox, matching its pre-install receiving selection. No enrollment,
credential, permission, message or financial changes were performed.

Validation: canonical Apple build, signature/executable checks, diff checks and
changed-screen inspection. Evidence: `/tmp/cs-mail-column-area-tint-build.log`;
pre-pass source snapshots: `target/column-area-tint-baseline-20261016/`. No new
tests or unrelated Rust/PostgreSQL suites were run. iOS, compact layouts and every
dark workspace were not visually exercised in this presentation-only pass.

## Shared return navigation and captured context — 16 October 2026

The Brooklyn-Lab-Co inbox → Manage permissions route recorded an origin but the
Express lanes detail exposed no history return action. The main-space owner now
supplies a labeled Back action whenever history exists and the rendered detail
has no existing equivalent. This covers lane/financial details, Directory, the
approach editor, alias management, nested mailbox views and unavailable native
details. Native correspondence and external-email readers retain their established
return controls. Alias management retains its separate collection-return action;
the origin-return control now comes from the shared main-space owner.

Navigation checkpoints capture a MailboxNavigation context (folder, query, sort,
bulk selection and stable list/search anchors) plus relationship/financial/lane
filters and alias-management selection. Back restores the captured values, not
whatever happened to remain in the mutable workspace after an intervening route.
Existing receiving scope, selected person/thread/message and the protected-edit
Save/Discard/Stay guard remain part of the return path. Explicit Inbox/mailbox
selection continues to start the intended fresh context. No Rust domain, cache,
credential, message, permission or financial contract changed.

Validation challenged the actual reported route before the repair, then the
installed fix: Brooklyn-Lab-Co/Inbox → Manage permissions → Back restored that
inbox. A stronger nested journey began in Brooklyn-Lab-Co/Unread with A–Z enabled
and a search query, opened permissions, used View inbox (which intentionally reset
the mailbox), then returned twice. Unread, query and sort were restored exactly.
Permissions → Manage alias → Back restored the selected Brooklyn lane and its
receiving filter; the next Back restored Brooklyn-Lab-Co/Inbox. No alias mutation,
message send, charge or permanent deletion was performed. The app was left in
Brooklyn-Lab-Co/Inbox with the assessment query cleared.

The canonical build installed with the unchanged original signing requirement,
passed strict signature verification, and matches the built executable UUID
`91A769EC-BC4D-358D-9F81-A00C4E831C5B`. The app was quit/reopened through its original
account/Keychain association without enrollment or credential replacement.
Evidence: `/tmp/cs-mail-navigation-return-build.log`; source snapshots are under
`target/navigation-return-baseline-20261016/`.

Validation scope: native build, signature/executable checks, diff checks and the
running macOS journeys above. No new tests or unrelated Rust/PostgreSQL suites
were run. iOS, dark/compact layouts, long-list scroll/selection restoration,
unavailable-reader states and every possible navigation permutation were not
manually exercised. These checks establish the local navigation repair, not public
mail delivery.

## Stable workspace organizers — 16 October 2026

Column A selects a workspace; its contrasting hover/focus previews now explain
that workspace briefly, without duplicating its lists or management controls.
Column B remains persistent and owns organization within each workspace. The
approved colors, continuous spill and absent sidebar hide control remain.

Comms separates receiving inbox scope from mailbox selection. Inbox, Unread,
Flagged, Archive, Trash, Sent and Outbox are selected in column B; the main-space
mailbox picker is removed. Sent/Outbox remain All-inboxes views, as before. Drafts
is available in the organizer. Mailbox selection uses the existing navigation
owner and scoped Rust projections. Explicit mailbox changes clear search/selection;
opening correspondence preserves the originating mailbox/search return context.
No unread counts or route membership are inferred from sender addresses.

Relationships has a searchable, unique correspondent list, with pending incoming
decisions and existing Rust permission projections grouped separately. Permission
subtitles explicitly describe contact from that person, not outgoing authority.
Outgoing-only/unclassified correspondents have their own section. Directory,
My aliases and My approach page form a separate management section. That organizer
stays present while aliases are listed and managed in the main space.

Express lanes retains distinct private-alias and sender-lane groups, adds search,
and keeps direction/receiving-address filters without resetting the opened detail.
Economics organizes existing request records by direction and issuance month,
with name/class search and a record count. It displays Rust-supplied financial
outcomes and does not derive charges, refunds or financial totals from text.
The superseded multi-area sidebar-list renderer and projection are removed.

This is a Swift presentation change. Saved searches, additional financial
categories/totals/distributions and Notes organization are not implemented by this
pass. Existing backend permissions, recovery contracts, encrypted caches and
original account/Keychain association are unchanged.

The canonical build installed after owner signing-key approval, retaining the
original designated signing requirement. Strict signature verification passed;
installed and built executable UUIDs match
`F9C8ADBE-4E49-390F-AE2E-DFB1726EB797`. The prior app was quit and the canonical
app reopened through the original account/Keychain association, without enrollment
or credential replacement. All three requested receiving aliases and the existing
correspondence remained available.

Running macOS inspection verified the concise Relationships preview, Comms Sent
selection and correspondence round trip, a Daniel relationship search retained
through correspondence/back and alias management, and the main-space alias list
while the same relationship organizer remained visible. Economics direction and
month filters changed the record list while preserving the opened Jules record;
Express lanes direction switching likewise preserved Elena's incoming detail.
Dark appearance verified the lane organizer and contrasting light Comms preview.
A clean reopen reset assessment-only navigation filters. NYC-Hospital/Unread
retained its receiving scope and excluded Sent/Outbox; Drafts opened from B and
showed all three existing unsent drafts. Light appearance was restored and the app
was left in All inboxes/Inbox. No messages, permissions or financial mutations were
submitted during this inspection.

Validation: canonical Apple build, strict signature and executable checks, diff
checks and the affected running macOS journeys above. Build log:
`/tmp/cs-mail-stable-organizer-build.log`. Pre-pass source snapshots are under
`target/stable-organizer-baseline-20261016/` (the superseded multi-area renderer
and projection remain recoverable through Git). No new tests or unrelated Rust or
PostgreSQL suites were run. iOS, compact layouts, full VoiceOver, very large lists,
all dark-area combinations, unseen pending/outgoing-only relationship groups and
pointer dwell/dismiss timing were not exercised. This is local UI evidence, not
public mail delivery validation.

## Column B restored after layout assessment — 16 October 2026

At Marcus's request, the column-free navigation assessment was reverted. The four
changed Swift files exactly match their pre-removal snapshots in
`target/navigation-popup-baseline-20261016/`. Persistent column B, ordinary rail
click navigation, transient previews and the alias-management sidebar are restored.
The approved contrasting previews, turquoise Comms palette, continuous spill and
removed sidebar hide button remain. New utility for column B is undecided.

The canonical build installed using the original signing identity and unchanged
designated requirement. Strict signature verification passed; installed and built
executable UUIDs match `7D98BFE9-3389-3CB8-BCCC-C1B17FA7EE3B`. The earlier process
was quit and the canonical app reopened on the original account without enrollment
or credential changes. Running macOS inspection confirmed Comms/All inboxes with
persistent column B, existing correspondence, all three requested alias inboxes,
turquoise perimeter lighting and no sidebar hide control. The app was left there.
Evidence: `/tmp/cs-mail-column-b-restored-build.log`.

Validation: canonical Apple build, strict signature and executable checks, exact
source-restoration comparison, diff checks and running macOS inspection. No new
tests or unrelated Rust/PostgreSQL suites were run. iOS, compact layouts and a fresh
full dark-mode/hover matrix were not checked for this restoration.

## Contrasting navigation previews and continuous spill — 16 October 2026

The native column-B Hide Sidebar toolbar item is removed at its sidebar owner.
Column A previews now use a darker area-tinted surface in light appearance and a
lighter surface in dark appearance, with matching text/control appearance and an
explicit contrasting Close control. The preview and receiving column share one
palette: Comms uses inky turquoise, Relationships dark gold, Express lanes a
visible silver-blue and Economics olive. Main-space reading surfaces stay neutral;
non-Comms column interiors retain their top-only tint.

The twelve discrete spill bands are replaced by one blurred perimeter stroke,
clipped outside the column, using continuous shadow-style falloff. Initial spill
opacity compensates for each color's relative luminance against an approximate
appearance background. This provides a consistent starting-contrast target and
shared falloff, not a claim of perceptually exact equality on every display.
The old separate surface parameter and ring implementation are removed.

The canonical build used the installed Command Line Tools toolchain because the
selected Xcode toolchain currently reports an unaccepted license; no license or
system developer selection was changed. The original signing identity was used,
the build script verified unchanged designated requirements, and final strict
signature verification passed with access to the normal signing/trust services.
Installed and built executable UUIDs match
`0A7C6BC3-183B-3C13-9DCF-11EA93F6D0EE`. The app was quit/reopened at the canonical
path on its existing account without enrollment or Keychain changes. Existing
correspondence and the requested public/private alias inboxes remained visible.

Running macOS inspection covered turquoise, gold, silver-blue and olive lighting,
dark appearance for silver-blue/olive, dark previews in light appearance and a
light Relationships preview in dark appearance. Native shadow and earlier ring
rendering were visually compared before replacement; the new falloff is smoother,
without claiming an exact reproduction of macOS's window-shadow kernel. Final
inspection confirmed no sidebar hide item, readable preview Close, successful
Close dismissal and the original alias inboxes. Light appearance was restored and
the final app was left in All inboxes. Build evidence:
`/tmp/cs-mail-color-pass-build.log`; source snapshots before this pass are under
`target/color-pass-baseline-20261016/`.

Validation scope: canonical Apple build, strict signature verification, matching
executable UUIDs, diff checks and changed-screen inspection. No new tests or
unrelated Rust/PostgreSQL suites were run. iOS, compact layouts, full VoiceOver,
all color/appearance combinations and pointer dwell/dismiss timing were not
validated in this pass. Search and rich message formatting are separate work.

## Current top-only column tint — 16 October 2026

Visual comparison confirmed the earlier non-Comms gradient incorrectly tinted the
full column. It has been replaced by `EditorialColumnTopTint`: a vertical fade
confined to the top 120 points, ending fully transparent with no painted base or
bottom color stop. The middle and bottom retain the native sidebar background.
Relationships uses dark gold, Economics olive and Express lanes blue-grey. Comms
and column-A navigation previews remain on their original rendering paths.

The canonical build installed after Marcus approved the existing signing key.
Strict signature verification passed, the original signing requirement is retained,
and installed/built executable UUIDs match
(`38397DFF-6C72-36F4-8B91-E9D0485580EB`). After quit/reopen on the original account,
macOS screenshots were inspected for Economics, Relationships and Express lanes:
the tint is restricted to the top, with neutral middle/lower interiors. The app was
left on Economics. Evidence: `/tmp/cs-mail-column-top-tint-build.log`. Diff checks
passed. No new tests or unrelated Rust/PostgreSQL suites were run for this
presentation-only correction. Dark mode and iOS were not inspected in this pass.

## Earlier column-interior scope correction — 16 October 2026

Marcus clarified that Comms column B must remain as it was and that column-A
navigation hover boxes were outside this change. The navigation wrapper now leaves
Comms and every preview on their original rendering path, without the added
gradient, foreground override or expansion frame. Only actual non-Comms column-B
interiors receive the shared color wash. The requested dark-gold Relationships
palette is retained. This supersedes the broader preview/Comms treatment below.

The canonical build installed, retained the original designated signing requirement,
passed strict signature verification and matches the final built executable
(`72BB0374-7ED4-38E8-9668-AB9026FFBCB0`). After quit/reopen on the original account,
running visual inspection confirmed the Relationships hover preview's restored
neutral interior and Comms' restored native background with the alias scenario
intact. The app was left in Comms/All inboxes. Evidence:
`/tmp/cs-mail-column-scope-restored-build.log`. Diff checks passed. No new tests or
unrelated Rust/PostgreSQL suites were run; iOS and all dark/preview combinations
were not inspected in this correction.

## Column interior implementation, before scope correction — 16 October 2026

All column-B content now uses one shared interior gradient treatment, with its
color drawn from the same palette as perimeter lighting: Comms blue, Relationships
dark gold (`0.68, 0.46, 0.10`), Express lanes blue-grey, and Economics olive.
Relationships' former cream lighting is replaced by dark gold at Marcus's request.
Tint fades across the interior over the existing canvas, with reduced strength in
dark mode. It no longer depends on wallpaper/native-sidebar blending. The shared
navigation-column wrapper covers the inbox selector, other area lists, My aliases
and navigation previews; the old opaque list background was removed.

The canonical build installed successfully with the original designated signing
requirement. Strict signature verification passed and installed/built executable
UUIDs match (`EFE0F3BE-83EF-39E3-9DD2-67152CD912C1`). The app was quit/reopened on the
preserved owner account and alias scenario. Running macOS visual inspection covered
all four area interiors in light mode, My aliases, the gold Relationships preview,
and a Relationships dark-mode readability check. Light appearance was restored and
the app was left on Relationships. No account, permission or correspondence records
were intentionally changed. Evidence: `/tmp/cs-mail-column-interior-final-build.log`.

Validation scope was the native build, signature checks, diff checks and changed
screen inspection. No new tests or unrelated Rust/PostgreSQL suites were run for
this presentation-only change. iOS, compact layouts, every preview and every dark
area combination were not manually inspected.

## Current owner alias scenario — 16 October 2026

At Marcus's explicit request, the existing owner account now displays protocol
address `mtallis62831@c-sqdmail.com`, public alias `marcus.thomas@c-sqdmail.com`,
and two active private aliases: `mat7771@c-sqdmail.com` (NYC-Hospital) and
`m11a33t@c-sqdmail.com` (Brooklyn-Lab-Co). Addresses use the existing lowercase
canonicalization. The private routes were explicitly supplied local development
fixtures, constructed through the checked AliasPortfolio domain contract; ordinary
private-alias creation continues generating addresses. No alternate member app,
owner account, client adapter or runtime mode was introduced.

The current launcher is `target/alias-assessment-owner-20261016/start.sh`, at the
existing 9693 endpoint, against `cs_mail_alias_assessment_owner_20261016`. It uses
the pinned existing server binary, original deployment keys and unchanged account,
persona and registered operational/content keys. The prior schema-40 source and
`target/alias-inboxes-owner-20261016/start.sh` are retained offline as a checkpoint;
do not revert to them after new activity on the current copy. The directory/SMTP
namespace in the current configuration is `c-sqdmail.com`; this is local configuration,
not DNS/MX ownership or live-provider delivery.

The explicit offline tool and wrapper in `tools/development-continuity/` preserved
the source database, recorded the old directory entry and scenario plan, and changed
only the owner directory mailbox/revision and three checked alias records in one
transaction. No signed historical command or receipt was rewritten or fabricated.
112 other table/sequence fingerprints and every configured key file were identical.
Both rehearsal and owner-copy checks passed rollback after all writes and exact
retry without duplicates. The original encrypted device store was backed up and
confirmed unchanged before reopening. Neither tool accesses or writes Keychain.

The existing signed canonical app reopened through its original Keychain identity
without enrollment. Running inspection verified all three inboxes, each private
lane's exact address and Active status, inbox-to-permissions and lane-to-inbox links,
the public-facing alias in My aliases, and the requested replacement address in
My approach page. Existing correspondence remains in All inboxes; the new inboxes
are empty, with no invented historical routing or sample messages. The app was left
in All inboxes with all three alias selections visible.

Validation: maintenance binary build, targeted clippy with warnings denied,
PostgreSQL preservation/rollback/retry experiments and running macOS inspection.
Evidence: `/tmp/cs-mail-alias-assessment-check.log`,
`/tmp/cs-mail-alias-assessment-owner.log`, and the current deployment's
`verified.json`/`scenario-plan.json`. No product implementation, schema or Swift
presentation code changed, so no new app build or unrelated full quality gate was
needed. Populated alias correspondence, iOS and live mail delivery were not checked
in this fixture pass. Alias-aware public approach-page publication remains unfinished:
My approach page still uses the existing protocol-address directory publication.

## Alias inboxes and receiving evidence — 16 October 2026

Comms column B now selects All inboxes or an alias inbox, using owner-only purpose
labels, unread counts and a historical-inbox section. The main space browses and
reads that selection; search binds to the same scope. A conversation can appear
in several inboxes while retaining one identity. Alias readers show and acknowledge
only matching received messages, with an explicit full-conversation action. All
inboxes retains earlier mail with no recorded alias; no sender/address heuristic
backfills membership. Conversation-wide organization controls remain in All
inboxes; custom unions and independent per-alias organization are later work.
Express lanes separates People and organizations from Private aliases, exposes
explicit receiving scopes and connects alias permission details with its inbox.

Rust's application layer owns historical DeliveredRoute facts and coordinates
explicit signed native alias targets or authenticated SMTP envelope addresses
with the existing deterministic alias admission owner. Storage commits receiving
evidence with the message, copies, conversation, allowance effects and terminal
receipt. Identity-addressed delivery stays distinct and gains no inferred alias.
Recipient account activity and both billing records are protected inputs; service
coverage is evaluated at the application's decision timestamp. The portable client
checks receiving evidence against incoming headers, derives inbox/count projections
without fetching message content, and binds retained-text search and read/unread
actions to an explicit scope. There is no inbox aggregate in the request kernel.

The existing native journey passed receiving through two private aliases in one
conversation, separate unread counts, rejection of a cross-scope read request,
scoped search and cursor binding, retirement history, refused retired delivery,
and cross-alias blocking. Existing request, recovery, organization, deletion,
source-reuse and external journeys also passed. A pre-existing funding experiment
now accepts an immediate definite refusal from a fast simulator while still
requiring exactly voided funding and no delivery. No test cases were added.
The existing real SMTP experiment passed misleading To-header routing, interrupted
message/evidence rollback, a new authenticated guest, canonical address case,
exact redelivery after retirement and refusal of fresh mail after retirement.
Evidence: `/tmp/cs-mail-alias-inboxes-native-checks-complete.log` and
`/tmp/cs-mail-alias-inboxes-smtp-test.log`. The full local quality gate passed: identity pins, dependency direction, formatting,
workspace and production lint, workspace tests, PostgreSQL suites and application
binaries. Evidence: `/tmp/cs-mail-alias-inboxes-quality-complete.log`.

The schema-39 owner service was offline when checked. The explicit upgrade created
a fresh schema-40 copy and preserved all 114 original table/sequence fingerprints,
original migration history and configured key files. No owner Keychain credential,
account association, address or local v3 cache was replaced. The current pinned
owner launcher is `target/alias-inboxes-owner-20261016/start.sh`, with
`owner-ready/verified.json` and `server.sha256`, serving the original 9693 endpoint.
The original schema-39 database and launcher remain the rollback checkpoint.
The existing app reconnected and refreshed successfully through its original
credential. Evidence: `/tmp/cs-mail-alias-inboxes-preservation.log`.

The final signed Apple build installed at the canonical path with the original
designated signing requirement. Strict signature verification passed and the
installed executable's UUID matches the final build. The app was quit and reopened
on the original account. Running-app inspection verified Inboxes/All inboxes,
Jules correspondence and its return path, Manage aliases and its return path,
Express lanes' separate sections, Elena's sender-lane details with explicitly
unrecorded alias scope, and lane-to-correspondence-to-lane navigation. The owner
profile still shows marcus@csqd-mail.com. No owner profile was published, alias
created, message sent, permission changed or retained copy removed. The owner has
no managed aliases; populated alias rows/readers were exercised through native
contracts rather than invented in the owner account. The app was left in All
inboxes. Evidence: `/tmp/cs-mail-alias-inboxes-apple-final.log`.

Current limits: this establishes local receiving contracts, not public MX or
SMTP2GO delivery. Native alias-send contracts require an explicit known participant
and alias target; normal address resolution/composer selection and outbound/reply
alias addressing remain separate work. Public alias approach-page publication and
held-mail release are not implemented here. A standalone MX host needs the explicit
optional registration capability to open previously unknown guest pairs. Inbox
routing observations are session-local; durable offline alias metadata, very large
mailbox performance, iOS, dark/compact layouts and full VoiceOver assessment are
not established by these checks.

## Navigation context and protected edits — 15 October 2026

Correspondence now returns to its actual origin: retained search results, the
selected mailbox, Relationships map or Directory profile. Mailbox query, sorting,
selection and stable scroll anchors live in one presentation navigation owner;
request and lane direction also survive a correspondence round trip. The toolbar
Inbox and Communications root still explicitly return to a fresh Inbox. Alias
management and lane review use the same navigation history.

My approach page uses one session-local editor draft. Leaving dirty fields offers
Save changes and continue, Discard changes and continue, or Stay. Saving continues
only after confirmed publication; a failed save keeps the editor open. Checking a
saved publication does not overwrite newer dirty fields. Explicit Lock uses the
same guard. This draft is not durable across quit or automatic session locking.

Inactive readers are unmounted and the model requires the matching visible reader
context before recording read identities. Contact selection records the latest
intent during an operation and loads it after the operation completes; stale loads
cannot replace the selected contact. Late Directory lookups respect newer
navigation intent. These changes remain in the Swift adapter; protocol and Rust
domain ownership are unchanged.

The final canonical signed build was installed, quit and reopened on the existing
owner account, retaining marcus@csqd-mail.com and the original designated signing
requirement. Strict signature verification passed. Running-app inspection verified
search → Daniel → Search results with the query/results retained, Sent → Jules →
Sent, Relationships → Jules → Relationships map, and Directory → Daniel profile →
correspondence → the same profile. A temporary local display-name edit triggered
the exit dialog; Stay retained it. The original text was restored without
publishing, and the app was left in Inbox.

The existing native integration journey was extended to challenge two conjectures:
an inactive reader must not mark messages read, and the latest contact selection
must survive an overlapping refresh. Both passed, as did the existing isolated
local request, native and external journeys. No test cases were added. Evidence:
`/tmp/cs-mail-navigation-native-checks.log` and
`/tmp/cs-mail-navigation-final-build.log`. Diff checks passed. No unrelated Rust
or PostgreSQL suites were run. iOS, compact/dark layouts, long-list scroll precision,
and Save/Discard against the owner profile were not manually exercised. These
checks establish local navigation behavior, not public email delivery.

## Explicit Inbox return — 15 October 2026

Opened native correspondence now has an explicit Inbox back link in Communications;
the toolbar shows a labeled Inbox action. Selecting Communications clears the
selected contact and returns to Inbox instead of keeping that contact open.
External correspondence has the same explicit return action. Returning clears
the open thread/message focus and resets the mailbox view to Inbox. Other areas
retain their existing Home return behavior. This changes navigation only.

The canonical signed build installed with its original signing requirement and
was quit/reopened on the existing account. Native inspection opened Jules Morgan
and verified all three return paths: the conversation's Inbox back link, the
Communications navigation item and the toolbar Inbox action. Each returned to
Your mail with Inbox selected; the app was left there. Evidence:
`/tmp/cs-mail-inbox-return-build.log`. Diff checks passed. No new tests or unrelated
Rust/PostgreSQL suites were run for this presentation-only change. External and
iOS navigation were not manually exercised.

## Reading, search, organization and outgoing work — 15 October 2026

Communications now opens Your mail, with Inbox, Unread, Flagged, Archive, Trash,
Sent and Outbox. The list uses message activity with stable identity tie-breaking;
A–Z is available. Personal read identities, flags and folder placement are encrypted
on this device. Reading marks only the represented message snapshot; synchronization
does not mark mail read and no sender-visible receipt is produced. Manual unread,
archive/restore and reversible Trash have no permission or retention effects.
Selections support bulk archive, flag, Trash and restore. Native permanent copy
removal is separately confirmed and journaled through DeleteCopies. External
permanent deletion is not offered.

Readers separate messages by sender and service commit time and render structured
text, retained inline images, quotations, shared copies and inert references.
Unavailable and access-denied native copies have distinct projections. Find within
a conversation, Latest and per-message Reply sit alongside existing request and
relationship controls. Search retained message text opens authenticated accessible
copies in bounded pages, reports searched/unavailable/remaining coverage and opens
the matching conversation/message. It never follows references and has no durable
plaintext index. Session locking clears transient results and the Rust search scope.
The sidebar's separate field is explicitly labeled Filter contacts.

Refresh observes mail without resuming mutations, reports incomplete native/external
observations, and rolls back a failed native page's cursor and headers together.
Swift refreshes on foreground activation and periodically while open. The last
complete refresh time remains visible. Existing explicit recovery controls retain
their original operation identities. Sent shows conversations containing committed
outgoing messages. Outbox exposes submission history recorded from this version
onward: unknown, native mailbox committed, external queued or refused. The encrypted
history retains the exact command, originating draft revision and terminal evidence
in the same atomic transaction as the cache and draft workspace. Relay handoff and
provider-reported observations cross FFI as distinct typed facts. Older history is
not fabricated; previous committed mail remains in Sent.

Validation extended existing experiments without adding test cases. Six portable
client checks passed, including later-arrival read races, malformed personal-state
restoration, failed organization writes, no implicit submission retry on refresh,
failed later mailbox pages, search-scope clearing, refusal/unknown outcome history
and exact-command replay. Strict client/FFI lint, identity pins, formatting and
diff checks passed. The native SQLite/FFI journey against isolated local PostgreSQL
services passed third-record transaction rollback, request/native/external journeys,
personal organization and retained-text search. Fixture-only permanent removal
made the caller's copy unavailable while the peer retained its readable copy.
Evidence: `/tmp/cs-mail-mailbox-tests-final.log`,
`/tmp/cs-mail-mailbox-lint-final.log`, `/tmp/cs-mail-mailbox-native-checks.log`,
`/tmp/cs-mail-mailbox-identity.log` and `/tmp/cs-mail-mailbox-apple-final.log`.

The first canonical build installed with the original signing requirement and
was quit/reopened on the preserved owner account. Native light-appearance inspection
found the previously missed older text through search, opened the matching message,
checked individual timestamps, marked a snapshot unread, archived/restored a
conversation to Inbox, and inspected Sent and initial empty Outbox. No owner
message was sent, charge authorized or retained copy permanently removed. Locking
cleared search results and open mail; reopening retained the original account and
`marcus@csqd-mail.com`. The mailbox workspace was left open. The isolated test
services were stopped after validation. Following the second macOS signing
authorization, the final build containing the native-page rollback refinement
installed at the canonical path with the original signing requirement. The app
was quit/reopened; its installed executable's build UUID matches the final build,
and its signature verifies strictly. Native
inspection confirmed the mailbox workspace, persisted unread/folder state and all
three earlier unsent drafts. The final mailbox workspace is running and left open.

The full quality gate and unrelated server suites were not run. iOS, dark/compact
layouts, full VoiceOver assessment, actual power loss, very large mailboxes,
cross-device personal state and live/public-provider delivery were not exercised.
Populated outgoing history and external rendering were exercised through native
contracts; the owner account's Outbox is empty and its external inbox unpopulated.
No new provider-event ordering experiment was run; existing server delivery rules
are unchanged. Subjects, general attachments and complete offline content search
remain separate work.

## Durable drafts and submission handoff — 15 October 2026

The member application now autosaves native messages, external emails, approach
requests and follow-ups in an encrypted device-local workspace. Communications
offers a Drafts list that reopens the selected writing by its exact identity.
New emails and new native conversations create independent drafts; replies resume
their existing context. Composers show saving status, retain writing after refusal,
offer explicit save/close and discard choices, and keep disclosure consent and
charge approval separate from saved writing. Local saving does not wait for the
network operation queue. Drafts are local to this device; they are not synced.

Rust's client workspace owns the deterministic draft lifecycle, checked identities,
revisions and restoration. The protocol kernel does not own drafts. Submission
atomically moves an exact draft revision into the existing recovery journal;
uncertain results retain that operation for exact retry, while a confirmed refusal
restores editable writing. SQLite commits workspace and journal together. Existing
account encryption and the original Keychain credential association are retained.

Validation extended existing experiments, without adding test cases: six portable
client recovery checks passed, including stale revisions, independent drafts,
local saves during network work, invalid restoration, atomic handoff failure,
refusal restoration and exact retry after reopen. The native SQLite/FFI journey
passed against isolated local PostgreSQL services, including second-write rollback,
request-draft reopen before charge approval, native and external submission, reuse
policy, consent, refusal and recovery. Strict client/FFI lint, formatting, identity
source pins and diff checks passed. Evidence:
`/tmp/cs-mail-drafts-tests-final.log`, `/tmp/cs-mail-drafts-lint-final.log`,
`/tmp/cs-mail-drafts-native-checks.log`, `/tmp/cs-mail-drafts-identity.log` and
`/tmp/cs-mail-drafts-apple-build.log`.

After macOS signing authorization, the canonical build installed successfully with
the original signing requirement. The app was quit and reopened at
`target/apple/CsMail-continuity-20261012.app`; native inspection confirmed the
existing owner address `marcus@csqd-mail.com` and existing relationships. Two
independent external drafts retained their distinct recipients and bodies, and a
native reply retained its writing across a complete app restart. Native inspection
also checked save status, the close/save choices and discard confirmation/cancel;
it did not execute permanent discard. The three clearly labeled usability-check
drafts remain unsent in Drafts for assessment. No owner message was sent or charge
authorized. This canonical build also includes the preceding relationship-map
spacing refinement, superseding its historical signing wait below.

The full quality gate and unrelated suites were not run. iOS, dark/compact draft
layouts, actual power loss and cross-device recovery were not exercised. Native
contract checks covered request/follow-up submission; those composer screens were
not manually inspected on the owner account. Local simulation establishes no
SMTP2Go/public delivery. Unread flags and broader mailbox organization remain
subsequent slices.

## Relationships overview map — 14 October 2026

The Relationships overview replaces Choose a request with a static incoming
relationship map centered on You. It reuses the Rust-supplied permission summaries
already used by Home: standing permission has a pale gold solid line, express
lanes a cool silver dashed line, and history without standing permission a faint
dotted line. Each person retains their supplied permission subtitle and opens the
existing correspondence view. Closed-route history is not an active permission;
there are no inferred connections between contacts or invented alias branches.
Swift owns only arrangement, line styling and navigation. Nodes use generous
click regions and perimeter-only hover feedback, with no continuous animation.
The map scrolls as needed and uses four fixed positions per ring to make room for
labels. Receiving-route grouping and an outgoing map remain subsequent work.

The first canonical signed build and diff checks passed. Installation retained
the original signing requirement; the app was quit/reopened on the existing owner
account. Native inspection checked the four-contact map in light appearance and
confirmed that Daniel's node opens Daniel's correspondence. The map was left open.
No account or relationship data was changed. Evidence:
`/tmp/cs-mail-relationship-map-build.log`. A subsequent refinement aligns the
node perimeter with its hover outline and limits each ring to four fixed positions.
It compiled successfully (`/tmp/cs-mail-relationship-map-layout-build.log`), but
its final canonical installation is waiting at macOS signing authorization
(`/tmp/cs-mail-relationship-map-final-build.log`). The running first build uses
the same four-contact arrangement; the refinement is not yet installed.
No tests or unrelated Rust/PostgreSQL suites were run for this presentation-only
change. Empty and multi-ring layouts, pointer-hover rendering, dark appearance,
compact layout and iOS were not visually exercised.

## Generated private aliases — 14 October 2026

New Alias now separates private purpose/description from public email-address
choice. Private creation asks for owner-visible context and Rust generates an
unpredictable 128-bit address. The list and alias-lane detail show the description
prominently; Copy address copies only the address. Confirmed creation selects the
alias ID from the verified outcome. Private replacement generates another address
and preserves the description. Public creation uses a chosen address; a current
public alias instead offers Manage public alias, where replacement is available.
Rust owns the at-most-one-current-public-alias rule, including concurrent requests;
Swift presents its projection and never supplies a private mailbox candidate.

Signed management uses version-2 commands/acknowledgements and schema 39, without
old-format runtime readers. The explicit offline creation upgrade refuses populated
legacy alias metadata; it does not invent descriptions or rewrite signed history.
The owner source had no alias records or commands. The preservation tool restored
a fresh copy and verified all 114 original table/sequence fingerprints, original
migration history and configured key files. Original account association and
`marcus@csqd-mail.com` were retained; no Keychain credential was replaced or
reenrolled. The schema-38 source and launcher remain the rollback checkpoint.
The current pinned owner launcher is `target/alias-creation-owner-20261014/start.sh`,
with its checked server hash and `owner-ready/verified.json`, serving the same 9693
endpoint. Evidence: `/tmp/cs-mail-alias-creation-preservation.log`.

Validation passed: scoped Rust check; the existing domain restoration and portable
client recovery experiments; the existing PostgreSQL lifecycle/replay experiment,
extended to race different public addresses for one owner and replace the winner;
strict lint for the six affected Rust packages with all targets/features; identity
source pins; formatting/diff checks; canonical signed Apple build; and the existing
native enrollment/alias experiment against a fresh local simulation. These challenge
invalid restoration, encrypted description storage, exact retries, atomic failure,
public-alias cardinality, permanent reservation, Directory isolation and account/
credential continuity. Existing experiments were adapted; no test cases were added.
Evidence: `/tmp/cs-mail-alias-restoration-tests.log`,
`/tmp/cs-mail-alias-recovery-tests.log`, `/tmp/cs-mail-alias-creation-postgres-final.log`,
`/tmp/cs-mail-alias-creation-lint-final.log`,
`/tmp/cs-mail-alias-creation-native-checks.log` and
`/tmp/cs-mail-alias-creation-apple-build.log`.

The canonical app was installed with its original signing requirement and
quit/reopened on the preserved owner deployment. Native inspection checked both
creation types, private draft enablement, Cancel and a fresh empty dialog. No
owner alias was created (both alias routes and command counts remain zero).
Populated alias screens, the existing-public-alias navigation branch, clipboard
interaction, dark appearance and iOS were not visually exercised; native contract
checks covered populated projections. The full quality gate and unrelated suites
were not run. Alias mail delivery/publication and public-provider delivery remain
outside this management change; local simulation is not evidence of live delivery.

## Express lanes navigation icon — 14 October 2026

Column A uses two upward arrows side by side for Express lanes, composed from
matching native symbols. Its label, click region and navigation meaning are
unchanged. The canonical signed build installed with the original signing
requirement and was quit/reopened; the paired arrows were visually checked in
light appearance. Evidence: `/tmp/cs-mail-express-arrows-build.log`. Diff checks
passed. No tests or unrelated Rust/PostgreSQL suites were run for this icon-only
change; compact layout, dark appearance and iOS were not visually inspected.

## Matching rail and top-row canvas — 14 October 2026

Column A now explicitly draws the shared canvas instead of inheriting it. The
root canvas extends through the top safe area, and the macOS toolbar's separate
background is hidden so the top row uses that same color: `#F7F6F2` in light
appearance. The main space retains the same canvas. Column B's native container,
toolbar cap, perimeter lighting and spacing remain in place. An opaque toolbar
fill was removed during inspection because it covered the panel's top edge.

The canonical signed build and diff checks passed. The app was installed with
its original signing requirement, quit/reopened and inspected on Communications
in light appearance: the top row, rail and main-space backgrounds match and the
panel's full illuminated perimeter remains visible. Evidence:
`/tmp/cs-mail-unified-canvas-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; dark appearance, compact layout and iOS were not inspected.

## Column B colors by area — 14 October 2026

Column B now uses pale gold for Relationships (including its address and profile
destinations), cool light-gray silver for Express lanes, electric blue for
Communications and muted olive green for Economics. One presentation-owned palette
selects the spill from the current navigation area. Communications, Relationships
and Economics use a one-point rim in a darker shade of their wash color (65% of
each RGB component, drawn at 55% opacity). Express lanes uses a white rim at
32% opacity, as do contextually lit navigation previews. All cove rims now share
one-point thickness. Each rim is centered on the panel outline,
with color immediately adjacent to its outer edge. The wash starts at 13%
opacity and fades quadratically over twelve
points. The drawing surface extends fifteen points beyond its layout bounds
on every side so the left spill has the same drawing room as the right.
The earlier six-point near-white transition was removed
because it made the colored spill appear detached from the rim.
The native panel, toolbar cap, shadow, narrower column A and gutter are retained.
Home and Notes have no column B. Navigation previews use their area's cove
lighting only when column B is absent; otherwise they retain their original
border and shadow. The member view passes column visibility explicitly to the
navigation shell, including the sidebar's show/hide state. Preview lighting
reuses `EditorialCoveLight`, with no separate palette or falloff implementation.

The preview wash starts at 20% opacity rather than column B's 13%, so the muted
colors remain discernible beside its white rim on the shared ivory canvas.
The twelve-point quadratic fade and visibility condition are retained. The
canonical signed build and diff checks passed, installation retained the original
signing requirement, and the app was quit/reopened. The Economics preview over
Home was visually checked and left open. Evidence:
`/tmp/cs-mail-preview-wash-visibility-build.log`. No tests or unrelated
Rust/PostgreSQL suites were run; the other palettes, dark appearance and iOS
were not visually rechecked for this opacity adjustment.

The white rims were subsequently reduced from three points to one, matching the
tonal rims. Their colors, opacity, wash falloff and contextual preview behavior
are unchanged. The canonical signed build and diff checks passed, installation
retained the original signing requirement, and the app was quit/reopened.
The Economics preview over Home and Express lanes column B were visually checked
in light appearance. Evidence: `/tmp/cs-mail-uniform-rim-build.log`. No tests or
unrelated Rust/PostgreSQL suites were run; dark appearance and iOS were not checked.

The conditional treatment passed the canonical signed build and diff checks,
was installed with the original signing requirement and quit/reopened, and was
visually checked in light appearance: cove previews over Home and Notes, original
preview shading over a visible Communications column B. Evidence:
`/tmp/cs-mail-contextual-preview-light-build.log`. No tests or unrelated
Rust/PostgreSQL suites were run. Manually hidden-sidebar behavior, dark appearance,
compact layout and iOS were not visually exercised in this pass.

The thinner tonal column B rims passed the canonical signed build and diff
checks, were installed with the original signing requirement and quit/reopened,
and all four area screens were visually inspected in light appearance, including
the preserved Express lanes white rim. Evidence: `/tmp/cs-mail-tonal-rim-build.log`.
The native containers, panel positions and contextual preview behavior are
unchanged. No tests or unrelated Rust/PostgreSQL suites were run; dark appearance,
compact layout, preview rims and iOS were not visually rechecked for this pass.

The earlier unconditional preview-lighting experiment was removed: previews retained their original
border and shading, without a cove rim or colored wash. The restoration passed
the canonical signed build, was installed and quit/reopened, and the Communications
preview was visually checked and dismissed with Escape. Evidence:
`/tmp/cs-mail-restore-preview-shading-build.log`.

Economics was then toned down from neon green to olive (RGB 0.55, 0.62, 0.30).
Its white rim and wash strength/falloff remain unchanged. Lighting now has one
presentation owner in `EditorialCoveLight`; the former private implementation was
removed. The canonical signed build and diff checks passed, installation retained
the original signing requirement, and the reopened Economics screen was visually
checked in light appearance. Evidence: `/tmp/cs-mail-olive-glow-build.log`.
No tests or unrelated Rust/PostgreSQL suites were run; dark appearance, compact
layout and iOS were not exercised for these revisions.

The canonical signed build passed, preserved the original signing requirement,
was installed and quit/reopened, and all four area screens were visually checked
in light appearance. Express lanes is left open. Evidence:
`/tmp/cs-mail-area-glow-build.log`. Diff checks passed. No tests were added or
unrelated Rust/PostgreSQL suites run. Dark appearance, compact layout and iOS
were not visually exercised; no account, credential or domain state was changed.

The rim is now one three-point stroke on the outline rather than three filled
bands. Its outer edge defines the colored wash's origin. The canvas was expanded
without moving either column to remove left-side clipping. The canonical signed
build passed, retained the original signing requirement, was installed and
quit/reopened, and both verticals, rim and corners were visually checked on
Communications in light appearance. Evidence:
`/tmp/cs-mail-perimeter-rim-build.log`. Diff checks passed. No tests or unrelated
Rust/PostgreSQL suites were run; other palettes, dark appearance and iOS were
not visually rechecked for this shared geometry correction.

The wash was subsequently softened from roughly 24.5% opacity at the rim to
13%, and shortened from a 24-point total spread to 15 points with a faster fade.
The white rim, palette assignments, native panel and restored column spacing are
retained. The canonical signed build and diff checks passed; the app was
installed with its original signing requirement, quit/reopened and visually
checked on Communications in light appearance. Evidence:
`/tmp/cs-mail-softer-wash-build.log`. No tests or unrelated Rust/PostgreSQL suites
were run. Other palettes, dark appearance and iOS were not visually rechecked.

The adjacency correction passed the canonical signed build and diff checks,
was installed with the original signing requirement and quit/reopened, and was
visually checked on Communications in light appearance. The blue spill now
touches the white rim around the panel. Evidence:
`/tmp/cs-mail-adjacent-glow-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run. The other three palettes, dark appearance and iOS were not
visually rechecked for this shared boundary correction.

## Narrower column A — 14 October 2026

The vertical navigation rail is 64.5 points wide, three quarters of its former
86 points. Buttons retain their 64-point height and use the available rail width.
Relationships scales modestly to remain on one line; Express lanes wraps onto
two lines. An additional 21.5-point space after the rail preserves column B's
original horizontal position. Preview positioning includes that space as well.
The compact horizontal rail is unchanged.

The vertical divider now stops above the window bottom, with its bottom inset
matching the top safe-area inset. Column widths, gap and panel placement are
unchanged; the compact horizontal divider retains its existing layout. The
canonical signed build and diff checks passed, the app was installed with its
original signing requirement and quit/reopened, and the divider endpoints were
visually checked on Express lanes in light appearance. Evidence:
`/tmp/cs-mail-divider-inset-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; dark appearance, compact layout and iOS were not checked.

The initial implementation incorrectly moved column B left with column A. The
spacing correction restores its original position while keeping column A narrow.
The canonical signed build passed, retained the original signing requirement,
was installed and quit/reopened, and the larger gap and unchanged native column B
were visually checked on Communications. Evidence:
`/tmp/cs-mail-column-gap-build.log`. Diff checks passed. No tests or unrelated
Rust/PostgreSQL suites were run; previews, dark appearance, compact layout and
iOS were not rechecked for this spacing correction.

The canonical signed build passed, retained the original signing requirement,
was installed and quit/reopened, and was inspected on Express lanes. The native
column B container, pale-gold glow and inter-column gutter were preserved. The
Relationships preview was opened through its accessibility action, checked for
placement and dismissed with Escape. Evidence: `/tmp/cs-mail-narrow-rail-build.log`.
Diff checks passed. No tests or unrelated Rust/PostgreSQL suites were run;
pointer-hover dismissal, compact layout, dark appearance and iOS were not
exercised in this pass.

## Cooler ivory canvas — 14 October 2026

The shared light-appearance canvas is now `#F7F6F2`, replacing `#F7F6EF` across
column A, column B, the main space and navigation previews. The silver lighting
and dark palette are unchanged. The canonical signed build passed with the
original signing requirement, was installed and quit/reopened, and the light
Express lanes screen was visually inspected. Evidence:
`/tmp/cs-mail-cool-ivory-build.log`. Diff checks passed. No new tests or unrelated
Rust/PostgreSQL suites were run for this color-only change; iOS was not inspected.

## Column B perimeter-lighting experiment — 14 October 2026

The earlier Express lanes trial combined a thicker white edge reflection
with a very pale gold spill around column B's whole rounded perimeter. The
white rim holds for two points before a six-point blend into the spill; the
24-point outward falloff and opacity are retained. Its interior remains untouched. Adjacent outward bands
follow the measured panel bounds and include the native toolbar cap with its
eight-point top gutter. The former flat silvery-gold tint, right-edge-only gradient
and rejected Daniel portal trial are removed. The decoration accepts no input,
adds no accessibility content and has no animation. This remains a local visual
experiment, not an adopted toolkit rule.

The thicker white/pale-gold revision passed the canonical signed build, was
installed with the original signing requirement and quit/reopened, and was
visually inspected on Express lanes in light appearance. The native panel shape,
sizing, toolbar cap and shadow are unchanged. Evidence:
`/tmp/cs-mail-pale-gold-rim-build.log`. Diff checks passed. No tests were added
or unrelated Rust/PostgreSQL suites run; dark appearance and iOS were not inspected.

The attempted shadow-free replacement panel was rejected and removed. The
original native column B container, toolbar cap and sizing are restored, with
the six-point silver reflection and lighter spill retained. The restoration
passed the canonical signed build, preserved the signing requirement, was
installed and quit/reopened, and was visually checked on Express lanes.
Evidence: `/tmp/cs-mail-restore-column-build.log`. The native shadow remains.
No automated tests or unrelated Rust/PostgreSQL suites were run for this
presentation-only restoration; dark appearance and iOS were not inspected.

The canonical signed build passed, preserved the original signing requirement,
and was installed at `target/apple/CsMail-continuity-20261012.app`. The prior
process was quit and the app reopened. All four illuminated edges and rounded
corners were inspected in light appearance on the running Express lanes screen.
No account, credential or domain state was changed. Evidence:
`/tmp/cs-mail-silver-rim-build.log`. Diff checks passed. No tests were added
or unrelated Rust/PostgreSQL suites run. Dark appearance, alternate window sizes
and iOS were not visually exercised in this pass.

## Visual simplicity pass — 14 October 2026

The [Swift design principles](../../docs/swift-design-principles.md) now guide
native presentation. Express lanes and My addresses no longer show independent
reload buttons or observation timestamps. Alias metadata loads on entry and
foreground return, and refreshes every minute while the workspace is active.
The single toolbar Refresh targets the current workspace. Address changes still
require a current Rust projection; failed reads retain a concise notice. Read
cancellation preserves the active session and committed navigation, while
submitted changes retain their existing journal/recovery semantics.

Directory has one name/address/link input, with exact references validated by
the existing Rust lookup contract and textual searches using the listed-results
contract. The duplicate toolbar refresh is removed. My approach page uses Save
changes and shows recovery only when a saved publication is pending. Refresh
preserves unsaved edits; a changed saved revision requires an explicit discard
choice before replacement. Home replaces generic scope/validity qualifications
with the projected lane expiry. List dates are shorter; lane details retain
exact expiry, purpose and message allowance.

Creation, alias lifecycle, composer and recovery copy is shorter. Preserved
conversations use available correspondent names and derived display labels instead
of raw identifiers. The Rust financial presentation uses plain charge/refund
labels without changing amounts or lifecycle decisions. Development amounts
remain explicitly simulated units: the current contracts do not supply a
currency code and decimal scale, so no currency conversion is inferred.
Disclosure consent, charge authorization, permanent address reservation,
permission limits and unfinished delivery remain visible where relevant.

The canonical app was signed with its original requirement, installed at
`target/apple/CsMail-continuity-20261012.app`, quit/reopened and inspected against
the preserved owner account. Home, lane grouping/details, My addresses, both
creation purposes, approach-page controls, Directory search/exact lookup,
financial details, and empty native/external composers were inspected. Rapid
lane-to-communications navigation retained available composing controls without
a manual refresh. Inspection drafts were closed; no messages, address creations,
profile changes or payment authorizations were submitted. Marcus's original
`marcus@csqd-mail.com` publication and account/credential association remain in
place. The existing pinned owner backend was restarted without a database or
configuration cutover.

Six existing client tests, formatting/diff checks and the final signed native
build passed. Evidence: `/tmp/cs-mail-visual-cleanup-client-tests.log`,
`/tmp/cs-mail-visual-cleanup-final-build.log` and
`/tmp/cs-mail-visual-cleanup-owner.log`. No automated tests were added. Unresolved
publication/address recovery, populated alias details, deliberately delayed
read cancellation, iOS and public-provider delivery were not exercised in this
pass. Unrelated PostgreSQL suites and the full quality gate were not run.

## Express lane groups — 14 October 2026

Column B now separates Alias-scoped express lanes from Sender-scoped express
lanes. Incoming/Outgoing belongs only to the sender group. Sender details retain
their scope, expiry and rate; alias rows and details show the Rust-projected
lifecycle and link to the existing address manager. Rust classifies aliases from
their checked grant, excludes public addresses, and retains closed lifecycle
states. Swift never treats an alias identifier as a sender identity.

The canonical signed app was installed with its original signing requirement,
quit/reopened, and inspected against the preserved owner account. Both headings,
the empty alias group, incoming sender details and outgoing switching were
verified in the running app. It is left on Incoming with Elena's sender lane
selected. No owner test aliases or credentials were created. Populated alias
rows/details were not visually exercised in the owner account, which has no
additional addresses; classification and closed states were exercised through
the real isolated native bridge experiment.

Six client tests, strict client/FFI lint, formatting/diff checks, the signed native
build and the existing scoped native enrollment experiment passed. The experiment
was extended to challenge public-address misclassification and loss of closed
alias lifecycle state; no test cases were added. Evidence:
`/tmp/cs-mail-lane-groups-client-tests.log`,
`/tmp/cs-mail-lane-groups-lint.log`,
`/tmp/cs-mail-lane-groups-native-checks.log` and
`/tmp/cs-mail-lane-groups-build.log`. No schema or remote API change was needed.
Unrelated PostgreSQL suites, the full quality gate, iOS and public-provider
delivery were not run. Additional-alias mail delivery remains pending.

## New Alias creation dialog — 14 October 2026

A subsequent terminology pass changes the Relationships entry and workspace to
My aliases, and aligns empty states, private/public purpose descriptions,
alias-scoped lane details, lifecycle confirmations and pending-update recovery
with alias wording. Email address remains the label for the actual mailbox value;
physical-address, Directory and external-recipient wording retain their meanings.
This pass changes presentation copy only, with no new domain rules or tests.

The canonical signed build passed and installed with the original signing
requirement. After quit/reopen, native inspection checked the Relationships entry,
My aliases workspace and both private/public-facing creation descriptions; the
empty public-facing dialog was left open. No alias was created. Evidence:
`/tmp/cs-mail-alias-terminology-build.log`. Diff checks passed. Populated lifecycle
states and mutation/recovery outcomes were reviewed in source but not exercised;
dark appearance, iOS and unrelated Rust/PostgreSQL suites were not run.

The creation control and dialog title now read New Alias, with Create alias and
matching progress/outcome wording. The permanent-reservation/incomplete-delivery
paragraph was removed from this dialog at the owner's request. Purpose selection,
Rust validation and creation/recovery semantics are unchanged. The canonical
signed build and diff checks passed, installation retained the original signing
requirement, and the app was quit/reopened. The New Alias button and private
creation dialog were visually inspected; no alias was created. Evidence:
`/tmp/cs-mail-new-alias-copy-build.log`. No tests or unrelated Rust/PostgreSQL
suites were run; mutation outcomes, public-facing selection, dark appearance
and iOS were not exercised for this copy-only change.

New address opens a dedicated creation sheet with Full address, purpose, Create
address and Cancel. Creation is no longer an inline navigation destination or a
focus event. Cancelling dismisses the draft without submitting it. A confirmed
creation opens the verified record; an unknown outcome retains the original Rust
journal and directs the member to Resume update. Closing after an attempt is
labelled Close, not cancellation of the operation. Lock dismisses the sheet and
session-generation checks suppress late result callbacks.

The final canonical signed build was installed and quit/reopened with its original
signing requirement and account association. Native inspection checked that the
button opens a sheet, typing enables the separate Create action, Cancel closes
the sheet, reload still shows no additional addresses, and a fresh dialog opens
with an empty draft. Its layout was inspected. The app was left with the new
dialog open. No owner address was reserved. Evidence:
`/tmp/cs-mail-new-address-dialog-final.log`. Diff checks passed. No tests were
added and no Rust/PostgreSQL suites were run for this Swift presentation change.
Creation commit/recovery and iOS runtime were not re-exercised in this repair.

## My addresses management — 14 October 2026

The unified app has a My addresses workspace under Relationships, with an address
list in column B and details in C; New address opens a creation sheet. Private/public purpose, lifecycle,
transition status, available actions and preserved conversation facts come from
Rust projections with an observation time. Swift captures user intent and revision;
Rust validates and journals Create, Restrict, Replace and Retire. A typed refusal
is distinct from an unknown outcome. Resume update reuses the original signed
operation; stale snapshots disable new changes. Lock clears address presentation,
and session generations reject late results. No account or Keychain credential
is created or replaced by these operations.

The existing primary address and My approach page remain intact. Additional
addresses explicitly state that sending/receiving is unavailable until delivery
integration. They do not receive public profiles, paid-request links or automatic
permission transfers. Composition, receiving-route evidence, held-mail handling
and alias-aware acceptance remain subsequent work.

The offline `tools/development-continuity/alias_metadata_upgrade.py` restored a
fresh schema-38 owner copy and verified all 110 original table/sequence
fingerprints, original migration history and configured key files. The original
schema-37 database remains untouched as a rollback checkpoint. The new pinned
owner launcher is `target/alias-native-20261014/start.sh`, using
`target/alias-native-20261014/owner-ready/maild.json`; its server hash and
preservation result are retained beside it. It serves the existing 9693 endpoint.
No reenrollment, owner-address substitution or direct canonical-record edits were
performed.

Scoped validation passed: six portable-client tests, including extended exact
alias recovery and typed refusal; strict lint for the client/bridge with all
targets/features; all 101 identity source pins; formatting/diff checks; Swift
compilation; and the existing native enrollment journey extended through the
real isolated backend for create/restrict/replace/retire, invalid transition
refusal, retired-address reservation, Directory isolation and retained account/
credential. No test cases were added. The native run used the explicit alias
scope; unrelated conversation/payment branches were not run. Evidence:
`/tmp/cs-mail-address-client-tests.log`, `/tmp/cs-mail-address-lint-final.log`,
`/tmp/cs-mail-address-native-checks.log`, `/tmp/cs-mail-address-preservation.log`
and `/tmp/cs-mail-address-swift-final.log`. Actual alias delivery, iOS runtime,
held-mail release and public-provider delivery were not exercised.

Installation completed at the canonical
`target/apple/CsMail-continuity-20261012.app` path after Marcus authorized the
original signing key directly in macOS. The build script verified the original
signing requirement before replacement. Quit/reopen and refresh were inspected in
the installed app: My addresses loaded its real empty list and observation time;
private/public purpose copy and the creation confirmation were checked; the
inspection draft was cancelled and cleared without reserving an address. The
original `marcus@csqd-mail.com` publication, simulated coverage and saved contacts
remain present without reenrollment or device-credential replacement. The app is
left on My addresses. Evidence: `/tmp/cs-mail-addresses-apple-final.log`.

Address-detail lifecycle controls were exercised through the real isolated native
bridge experiment, but were not visually exercised with populated addresses in
Marcus's account, which has no additional address records. No owner test addresses
were created. This validation does not establish alias mail delivery.

## Alias backend foundation — 14 October 2026

The portable Rust client and backend now have checked alias lifecycle, explicit
receiving scopes, signed owner management commands and encrypted exact-operation
recovery. The native bridge understands a pending alias update so the existing
resume path can resolve its original operation. Native alias controls and the
native/SMTP delivery integration are pending; this build does not make aliases
usable for mail. See the [implementation boundary](../../docs/application-persistence-boundaries.md#alias-foundation--14-october-2026).

The canonical signed Apple build was installed at
`target/apple/CsMail-continuity-20261012.app`, after comparing its original signing
requirement. Marcus authorized macOS's existing signing-key prompt directly.
Quit/reopen and refresh were inspected against the original pinned owner backend;
the approach page retains `marcus@csqd-mail.com`, simulated coverage and the original
contacts. No reenrollment or replacement device credential was introduced. The
backend had been stopped and was restored with its existing
`target/column-b-navigation-20261013-8a4b/start.sh` launcher. Its verified database,
configuration and pinned executable were retained; migration 0038 was not applied
to that owner deployment. Build evidence: `/tmp/cs-mail-alias-apple-build.log`.

Backend validation uses an isolated PostgreSQL database and extends existing
conjecture-driven experiments without adding test cases. The final gate run passed
identity pins, dependency direction, formatting, production lint, 114 workspace
checks/tests, 106 PostgreSQL tests and application binaries. Its aggregate result
retains an earlier all-target lint failure from a newly extended test fixture's
method/enum syntax. That fixture was corrected; the subsequent strict workspace,
all-targets, all-features lint rerun passed. Formatting and diff checks were then
rechecked. Evidence: `/tmp/cs-mail-alias-quality-gate-final.log` and
`/tmp/cs-mail-alias-clippy-final.log`. Actual alias delivery, concurrent alias
delivery/blocking, held-mail release, iOS and public providers are outside these
experiments.

## Neutral contact focus in column B — 13 October 2026

Sidebar rows suppress the system's blue focus effect and draw a neutral outline
for focus, retaining the existing neutral selection wash and large click regions.
This applies to the shared contact/request/lane/financial rows and sidebar destinations.
The canonical signed build was installed and reopened. Pointer selection in the
active Communications screen and Tab focus were inspected; focus remains visible
without blue highlighting. Build evidence: `/tmp/cs-mail-contact-focus-build.log`.
No tests were added or unrelated Rust/database suites run. iOS was not inspected.

## Area-specific selection in column B — 13 October 2026

Column A selects an area; B selects an item; C opens that item's contents. Home and
Notes have no sidebar. Communications has one searchable correspondence list,
including external mailboxes, and email composition is a toolbar action. External
reading no longer introduces another mailbox list; multiple conversations for the
same selected mailbox use a conversation picker.

Relationships retains Directory and My approach page, with Incoming/Outgoing request
filters. Each request has its own identity, status and recorded terms-issuance date;
direction is retained when opening its conversation. Express lanes lists current
incoming and outgoing grants, with expiry in the row and scope/rate in the detail.
Economics lists financial records with concise payment/refund summaries and issuance
dates, and opens one selected record. Q&A has one destination. The duplicate global
relationship/lane cards, self-links, permission subtitles in unrelated lists and
sidebar implementation explanations are removed.

The portable client obtains owned incoming and outgoing records through authenticated
member reads and projects lanes for an explicit time. The new Outbox read is restricted
to the signed address; recipient names have their own outgoing-view field. Navigation
does not change the encrypted cache format, create relationships, grant permission,
authorize charges or record sending consent. Its read failure is shown as a refresh
error rather than an empty-list claim.

The current owner launcher is `target/column-b-navigation-20261013-8a4b/start.sh`.
It uses the same `target/principles-alignment-20261013/owner-ready/maild.json`, verified
owner database, deployment keys and device credential. The prior pinned backend is
retained; there is no schema migration or reenrollment for this update.

Relevant validation: existing client tests, lint checks for the affected Rust crates,
the PostgreSQL acceptance scenario extended with owned-Outbox and other-persona signing-key refusal
checks, and the existing native journey extended with lane identity/direction/expiry
assertions. No test cases were added. The initial native run stopped in the optional
failed-payment experiment; its log is retained. The scoped final native journey passed
without that unrelated optional branch, including external consent/reply/permission
workflows. Evidence: `/tmp/cs-mail-column-b-client-tests.log`,
`/tmp/cs-mail-column-b-clippy.log`, `/tmp/cs-mail-column-b-service-tests.log`,
`/tmp/cs-mail-column-b-native-checks-final.log` and `/tmp/cs-mail-column-b-build.log`.
The installed owner app was inspected for list/detail selection, request filters,
historical record dates, both lane directions, full-width Home/Notes and the empty
email composer. Its owner has no external threads, so the integrated external-mailbox
list and multi-conversation picker remain unexercised interactively there. iOS,
public-provider delivery and unrelated Rust/database suites were not checked.

## Consistent reading and copying — 13 October 2026

`EditorialReadingView` gives each reading region one native, read-only text surface.
Conversations, external-message transcripts, Q&A, profile descriptions, request terms,
financial records, referenced text and account status support selection across their
paragraphs and blocks. Controls and editable fields remain separate. Navigation rows
and contact/profile cards expose a context-menu Copy command; their labels do not
compete with clicking through text-selection gestures. The source-selection editor
retains its existing policy-aware selection mapping.

The macOS view measures with a separate layout manager, so SwiftUI width probes cannot
resize the displayed text. Unchanged document refreshes preserve selection; appearance
changes preserve it when the text is unchanged. Text remains wrapped and scrolls in
its existing reading container.

Relevant validation: the canonical signed Apple build, actual dragging across Q&A
paragraphs and two native messages, copying/pasting into an unsent composer, scrolling,
Select All confined to the document, and a contact-card Copy command that leaves
navigation unchanged. Verification drafts were cleared without
sending. Original account and Keychain continuity are preserved. No tests were added
and unrelated Rust/PostgreSQL suites were not run. Build evidence is
`/tmp/cs-mail-reading-install.log`. iOS runtime, every secondary form and external
transcripts with messages have not been interactively exercised in the owner app.

## Audit repairs and preserved owner deployment — 13 October 2026

External mutations now return provider-signed acknowledgements bound to the exact
command digest, caller key/signature, recorded time and outcome. Authenticated
semantic refusals commit a receipt without conversation, ciphertext or delivery
changes. Replaying one after consent restoration or unblocking retains that original
refusal; a new attempt requires a new operation. HTTP errors, unavailable facts,
unresolved policy and altered acknowledgements retain the encrypted journal.

External paging follows cursor progress through expired-only pages. Native and
external contacts share one permission projection with explicit time and matching
lane evidence. Swift renders standing, bounded and closed perimeters from that typed
projection. Restored protocol records reject multiple unresolved requests and foreign
owners. A malformed stored record is a storage fault, preserving received work for
recovery instead of committing a permanent refusal. Legacy DMARC evidence acquisition
is service-owned; temporary failures remain retryable. Older atomic storage
orchestration is retained where this repair established no behavioral defect.

Schema 37 separates external operation receipts from conversation existence, so a
refused first invitation creates no conversation. It refuses populated predecessor
environments. The offline `tools/development-continuity/external_receipts_upgrade.py`
creates and verifies a fresh copy, including retained maintenance history and
sequence positions, before it can be used. Runtime cache format 3 and original
credentials are unchanged; there is no compatibility reader or reenrollment.

The current owner backend uses the verified `cs_mail_principles_owner_ready_20261013`
copy, with the same original account, personas, address, signing keys, service URL
and Keychain item. Its launcher is `target/principles-alignment-20261013/start.sh`;
its configuration and preservation record are under that directory's `owner-ready/`.
The original `cs_mail_continuity_20261012` database and earlier configuration remain
unchanged rollback evidence. Incomplete earlier copy attempts are retained and were
never started. The canonical app remains
`target/apple/CsMail-continuity-20261012.app` (**cs-mail Development**).

Validation extends existing experiments without adding test cases: exact replay,
transport/refusal distinction, acknowledgement substitution, expired-page traversal,
restored-state corruption/recovery, and external permission routes. The complete gate
and native member journey use isolated local PostgreSQL, fictional browser identities,
simulated payments and the local relay. An initial native run stopped on an unavailable
response; the failure record is retained alongside the successful fresh rerun.
Final gate evidence: `/tmp/cs-mail-principles-finished-quality-gate.log` (all eight
checks passed). Final native journey: `/tmp/cs-mail-principles-native-final-checks.log`.
The maintenance copy and exact rerun matched 101 tables and nine sequences on an
inactive check copy as well as the deployed preservation copy.
The installed app reopened the original address and displayed the external reader
and popup composer with unchecked disclosure consent and a disabled empty send.
Keychain item identity and credential fingerprints matched before and after.
Public/provider delivery, iOS, actual power-loss recovery and interactive external
lane/blocked perimeter inspection remain unestablished by this pass.

## External email in the unified app — 13 October 2026

Communications → External email opens the conversation reader and popup composer.
Choose an external address and explicitly authorize plaintext disclosure to the
relay and mailbox for that conversation. Only authored text is supported; native
quotations, sharing and references are excluded. The development backend uses a
local file relay, so no public email leaves this environment.

Rust owns signed commands, ciphertext preparation, encrypted local journaling,
exact-operation retry and canonical permission projections. Swift renders waiting,
unknown handoff, relay acceptance and provider reports separately. Provider
acceptance does not establish delivery. A lost response preserves the original
command; replacing an unresolved send is refused. Known acknowledgement remains
successful even if the subsequent view refresh fails.

Authenticated replies appear in an unpaid pending invitation. Confirmed Accept,
Reject, a seven-day express lane, Block and revocation use the existing relationship
and external-conversation owners. Disclosure withdrawal cancels waiting work but
cannot guarantee recall after dispatch begins; restoration does not revive cancelled
messages. Paid guest requests remain their existing workflow.

The agreed experiments challenge consent refusal, authenticated pending reply
admission without a paid request, withdrawal/restoration, rejection, explicit new
invitation, acceptance, lane, blocking, encrypted persistence, same-device reopening,
and exact ciphertext replay after response loss. The native harness uses isolated
accounts and the trusted synthetic receiver fixture, not a public authentication
endpoint. The owner deployment was explicitly authorized for a local-relay cutover.
Read-only preservation checks confirmed unchanged original Keychain entry/credential,
account/persona/device-key associations and existing deployment keys. No schema change
or account reenrollment was needed.

Existing PostgreSQL suites challenge email authentication, consent/dispatch
serialization and durable deduplication. Public SMTP/provider callbacks, full visual
and accessibility checks and iOS runtime behavior remain outside this pass.

# cs-mail Apple client

SwiftUI presentation over `cs-mail-client::member`, exported through
`cs-mail-client-ffi` with UniFFI 0.31.0. Domain and identity decisions retain their
existing Rust owners. Minimum development targets: macOS 14 and iOS 17.

Implemented: browser sign-in and first-device enrollment; encrypted device-state
persistence; reopening the same device; inbox/request reading, standing acceptance, rejection and express-lane acceptance;
durable decision and encrypted-send retry; native conversation reading and composing; explicit offline, denied-custody, review and expiry errors.
The interface requires fresh synchronization before enabling decisions.

## One unified application — 13 October 2026

There is one member app, with no demo mode or separate preview/assessment app.
Open `target/apple/CsMail-continuity-20261012.app` (display name **cs-mail Development**).
It runs all implemented native workflows against the preserved local development
account `marcus@csqd-mail.com`. The navigation rail, reactive lists, portal perimeter
feedback, popup composer, request initiation/review, follow-ups, received-conversation
replies, source reuse, contact controls, Directory, approach-page editor and recorded
request finances are available in that app. External-email composition, consent and
reply review now use the portable Rust client. Notes, annual finance interfaces and
replacement-device recovery remain unimplemented.

The transient fixture adapter, demo branches, startup flag and demo-packaging script
are removed. `MemberModel` and `DirectoryModel` share one `ClientWorker`; every action
uses the portable Rust client and canonical service. There are no fake decisions,
transient demo publications, mode switches or substitute readers. Simulated bank
and payment evidence and the persisted fictional participant accounts remain backend
adapters in this development environment, independent of the application interface.

Build and launch only this app:

```sh
DEVELOPER_DIR=/Library/Developer/CommandLineTools \
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk \
  scripts/apple-build.sh
open target/apple/CsMail-continuity-20261012.app
```

The build owns the original credential service, encrypted v3 store, service URL and
bundle identity. It requires stable signing and refuses to replace an existing app
with a different designated signing requirement. The redundant account-build wrapper
and named review-build variants are removed. Existing app bundles were archived and
verified byte-for-byte before separate demo/review/legacy bundles were retired.
`target/apple` now contains exactly one app bundle. Rollback archives are in
`target/apple-rollback-20261013`; restore a checkpoint only with its matching database,
cache and service binary. Original account/identity databases and older caches are
retained unchanged; the original schema-32 app is preserved in its verified archive.

Validation: the full eight-check quality gate including PostgreSQL passed
(`/tmp/cs-mail-unified-quality-gate.log`). The existing native journey passed against
separate test databases (`/tmp/cs-mail-unified-native-checks.log`), covering quotes,
reopening, funding/submission, follow-ups, multiple replies, acceptance, source reuse,
policy controls, revoke/block/unblock, retry/refusal recovery, SQLite and isolated
Keychain checks. No automated test cases were added. The final signed build passed
(`/tmp/cs-mail-unified-install-build.log`). Read-only comparisons preserved the
original Keychain persistent item identity and credential bytes, account/persona
association, public-key records, original caches and signing requirement.

The updated app reopened the original account without enrollment. Native inspection
confirmed the original address and saved Daniel correspondence; New conversation
opened the real composer and its retained-source picker. The empty verification
composer was closed without sending. iOS execution, live providers and a comprehensive
visual/accessibility audit remain outside this pass. Earlier references below to
review apps and offline fixtures are historical validation records, superseded by
this unified application cutover.

## Navigation framework — 13 October 2026

The member application now has an icon rail (Column A), a reactive navigation
list (Column B), and the main communication space. The destinations are Home,
Communications, Relationships, Express lanes, Economics and Notes. Home retains
the membrane overview; Communications searches existing people and opens their
correspondence. Relationships exposes incoming/outgoing request records, Directory,
the member's approach page and the existing saved-request review sheet. Selecting
a contact from a request or lane list keeps that area's rail selection.

The lane list describes incoming permission only; outgoing authority remains
separate in the contact controls. Economics reuses recorded request finances and
Q&A. Annual-service/distribution interfaces and forecasts, drafts/sent overview,
dedicated lane history/editing and a Notes editor are not implemented by this pass.
Their absence is explicit; no synthetic financial estimate or note-save action
is offered. On narrow layouts the rail becomes a row, with the same destinations.

Main-space portals highlight their existing boundary on hover, leaving their
interiors unchanged and preserving standing gaps and bounded-lane markers. The
entire portal remains clickable. Contact-list, conversation, Directory and
request-class rows retain whole-region shading and matching hit regions.
The rail previews the same navigation component
used in Column B, after a 250 ms initial pointer dwell; an already-open preview
switches immediately when another icon is hovered. A 220 ms dismissal grace period
allows movement into the scrollable preview. Pointer-opened previews stay open
only while the pointer is over an icon or the visible popup. Explicitly
keyboard-opened previews also retain the matching rail or popup focus.
Tab reaches the rail; Right Arrow opens a preview,
and Escape or Close dismisses it. Previewing does not change the selected area or
main space. Activating a preview item performs ordinary navigation. Keyboard,
accessibility actions and ordinary click navigation provide alternatives to hover.

One typed Swift destination owns the selected area and detail route. Swift owns
search, focus, preview and navigation state. Rust continues to provide permission,
request, lane, financial and retry meaning; no domain rule, schema, client journal
or credential operation changed. The correspondence view remains mounted while
another destination is shown, preserving its reading and popup-composer context.
Locking still removes the session and its transient readable content. Hover uses
existing summaries and initiates no decryption, funding, grant or send.

The agreed validation scope uses existing suites and native journeys, with no
new automated test cases. All eight quality-gate checks passed, including isolated
PostgreSQL suites (`/tmp/cs-mail-navigation-quality-gate.log`). The existing native
journey passed against a separate fictional deployment
(`/tmp/cs-mail-navigation-native-checks.log`), including exact request retries,
same-device reopening, encrypted persistence and isolated Keychain checks. Signed
macOS packaging passed (`/tmp/cs-mail-navigation-owner-build.log`). The earlier
build/gate attempt failed because the compiler had no Apple SDK path; the successful
runs supply `SDKROOT` explicitly and use Command Line Tools.

The isolated demo established three-slice layout, conversation search, Directory
routing, selected-contact/search retention, accessible previews, preview-item
navigation and Escape dismissal. The final demo starts without an unsolicited
preview. Tab followed by Right Arrow opened Communications' preview while Home
remained selected; search within that preview retained keyboard focus, and Escape
dismissed it without navigating. Pointer dwell/cross-gap timing was not established
through the available native automation. iOS execution, interactive expiry,
an exhaustive focus/clipboard/accessibility audit and composer retention under
owner-account navigation are not established by the native harness.

The temporary review bundles used for this pass have been retired. The unified
application section above records the current app and preservation checks.

### Hover correction — 13 October 2026

Marcus reported that hovering Home could show Communications and that leaving
the icon/popup did not dismiss it. Keyboard focus could overwrite the pointer's
destination, while popup tracking included transparent padding across the rail.
The correction gives pointer and keyboard previews distinct typed sessions and
one owner. One pointer observation now selects from measured icon and visible-popup
bounds in a shared coordinate space; transparent positioning padding is excluded.
Incidental rail or text-field focus cannot pin or rename a pointer-opened preview.
Previewing still performs no Rust operation or authority change.

The two challenged claims were that Home's visible hover region selects Home
regardless of earlier focus, and that leaving both icon and popup dismisses a
pointer-opened preview. The corrected offline demo was rebuilt and reopened;
Marcus confirmed both behaviors work. Exact dwell/cross-gap timing and all-icon
stress traversal were not separately measured. No automated test cases were added.
All eight quality-gate checks passed, including isolated PostgreSQL suites
(`/tmp/cs-mail-hover-fix-quality-gate.log`). The signed macOS build passed
(`/tmp/cs-mail-hover-fix-build.log`); candidate signing requirement and account
configuration still match the preserved owner app. The running owner app was not
closed or replaced, and iOS remains untested.

### Portal perimeter feedback — 13 October 2026

Main-space portal hover strengthens only the supplied membrane stroke. Its interior
has no hover fill; standing openings and bounded-lane markers retain their existing
shape. Home and the relationship/lane overview cards share this portal style.
The full card remains clickable, while its decorative strokes ignore hit testing.
Column A/B and ordinary list rows retain their region shading. No Rust, permission,
operation or persistence behavior changed, and no automated test cases were added.

The signed macOS build and all eight quality-gate checks passed, including isolated
PostgreSQL suites (`/tmp/cs-mail-portal-perimeter-build.log` and
`/tmp/cs-mail-portal-perimeter-quality-gate.log`). The refreshed offline demo opened;
portal activation still opened the selected correspondence, and returning to Home
restored the overview. Candidate signing/account configuration still matches the
original owner app. Full light/dark hover and iOS audits were not performed. The
owner app was not closed or replaced.

## Principles repair and received-conversation replies — 13 October 2026

Every received native conversation now has a confined reply route, independent
of request origin and broader reverse-lane expiry. Starting another conversation
still requires separate permission. Conversation receipt evidence and a distinct
relationship reply-withdrawal epoch replace per-request answer permits. Blocking
or revocation advances that epoch; unblocking alone does not revive old offers.
A request's broader reverse-lane consent is separate immutable submission evidence,
so later receipt of a message cannot silently restore withdrawn broader consent.

Both request and ordinary publication declare receipt evidence with mailbox
publication in one atomic consequence. Request delivery distinguishes creating a
conversation from updating an existing one. Unchanged opposite directions have
no invented transition or journal entry. The native source picker explicitly
loads retained message detail for the selected source contact. Same-contact
navigation, request formation and cancellation reload current correspondence.

Protocol outcomes have one typed application owner; the client verifies their
commitment against the signed receipt. Billing refusals are durable outcomes and
native billing responses bind the exact command and outcome under the provider's
signature. Definite refusals resolve setup journals; transport uncertainty retains
exact bytes. The superseded standalone native class-publication workflow is
removed in favor of atomic address publication. Request finances distinguish
pending, finalized, voided and reversed funding from the authorized amount.

Schema 36 and device-cache format 3 are explicit cutovers. Ordinary migrations
require a fresh environment. The preservation-checked maintenance tool upgrades
Marcus's existing database and a separate encrypted cache copy, retaining the
original Keychain entry, account/persona association and address. It records reply
receipt corrections, preserves withdrawals and financial history, and never
changes historical agreement to a broader reverse lane. Prepared ciphertext and
its durable intent survive uncertain commit acknowledgements and can be reconciled
without re-enrollment or a second charge. Runtime has only the format-3 reader.

Agreed checks challenge receipt substitution, refusal replay after policy change,
source-picker discovery through the actual presentation model, same-person reload,
failed funding, original-conversation replies and interrupted conversion recovery.
All eight local quality-gate checks passed, including PostgreSQL
(`/tmp/cs-mail-repair-clean-quality-gate.log`). The native journey passed
(`/tmp/cs-mail-repair6-native-checks.log`), including exact Unicode selections,
source discovery, same-contact reload, setup refusal and voided funding.
Standalone maintenance-tool tests, clippy and build passed. An injected lost
commit acknowledgement retained and reconciled the encrypted prepared cache.

The original account upgrade and signed app installation are complete. A fresh
backup comparison preserved account/persona records, endpoint keys, requests,
billing, request finances and ledger records. The original v2 cache and Keychain
credential remain unchanged; the new separate store uses v3. The signing
requirement matches the previous app. All four original fictional peer endpoints
decrypted exact verification replies from the original owner identity. Native
inspection confirmed the original address, updated synchronization, and Reply in
Elena's retained conversation and Maya's expired original conversation.
The contact overview distinguishes outgoing initiation permission from permission
the contact has to approach the member. Standing permission and active outgoing
lanes offer New conversation; otherwise Request a relationship opens the selected
Directory profile. Received-conversation replies remain separate. Requests retain
published-approach selection and signed quote review.
Native clipboard/focus interaction, full visual layout, iOS, real providers
and replacement-device recovery remain separate evidence.

## Pending incoming request entry — 13 October 2026

Contact and Directory actions use one Rust `RequestEntry` projection over the
authenticated pair. A received request still within its decision window projects
IncomingPending; the contact overview offers Review request instead of Request a
relationship, preserving Reply in the existing conversation. A pending outgoing
request, existing initiation permission, unavailable direction and own address
have distinct alternatives. Public Directory search results are Unverified;
profile lookup independently obtains the private authenticated request state.
A published class is an invitation, not eligibility or charge authorization.

The projection reuses `RequestLifecycle::awaits_decision_at`, the same rule used by
paired preparation/submission. It passes time explicitly; Swift does no deadline
comparison. Passed decision windows no longer appear as awaiting a decision even
when expiry recording is delayed. A later quote/formation still rechecks the
protected pair, history, class and policy before creating payment work.

Agreed checks extend the existing native journey to challenge incoming/outgoing
classification, Directory consistency, terminal-state release and retained replies.
Native inspection confirmed Maya's pending notice and Review request, the existing
request's decision controls and Reply, and no Directory request action after class
selection. The first native harness run exposed a self-profile lookup constructing
a self relationship; OwnAddress now preserves public profile access without that
pair read. The fresh native journey passed (`/tmp/cs-mail-request-entry3-native.log`), including
incoming/outgoing and Directory classification, terminal-state release and retained
replies. Its Directory check publishes its fictional sender's public profile before
lookup; an unpublished profile is correctly unavailable. All eight quality-gate checks passed (`/tmp/cs-mail-request-entry-quality-gate.log`),
as did the signed build and signature verification. Final native inspection again
confirmed Maya's pending notice, Review request and retained conversation access.
Expiry uses the existing domain decision-window
experiment; no interactive wait across expiry or iOS execution was performed.

The preserved local backend now runs independently from an immutable binary copy,
using the same database, configuration and keys. Its restart script also pins each
built binary so rebuilding the repository cannot replace its executable in place.
The earlier isolated fixture and its data are retained; no owner account was reset.

## Preserved acceptance alignment — 13 October 2026

The original accepted Daniel and Elena development samples were converted without
reverse-lane agreement. The explicit maintenance correction obtains fresh signed
consent from each original fictional recipient key, while retaining the historical
request terms and acceptance evidence. Its current policy, immutable source
request and acceptance event define the correction window. Expiry remains original
acceptance time plus the policy lifetime; the correction does not restart it.

The application owns the deterministic correction decision. The storage adapter
protects both directions and current authority, then uses the ordinary lane
transition and receipt persistence. Effects, receipt and correction provenance
commit in one transaction. Exact prepared commands survive unknown acknowledgements;
a changed retry is refused. Later withdrawals are compared by canonical event
order. Current stronger permission, later explicit grants, expired windows and
unresolved reverse requests are preserved. The maintenance API is compiled only
with `development-maintenance`; it is not a runtime fallback or a new app endpoint.

The original account correction was applied after rehearsal. Account/persona,
operational/content keys, requests, billing, request finances and ledger records
compared unchanged. The original Keychain helper verified credential continuity.
Both corrections applied once; Maya and Jules were preserved because their
original requests were not accepted. Rehearsal retained the exact correction
through simulated lost acknowledgement, replayed without another grant, and
refused resurrection after signed block/unblock and an altered retry. Evidence is
in `target/acceptance-alignment-20261013`. Actual power loss and iOS are not covered.

The sidebar quietly labels its groups Who can contact you. Manage contact
separates the member's outgoing route from permission they grant to the contact.
Inactive incoming lanes no longer appear as current express-lane permission.
The signed macOS build and signature verification passed. All eight final gate
checks passed (`/tmp/cs-mail-acceptance-alignment-final2-quality-gate.log`). Native
inspection confirmed the original address, Who can contact you, Daniel's New
conversation action, a fresh empty composer, and the two directions in Manage
contact. The validation composer was closed without sending. Exact original-account
replay reported both corrections as replayed with no second grant. Original
signing requirement, Keychain credential and predecessor encrypted cache remain
preserved. iOS and a comprehensive visual/accessibility audit remain untested.

## Contact actions and recipient expiry reminder — 13 October 2026

Rust projects the outgoing route for New conversation; Swift does not interpret
incoming standing permission as permission to send. The contact header explicitly
labels permission for contact from the other person. Request a relationship opens
that person's published approaches before quote review. Profile lookup finishes
before Directory navigation, and initial result loading cannot replace a profile
or erase its lookup failure.

A typed Rust projection supplies a recipient-side expiry reminder only for a
current lane within the confirmed three-day window. The effective expiry uses an
expiring lane's horizon and a persistent lane's final validity. It is evaluated
from explicit lane evidence and time on synchronization/contact reads, with no
permission mutation. The sidebar offers a review entry; the contact view offers
Upgrade to standing permission and Let it expire. Upgrade uses the existing
recipient-signed standing transition after confirmation. Let it expire merely
acknowledges the reminder for that open view; the lane's schedule remains unchanged.
No operating-system notification or background reminder service is established.

The existing native journey was extended (no new test cases) to challenge a
recipient-only two-day reminder, no seven-day reminder, the standing upgrade,
and unchanged reverse permission. It passed on an isolated environment
(`/tmp/cs-mail-contact-actions-native.log`). All eight quality-gate checks passed
(`/tmp/cs-mail-contact-actions-quality-gate.log`). Native inspection confirmed direct profile routing, approach selection, signed
quote review remaining presented through journal updates, discarding the uncharged
quote, and Reply in the existing conversation. Quote presentation is now owned by
the main member view rather than a Directory row whose lifetime can end on update.
The saved-workflow review retains Done even after the quote is discarded. No
validation charge was authorized. iOS and interactive near-expiry reminder layout
are not established. The preserved owner fixtures do not establish outgoing
standing/lane permission: native checks used isolated two-party actors for those
routes; the header distinguishes incoming permission from the outgoing action.

## Appearance control restoration — 13 October 2026

The shared member view exposes a toolbar button to switch between light and dark
appearance. Before the first explicit choice it follows the system appearance;
once selected, `csMailAppearance` in the app's preferences retains the choice
across account locking, demo changes and reopening. Sheets inherit the window's
appearance. This is local presentation preference, with no Rust domain rule or
credential change. No automated test cases were added.

Validation scope: switch both ways in the signed macOS app, reopen to challenge
persistence, and run the existing full quality gate. iOS execution is not covered.
The signed macOS build and all eight quality-gate checks passed
(`/tmp/cs-mail-appearance-quality-gate.log`). The installed app was observed
switching to Light and back to Dark. After a confirmed process exit and reopening,
Dark remained selected and the toolbar offered Light. The original designated
signing requirement and account association are retained. iOS was not run.

## Native member workflows — 12 October 2026

The current signed continuity app now exposes native paid requests from the
Directory, explicit signed-quote review before charge authorization, encrypted
formation/submission retry, cancellation of unsubmitted preparation, and follow-ups
under the original request. No-charge and existing-request quote outcomes are
separate from refusal. Expired quotes are reported explicitly. Funding waits return
to the interface rather than polling. The staging endpoint returns the Rust
application's durable document receipt, replacing the former unbound `stored` flag.

Contact management exposes standing permission, revocation, blocking/unblocking,
a personal/human-initiated express-lane preset (20 messages per day, with reviewed
lifetime), and exact follow-up limits. Lane regrants retain the lane identity and
advance its version. Request status and current request money are available to the
sender; recorded money appears separately in Q&A, outside recipient decisions.

Authenticated native mutation refusals now commit a terminal operation receipt.
Exact replay remains refused even after permission/policy changes. The portable
client checks its operation/digest, resolves only acknowledged work, and retains
uncertain transport outcomes. Refused sends no longer prevent synchronization.
Conversation-policy changes have their own durable journal. Both participants must
approve relaxation at the reviewed revision; restriction cancels a proposal.

Each conversation projects its own confined answer permission and received/sent
request linkage. Selecting a thread scopes its request controls, and decision
callbacks name that exact displayed request. Older rejected/expired conversations
remain answerable when their original permits remain active. Historical absence of broader-lane terms does not remove confined replies;
conversation receipt evidence now supplies those replies.

Source inclusion is explicit: quotations, retained shared copies and references
are assembled by Rust from exact selections, retain provenance, and are checked
against current ancestry policy at commit. References can be opened without
treating their pointers as access grants. The composer-led passage interface below
replaces the former whole-message interface.

## Conversation navigation and exact source selection — 13 October 2026

Opening a person now shows their conversations directly, with content-derived
labels, previews and dates. Opening a row scopes reading and request decisions to
that conversation; there is no “Conversation” picker or combined-history composer.
Reply appears only for a currently projected answer/standing/lane route. Otherwise
the reader explains that replying is unavailable. Starting a new conversation is
separate from starting a request, which opens the recipient's Directory profile
and signed-quote review. Contact controls remain under Manage contact.

Reply and New conversation open an outgoing composer with a named destination.
Include an existing message opens an authenticated retained source in a resizable
panel beside that composer on macOS. Its read-only native text view supports normal
highlighting and copying. Swift maps UTF-16 highlights to ordered UTF-8 ranges in
the source's exact flattened content blocks. A selection can cross text blocks.
Images can be selected as whole blocks; embedded reference markers cannot be
selected, and crossing them is refused rather than silently dropping content.
Quote selection, Reference selection and Share selected copy add removable previews
with source attribution to that draft only. There is no source-selection state
carried between contacts and no reuse menu in the reading view. Closing an unsent
draft asks before discarding its in-memory content; locking removes it.

Rust exports the authenticated source's immutable version and selectable blocks
in memory. The bridge now requires that version and explicit elements; the old
whole-message constructor and unsupported-whole-message error are removed.
Rust validates target version, block bounds and UTF-8 boundaries for references
as well as retained copies before journaling a send. Domain policy and live
permission are still rechecked at the atomic commit boundary. Plain copying and
pasting authored text does not manufacture quotation provenance.

Agreed validation adapts the existing native journey to challenge exact Unicode
excerpt preservation, cross-block mapping, exclusion of surrounding text and
refusal of out-of-bounds, substituted-version and split-UTF-8 selections. No new
test cases were introduced. The corrected native journey, signed macOS build and
all eight final quality-gate checks (including PostgreSQL) passed. Preservation
checks retained the original credential/account/address, cache and signing
requirement; the previous app and validation logs are saved in
`target/native-selection-owner-upgrade-20261013`. The first run stopped on a test
helper attempting to match a component of a compound emoji; that helper was
corrected and the full journey passed on a fresh isolated environment.
Interactive visual behavior, image-selection runtime,
iOS, native-view focus/clipboard interaction and response-loss/crash injection are
outside this evidence. The new source reader does render retained images;
ordinary conversation reading still uses image placeholders. Image authoring
remains separate work.

Validation reused the existing native enrollment check sequence, extended for the
consequential new member journeys; no new test cases were added. Two fresh fictional
members exercised quote review without a charge, same-device reopening, finalized
funding and native submission, follow-up, answers after rejection and after a later
request, acceptance, quotation/share/reference resolution, no-charge quoting,
two-party policy relaxation, revoke/block/unblock, lane regrant, refusal recovery,
and plaintext exclusion from both encrypted SQLite caches. The complete quality
gate, including PostgreSQL, and signed macOS build passed. The first isolated run
exposed the discarded staging receipt; its database/state were preserved and the
repaired journey passed on fresh isolated environments.

The original credential was read only through the preservation helper's anonymous
pipe. Checks retain its bytes, account/persona association, registered public keys,
`marcus@csqd-mail.com`, readable sample conversations and original signing
requirement. The current encrypted cache was unchanged during the upgrade checks.
A rollback copy of the prior continuity app/cache/database is in
`target/native-owner-upgrade-20261012`; the earlier schema-32 checkpoint remains
untouched. The same schema-34 backend/configuration and keys are reused.

Interactive inspection was blocked by an app-control timeout. Reopen
`target/apple/CsMail-continuity-20261012.app` to load the rebuilt executable;
a previously running process still contains the earlier build. iOS runtime,
replacement-device recovery, annual billing/distribution interfaces, external-email
composition/consent UI, crash/response-loss injection in the new native facade,
and live providers were not exercised in this pass. Existing Rust/PostgreSQL
replay, concurrency, expiry, consent and rollback suites remain separate evidence.

## Current owner app — 12 October 2026

The recipient request screens and decision confirmations show communication
permission rather than charge/refund breakdowns. A separate Q&A sidebar section
explains charges, refunds, expiry, cancellation and the shared member pool in
plain language. This is presentation only; Rust settlement and financial records
are unchanged. The signed macOS build and full Rust/PostgreSQL gate passed,
and original-credential account/conversation checks passed. Interactive visual
verification of this Q&A change was blocked by app-control timeouts; iOS layout
was not exercised.

Open `target/apple/CsMail-continuity-20261012.app`. The explicit development
conversion restored a separate candidate from the original account backup and
verified Marcus's original account/persona IDs, registered keys, address and
Keychain credential. The four original sample conversations are now current
native documents with matching message text and original decisions/permissions.
The original schema-32 databases, signed app and encrypted device cache remain
unchanged for rollback. This is the same account, not a fresh enrollment.

The current app uses `com.csqd.member.development` for Keychain and stable signing
identity `com.csqd.member.dev`, but keeps its converted cache separately in
`target/apple-continuity-state-20261012/device`. The backend listens on 9693 using
`cs_mail_continuity_20261012`; restart with the `start.sh` in that state directory.
Do not open the saved old `CsMail.app` against the current backend.

A new, clearly labelled Maya request exercises agreed native reply permission.
The app sent a pending answer and Maya's endpoint decrypted it; the request is
left awaiting Marcus's decision. The original Maya request expired under its
original deadline and remains readable. Original samples did not offer the new
reply consent, so their conversion does not grant new reply permission. Accepting
new requests activates the agreed reverse lane. Paid request initiation, revocation/blocking and source-reuse controls are now
implemented in the native member workflow slice described above.

The full Rust/PostgreSQL gate, signed macOS build, original-credential live reads,
plaintext comparisons and transaction rollback passed. See the standalone
[development conversion record](../../tools/development-continuity/README.md)
for exact preservation checks and limitations. iOS and replacement-device
recovery remain untested.

## Current baseline — development paused 11 October 2026

The 12 October native messaging cutover requires schema 34 and device-cache format
2 in a fresh, separate environment. Migration 0034 refuses populated earlier
schemas; the preserved schema-32 environment below remains an earlier checkpoint.
Keep its databases, deployment keys, state directory, signed app, Keychain
credential and Application Support store. Current binaries do not upgrade that
checkpoint or read its old native request content/cache. Use new empty databases,
state directory, ports, client-store directory and credential-service name for
cutover validation. Retaining the old Keychain bytes does not establish authority
in a new database; no credential or account is silently recreated.

Native requests now publish ordinary documents in one restricted conversation.
The contact card lists conversations with labels derived in memory, and provides
an encrypted send journal, composing and retry. A recipient can answer the
request conversation before deciding, after rejection or after expiry. Acceptance
activates frozen reverse-lane terms while preserving an existing custom lane.
Rust owns all admission and settlement; Swift selects the conversation and
presents the outcome. Paid request initiation uses the CLI; source reuse is available through Rust
correspondence APIs. Those UI workflows are not implemented here.
This section describes a historical isolated backend experiment. Its named app
bundles are retired. Use the single unified app described above for assessment;
isolated service and CLI/native-harness experiments do not create another member app.

Validation of this cutover: the complete Rust quality gate passed (112 workspace
checks and 104 PostgreSQL experiments), along with the HTTP/CLI demonstration
of pending answers, follow-ups, reverse-lane sends and settlement. The portable
client experiment also challenged an acknowledged send followed by a failed read,
so a refresh failure cannot be presented as a failed delivery. The macOS/FFI build
and native SQLite/isolated-Keychain checks passed. Browser enrollment was skipped
in that native harness; an interactive Swift messaging journey, iOS execution and
replacement-device recovery were not validated in this pass. These results use
separate test environments, not the saved owner account.

### Fresh messaging environment — 12 October 2026

The separate persistent backend is initialized at schema 34, with its own keys,
member and identity databases, and controlled browser provider. Its service is
`http://127.0.0.1:8580`; identity and browser-provider ports are 8581 and 8582.
State is in `target/apple-messaging-state-20261012`. Its signed app is
`target/apple/CsMail-messaging-20261012.app`, with bundle identifier
`com.csqd.member.dev.messaging-20261012` and client/Keychain namespace
`com.csqd.member.development.messaging-20261012`.

The fresh app completed browser sign-in using Marcus's fictional development
evidence and the controlled Marcus login. Its newly enrolled account has claimed
`marcus@csqd-mail.com` through the signed address-publication workflow. That address
is the owner's fixed development address across environments. No old account,
credential or contacts were imported; the original account remains separate and
preserved. The fresh profile currently has no paid request classes or simulated
service purchase. Those workflows are separate from claiming the address.

If the backend is stopped, restart it with:

```sh
target/apple-messaging-state-20261012/start.sh
```

The launcher refuses concurrent ownership and changed settings; reopening retains
this environment's keys and journals. The saved schema-32 databases, owner app and
credential namespace remain unchanged.

The normal signed app reopens Marcus's persistent development account at
`marcus@csqd-mail.com`. Communication space, Directory and My approach page share
one app and navigation model. The profile is currently listed with one introduction
class; up to eight classes may be published per address. Standing contacts and
express lanes are visually separate. Directory profiles have Back navigation,
readable text supports native Copy/Paste, and clicking the selected request class
clears its selection. The earlier offline fixture mode was retired in the unified
application cutover; current screens use account data.

The preserved personal environment contains six enrolled accounts: the owner and
five independently credentialed fictional participants. Maya was left awaiting a
decision; Daniel has standing permission; Elena has a seven-day express lane;
Jules was rejected; Noah's profile is visible but new requests are unavailable.
These are dated observations: request deadlines, service periods and lane expiry
advance with real time. On resuming, refresh canonical projections rather than
expect these categories indefinitely.

Identity evidence, banking and payments are simulated. The claimed mailbox neither
verifies a real external inbox nor enables public email delivery. Validation and
recovery observations are recorded under **Persistent owner-account cutover** below.
Earlier validation sections are historical records, not current account status.

The restart command below is historical: resuming that checkpoint requires its
matching schema-32 server and saved app, rather than current cutover binaries.
The saved app reopens its device without browser login. Preserve the
two databases, `target/apple-local-state` (including participant credentials),
`target/apple-signing`, the app's Keychain credential and Application Support store.
Do not reset storage or re-enroll to recover from custody or connectivity failure.
Quit the app before replacing its bundle with a rebuilt version. These same-device
observations do not establish backup or replacement-device recovery. This
documentation pause does not shut down the app or local services.

## Cancellation and Swift concurrency — 11 October 2026

The package and iOS target now use Swift 6 language mode. Queued client work checks
task cancellation and presentation lifetime before invoking Rust; late results
cannot reopen a locked or removed member view. Already-started Rust workflows
retain their existing durable operation and reconciliation semantics. Stopping
the UI wait is distinct from cancelling a domain request. Directory cancellation
preserves the prior screen, and delayed browser cleanup is scoped to its original
ceremony. SQLite's unchecked Sendable ownership was reviewed and documented; the
native harness vault's custody-denial flag now uses its existing lock.

Validation used existing checks only, with no new tests. The macOS Swift 6 build
passed (`/tmp/cs-mail-cancellation-build.log`), native encrypted SQLite replacement,
exclusive ownership and isolated Keychain checks passed
(`/tmp/cs-mail-cancellation-native.log`), and all eight quality-gate checks passed,
including PostgreSQL (`/tmp/cs-mail-cancellation-gate.log`). The initial sandboxed
gate failed on local socket permissions; the successful run had local test access.
Browser enrollment was explicitly skipped in the native harness. Native UI
cancellation/lock races, browser interaction and iOS builds/runtime were not
exercised. Full Xcode currently requires license acceptance; macOS validation used
the installed Command Line Tools. This pass did not replace the running signed
app or alter the preserved account environment.

## Visual development

The first visual direction draws on Cereal's editorial typography and warm
off-white palette, and John Pawson's proportions and use of space. Helvetica Neue
Light headings and regular Helvetica Neue controls/message text follow the
owner's preference for sans serif typography. Headings use a restrained scale,
labels use sentence case and ordinary spacing, and the light canvas is #F7F6EF.
The shared `CsMailPresentation` views provide the inbox, request reading area,
decision controls and empty state; the real member view supplies canonical facts
and retains the existing authenticated decision and confirmation workflows.
Dark appearance has its own palette. Inspection of the live reference pages found
NeueHaasUnicaPro-Regular for Cereal's sans-serif controls and
UniversNextPro-Light for Pawson's navigation and heading. Neither family is
installed here. The current typeface is an available approximation; those sites'
font files have not been copied or licensed for the app. The lighter weights,
smaller hierarchy and consistent family replace the initial generic sans-serif
treatment, without copying the sites' very small text sizes into reading surfaces.

The communication-space view now makes the membrane visible: a thin enclosing
boundary contains standing relationships and bounded express lanes. Pending
approaches without such permission sit outside it; closed/revoked history sits
outside as well. A pending request on a permitted relationship does not displace
that relationship from the interior. Each relationship portal has a continuous
closed outline, a broad opening for standing permission, or a narrow passage
with two boundary marks for an express lane. Text labels carry the same meaning,
so access is not communicated through geometry alone.

Rust's existing client projection now supplies a typed `PermissionRoute` alongside
the permission label. Both come from one match over the canonical relationship
state, and the narrow Swift binding forwards that type. Swift receives the route;
it does not infer membership from labels. This describes the recorded arrangement,
not current authorization to send: lane scope, expiry and allowance checks stay
with their existing owners. Cached views remain snapshots.

Selecting a visible correspondent card expands its boundary into their framed
portal over a short, 0.42-second transition; returning contracts it into the
communication space. Sidebar navigation remains available. Both the native member
app and preview use the same `EditorialSpaceView` and geometry namespace. "Read request" or
"Open correspondence" expands that frame into the reading surface with a short
local transition. System reduced-motion preferences disable motion; the preview
also exposes a local motion toggle. Opening and returning change only navigation;
both surfaces remain mounted to retain reading position. Reading never opens a
permission route. A return control leads back to the communication space, and
the sidebar supplies a familiar alternative to spatial navigation. Separate
conversation selection and side-by-side sources await richer client projections.

During an operation the member view disables correspondent selection. It also
requires the displayed conversation and selected sender to match before showing
a portal or executing a decision, preventing stale correspondence from supplying
the context for a different selection. These are presentation safeguards; no
permission or settlement rule changed.

The standalone design-preview executable has been retired. Visual review now uses
the member executable with one local-demo account, its real encrypted inbox, and
visible sidebar destinations for Communication space, Directory and My approach page.
The shared editorial presentation remains the member application's presentation.

Validation scope for this presentation slice: compile the shared views and native
preview, inspect layout and interactions, parse the member view, and attempt the
existing quality gate. No new tests or domain-rule changes. Full backend journeys and
iOS/device behavior require separate validation. Earlier presentation passes were
blocked because the sibling identity checkout lacked `identity-contract` and did
not match `identity-source.sha256`. Pulling identity-model from GitHub on 10 October
restored all pinned sources without changing the manifest. PostgreSQL suites were
explicitly skipped in those earlier passes; they provide no evidence from those runs.

Observed on 10 October 2026: the native preview compiled and launched; visual
inspection covered light and dark appearance. Conversation selection, the
external-email disclosure label, an established conversation without decision
buttons, accept/reject explanations and the empty inbox were exercised through
the native UI. Swift parsing and whitespace checks passed. Full member-view
type checking, live decisions, resizing/accessibility audits and iOS behavior
remain unverified.

The subsequent sans-serif and membrane/portal pass also compiled and launched.
Native inspection confirmed separate Requests/Relationships headings, explicit
standing/lane/revoked labels, overview-to-reader navigation, and the same scroll
position after returning and reopening. A revoked example retained its historical
message without decision buttons; an external request retained its disclosure
label. The empty inbox was exercised again. These are presentation observations
over fictional content, not new evidence for server authority or settlement.
At that stage, the existing quality gate was attempted again with PostgreSQL
explicitly skipped and remained blocked by missing/mismatched identity-model sources.

The spatial pass compiled the shared native presentation without warnings and
launched the membrane view. Native inspection covered closed approach portals,
standing openings, bounded passage marks, overview/reader navigation and the
preview reduced-motion path. Swift parsing, direct Rust formatting checks and
whitespace checks passed. At that stage, the Rust client projection/FFI additions
and the full member integration had not compiled against identity-model in this checkout;
the repository gate remained blocked. No new PostgreSQL evidence, accessibility
audit, lane-expiry experiment or iOS runtime evidence is claimed.

The subsequent card-to-portal pass restored identity-model by a fast-forward pull
to `f530e8c`, with all pinned hashes matching. The full Rust bridge, generated
Swift bindings, shared presentation, member view and native app compiled, and
the macOS app launched. The existing native harness passed SQLite durability and
exclusivity, isolated Keychain creation/reopen/refusal, browser enrollment through
the local identity service, encrypted-cache reopening and denied custody checks.
Native preview interactions covered standing and bounded portal navigation,
the reduced-motion path, and a closed request remaining closed while reading.
Reading position survived overview/reader navigation. No tests were added and
no domain permission or settlement rules changed. Live request decisions through
SwiftUI, a full accessibility audit, lane-expiry behavior and iOS runtime behavior
remain outside this pass's evidence.

The full repository quality gate subsequently passed all eight checks, including
the existing workspace and PostgreSQL suites, with
`CS_MAIL_TEST_DATABASE_URL=postgresql://localhost/cs_mail_portal_gate_1010`.
The controlled enrollment journey used separate fresh member and identity
databases (`cs_mail_portal_member_1010` and `cs_mail_portal_identity_1010`), fictional
browser identities and simulated banking. This establishes local integration
evidence; it does not establish real-provider assurance or release readiness.

## Build and run

```sh
scripts/apple-build.sh
open target/apple/CsMail.app
```

The script generates bindings, builds the native static library and Swift package,
and produces the unified development app. Once a signing identity is selected,
subsequent builds reuse it; otherwise the bundle permits visual review only. Generated binding files are ignored
and regenerated from the Rust interface. The Mac app uses a single window and a
client worker actor; blocking network work never runs on the UI actor.

For the persistent local backend, the **first initialization** requires two empty
local PostgreSQL databases. Later starts use the same databases and the same state
directory; they neither reset the databases nor regenerate keys. For example:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/apple_mail \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/apple_identity \
  scripts/apple-local.sh
```

The state directory defaults to `target/apple-local-state`; set
`CS_MAIL_LOCAL_STATE_DIR` for a separate validation environment. Database URLs,
ports and directory identity are bound in a private manifest. Changing any of them
requires a separate environment. Initialization refuses nonempty databases,
missing durable keys, or occupied ports. An advisory lock excludes a second
launcher; shutdown waits for companion processes before releasing ownership.
The cs-mail and identity databases, simulator records, deployment/evidence keys,
and OIDC signing key persist. OAuth codes and browser ceremonies remain transient.
Operator bootstrap and configuration use existing idempotent Rust commands.

The default ports are 8480–8482. The prepared personal development environment
uses 9693–9695, databases `cs_mail_personal_development_1011` and
`cs_mail_personal_identity_development_1011`, and the default state directory.
Restart it with:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/cs_mail_personal_development_1011 \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/cs_mail_personal_identity_development_1011 \
CS_MAIL_DEMO_PORT=9693 CS_MAIL_DEMO_IDENTITY_PORT=9694 CS_MAIL_DEMO_OIDC_PORT=9695 \
  scripts/apple-local.sh
```

The controlled browser provider offers Marcus plus fictional participants Maya,
Daniel, Elena, Jules, Noah, Alice and Bob. These are local fixture logins, not
real-user authentication. Every account must still enroll through the existing
identity application with distinct fictional evidence and simulated banking.
Naming a fixture does not create an account, relationship or financial effect.
There is no hosted service or external email in this environment.

Select an existing code-signing identity once, then build:

```sh
scripts/apple-signing-setup.sh
scripts/apple-build.sh
```

The selection script reads existing valid identities in Keychain and records only
the selected certificate fingerprint. It changes no certificate trust. Set
`CS_MAIL_APPLE_SIGNING_IDENTITY` when more than one identity is available.
Account packaging refuses ad hoc signing. The bundle is signed and verified before
replacing the previous bundle. The app uses the preserved development service at
`http://127.0.0.1:9693`. The build keeps the original account configuration;
it does not import or replace a device credential.

The selected signer on this Mac is the existing Apple Development identity. The
signed app automatically reopens saved device state and synchronizes; offline
failure preserves a visibly dated snapshot. Unfinished enrollment reconciles its
existing operation. Custody failures never replace the stored credential or make
another account. Browser cancellation fences callbacks from an older ceremony.
The app shows that banking and payments are simulated. It has one account-backed
execution path and no separate demo mode.

The persistent owner account is enrolled and has claimed `marcus@csqd-mail.com`
through the ordinary signed address publication workflow. Its public name is
Marcus; all enrollment evidence is fictional and service/payment activity is
simulated. The app reopens this account as its normal experience. The development
profile starts listed and contains one illustrative introduction class with 500
minor units collateral plus the operator's 50-unit processing component.

This Mac uses Chrome for browser login. Set `CS_MAIL_LOGIN_BROWSER_BUNDLE_ID` when
building to select an installed browser; the choice is retained under
`target/apple-signing/login-browser.txt`. Without a saved choice, macOS uses its
system authentication browser. The development app has its own callback scheme,
`com.csqd.member.development`, separate from older preview bundles. The controlled
provider presents an explicit return link so Chrome can return to the signed app
without weakening the login-page security policy. Rust still checks issuer,
state, nonce, PKCE and enrollment possession.

Fictional counterparts have separate development credentials and use the normal
signed CLI workflows. Provision or resume them with:

```sh
cargo build -p cs-mail-cli
python3 scripts/apple-local/provision-participants.py \
  --state-dir target/apple-local-state --recipient marcus@csqd-mail.com
```

The script binds the participant stores to this recipient, excludes concurrent
provisioners, waits for canonical coverage, and preserves journals on uncertain
outcomes. It does not recreate completed requests on rerun. The CLI fixture keys
are local development files protected by directory/file permissions; the owner's
credential remains in Keychain and is never imported into these stores. All
canonical effects pass through signed services; no database population bypasses
authority or settlement. The current app contains Maya's pending request, Daniel's
standing contact, Elena's seven-day personal/human-initiated express lane (20
messages/day), and Jules's rejected approach. Noah's public profile remains
visible with new requests unavailable because he has no purchased service.

A terms refusal creates no request or charge. The CLI retains that refused
operation's journal and retires only its unfinished approach plan, so a later
approach can start with a new operation. Transport failures retain the same plan
and signed bytes.

Device enrollment does not buy annual service or publish request classes. Those
remain separate workflows. The portable client and Swift binding now expose
signed purchase and class-publication use cases. My approach page now exposes
class/profile editing and the configured simulated annual-service purchase;
general billing and financial management remain separate work. Each setup request is committed to the encrypted setup
journal before transmission, and a retry retains the exact signed bytes.
A purchase acknowledgement does not imply funded service: current coverage comes
from the authenticated account view. The app displays an empty inbox until a
sender has submitted a request to a properly configured recipient.

Request confirmations now retain the reviewed sender and request ID. Selection
changes or locking dismiss the confirmation. Swift checks that the same request
is displayed, and Rust refreshes canonical state and requires that exact request
to remain awaiting a decision before signing. A later request from the same
sender cannot silently replace the reviewed request.

Rejected-request financial outcomes include a quiet "How the pool works" link.
Its visibility comes from the Rust financial projection's recorded nonzero
collateral forfeiture, forwarded as a typed flag; Swift does not infer it from
permission or status labels. The sheet explains pending forfeiture and maturity,
the previous calendar year's shared pool and equal shares for qualifying members,
and CSQD's separate processing charge. It does not estimate a personal payout.
The shared native preview includes Jules Morgan's fictional rejected request.
The full app and preview built; native inspection verified link placement,
sheet opening/dismissal, and absence on a pending request. The full quality gate
passed, including PostgreSQL. No tests were added;
live settlement, iOS and a full accessibility audit were not repeated for this
explanatory presentation change.

## Existing identity model and browser configuration

The client calls `/v1/enrollments`, `/complete`, `/renew` and the reconciliation
endpoint. It uses the existing enrollment intent/device proof and never decides
subject matching, login ownership, bank assurance or review outcomes. The local
`browser-identity` host uses the real identity application, PostgreSQL adapter and
`OidcJwksSessionVerifier`; only its loopback HTTP fixture hosting differs from the
production HTTPS host. The fixture emits signed RS256 JWTs, single-use codes and
S256 challenges. It does **not** establish a real person's identity.

`cs-maild` publishes this optional public configuration at `/v1/deployment`:

```json
{
  "browser_login": {
    "issuer": "http://127.0.0.1:8482",
    "client_id": "cs-mail-apple",
    "redirect_uri": "com.csqd.member.development:/oauth/callback"
  }
}
```

Register a public native client with code flow, S256 PKCE and this exact callback.
Issuer/client ID must match the identity service's OIDC configuration. A production
issuer and endpoints use HTTPS. Only loopback HTTP is allowed for local work.
Browser state, code verifier, tokens and raw applicant claims are transient. The
encrypted reservation stores a details fingerprint and the existing enrollment
intent, so restart resumes the same ownership claim. It never persists the ID token.

“Open account” reopens a device already enrolled here. Existing-account enrollment
on a replacement device remains a separate task using the identity model's
`RecoverDevice` contract; there is no new-account fallback and no profile importer.

## Local data and interrupted work

Keychain service/account: `com.csqd.member.development` / `member-device-v1`, device-only,
unlocked access. SQLite: `Application Support/com.csqd.member.development/member.sqlite`.
It contains one authenticated encrypted snapshot, using a separate Keychain cache
key. Cached message records remain HPKE ciphertext even inside that snapshot.
SQLite uses full synchronous commits; a process lock prevents concurrent owners.
No secret/debug formatting or HTTP response bodies appear in client errors.

Recipient-key registration and decisions persist the exact signed operation before
sending it. A lost response retains uncertainty; retry replays those bytes. Only a
validated provider receipt resolves a decision journal, followed by a canonical
refresh. The UI clears plaintext when locked/backgrounded and discards late results
from an older presentation session. Browser sign-in can be cancelled explicitly.

## Validation and current limits

The agreed claims concern secret/plaintext leakage and incorrect outcomes after
interruptions. Rust experiments challenge expired/substituted browser responses,
restart of enrollment, lost decision responses, failed local commits, ciphertext
integrity and denied key access during content retrieval. Existing service and
PostgreSQL suites cover canonical decisions and idempotence.

Run the native checks against a fresh local deployment:

```sh
CS_MAIL_APPLE_SERVER=http://127.0.0.1:8480 \
  apps/cs-mail-apple/.build/debug/CsMailAppleCheck
```

The harness checks native SQLite durability/exclusivity, actual isolated Keychain
creation/reopen/overwrite refusal, and Swift-to-Rust browser enrollment against the
existing identity service. Its fictional browser approval is performed over HTTP;
it does not automate Safari. Temporary native test credentials/files are removed.
This executable is used because the installed Command Line Tools lack XCTest.

Validated locally on 8 October 2026: macOS build and launch, enrollment form and
system authentication session startup, the controlled login page in Chrome,
native persistence/Keychain/FFI enrollment
checks, and the repository quality gate including PostgreSQL suites. Visual
inspection of the Safari authentication window was unavailable because Computer
Use lacked permission. Full visual sign-in, an interactive request decision, real
provider/bank assurance, signing/notarization and App Store distribution remain
unverified. This is the first development slice, not the full Milestone 4 exit.

An iOS Xcode target shares the same Swift package and Rust facade:

```sh
# Requires full Xcode and the relevant installed rustup target:
# aarch64-apple-ios-sim or aarch64-apple-ios
scripts/apple-ios-build.sh simulator
scripts/apple-ios-build.sh device
```

Full Xcode and iOS Rust targets are absent on the current machine. The project
plist parses, but iOS compilation, packaging and suspension/device behavior have
not been validated. The local loopback provider works on the Mac or simulator;
a physical phone needs a reachable HTTPS development deployment.

## Address publications and directory — 11 October 2026

The sidebar navigates between **Communication space**, **My approach page** and **Directory** in the same window. The editor publishes the
current device address's member-authored name, description, optional image, visibility
and up to eight request classes as one signed, revision-checked operation. Initial
publication is listed; visibility can be changed independently of access through
an exact reference or shared link. Image selection uses Apple's decoder to prepare
a bounded PNG and strips the input file's metadata. The screen displays the current
operator collateral bounds and processing component. Saved quotes are unaffected.

Directory search shows public publications; direct lookup includes unlisted ones.
A recipient detail shows currently quotable classes and their conditional-charge
rules. Class selection is review only: it does not authorize payment or contact.
Suspended/closed accounts return unavailable, without a reason. Expired coverage
leaves information visible and disables selection. An empty class collection offers
no paid request route. The sidebar separates standing contacts, express lanes and
closed approaches, retaining the existing Rust permission projection.

Portable Rust owns validation, signing, the encrypted publication journal and exact
retry. Explicit publication refusals resolve that operation; transport failures do
not. Reload retrieves canonical state. A second address needs its own recipient-key
authority; this slice does not introduce alias creation or device-key management.
The local service exposes `POST /v1/directory/commands`, `/search` and `/lookup`.
The public query endpoints support the future website, but this pass adds no website
or hosting. Discovery uses existing numeric public persona references and `/r/` links;
exact lookup also recognizes configured generated member email addresses. Human-readable mailbox claims were added in the subsequent owner-account cutover
recorded below; richer search and additional-address lifecycle remain later work. Search pages are live views, so edits
between pages can move entries; no frozen cross-page snapshot is claimed.

Migration 0031 adds address publications and operation receipts, with no replaced
reader or duplicate class store. Use fresh isolated development databases for validation;
older evidence databases remain untouched.

Validation completed locally on 11 October 2026. The full gate passed all eight
checks, including 113 ordinary tests/doctests, 94 PostgreSQL cases and the pinned
identity source manifest. Four new PostgreSQL experiments challenge foreign-address
(including same-account alias) authority, receipt-failure rollback and concurrent
edits, unlisting/account status/coverage, and replay over later edits. One portable
client experiment challenges lost responses and failed local acknowledgement commits,
requiring identical signed bytes after encrypted-store reopening.

The final macOS build and native harness passed SQLite/Keychain checks, browser
enrollment against the existing identity application, PNG publication, directory
search, unlisting, shared-link lookup and reopening. Native visual inspection found
and repaired editor clipping. Actual SwiftUI controls published the fictional Alice
profile and existing class, removed it from browsing through unlisting, opened it
through exact lookup, and preserved name/description/classes/unlisted state after
quit/reopen. Selecting a class displayed its 1550-minor-unit conditional charge
without authorizing a charge. Profile images were exercised by the native FFI
harness; the graphical file chooser was not separately exercised.

The final isolated demo runs against `cs_mail_directory_visual_1011` and
`cs_mail_directory_visual_identity_1011` on ports 9493–9495, with its encrypted
store at `target/apple/directory-final-recipient-1011`. The normal member store
was untouched. An earlier demo credential refused access after an ad hoc rebuild;
it and its database were preserved, and no replacement credential was written
into that store. Same-binary quit/reopen passed in the final demo. Earlier harness
checks used separate `cs_mail_directory_native_1011` and identity databases.

Not covered: public-web behavior/indexing, iOS runtime, a full accessibility audit,
remote image-decoder abuse campaigns, additional
address/device management, cross-page snapshot consistency or sending a paid request
from the directory. This slice publishes and reviews approaches; it adds no new
payment authority, public hosting, bank connector or external mail sending.

## Historical visual validation — October 2026

The offline visual fixture mode used in the October 11 navigation experiment has
been removed. Those observations did not establish canonical service outcomes.
The current application has one account-backed execution path. The subsequent
presentation observations below retain their original validation scope.

The navigation correction was checked in the rebuilt offline macOS demo: search
for Daniel, open his profile, refresh it, return to the same filtered results,
and open Maya through a shared link before returning to those same results.
The native build and all eight quality-gate checks, including PostgreSQL, passed
(`/tmp/cs-mail-directory-back-build.log`, `/tmp/cs-mail-directory-back-gate.log`).
No automated tests were added under the agreed scope. Live-service navigation,
paginated results and iOS runtime behavior were not exercised in this UI check.

Readable member-screen text inherits native text selection from `MemberView`,
including directory profiles, explanations and status text. The standard macOS
Edit menu supplies Copy/Paste; no separate clipboard implementation is needed.
Contact and navigation rows remain buttons that open their readable detail views.
Native verification selected Daniel's profile description by dragging, copied it
with Command-C, and pasted it with Command-V into both Directory search and the
profile description field. The editor draft was reloaded without saving.
The build and full quality gate passed (`/tmp/cs-mail-text-selection-build.log`,
`/tmp/cs-mail-text-selection-gate.log`). No automated tests were added; iOS selection
and every individual text/control combination were not exercised.

Clicking the selected “Ways to approach me” class now clears the selection and
its conditional-charge explanation. Clicking another class switches the single
selection. Native macOS verification covered selecting Alice's introduction,
switching to research collaboration and clicking research again to clear it.
The native build and full quality gate passed (`/tmp/cs-mail-class-deselect-build.log`,
`/tmp/cs-mail-class-deselect-gate.log`). No new automated tests were added under
the agreed visual-review scope; live-service and iOS interaction were not exercised.

The Swift presentation consolidation replaces label-string navigation with
`MemberDestination`, introduces distinct presentation identifier structs, moves
Directory's search/profile/return/refresh/failure state into `DirectoryModel`,
and exposes model output read-only. Session and operation enums replace stored
flags. Scoped errors preserve unrelated content and ordinary refusals no longer
mark the connection offline. A Swift actor replaces `ClientWorker`'s manually
confined queue and unchecked Sendable declaration. See
`docs/apple-client-architecture.md` for the boundary and ownership details.

Validation retained the agreed scope, with no new automated tests. The native
build and all eight existing quality-gate checks, including PostgreSQL, passed
(`/tmp/cs-mail-swift-state-build.log`, `/tmp/cs-mail-swift-state-gate.log`). A complete
Swift concurrency-check build of CsMailApple passed without source diagnostics
(`/tmp/cs-mail-swift-concurrency.log`; SwiftPM's user-cache access warnings remain).
Native offline checks covered contact/request navigation and its decision
explanation, filtered Directory search, unavailable lookup preserving results,
screen switching retaining search, profile refresh and return, shared-link lookup,
class selection/deselection, editing/saving a profile, Copy/Paste into search,
and quit/reopen resetting demo edits. The actor's live-service call path, browser
cancellation, response-loss recovery, paginated results, image selection and iOS
runtime were not exercised in this visual pass. Existing Rust suites remain
separate evidence for canonical behavior.


## Historical durability groundwork — before owner enrollment

Durable-development validation on 11 October 2026 used the existing suites and
native harness, with optional coordination added to its existing enrollment/reopen
scenario for a supervised-service restart; no new test cases were added. The
first restart attempt exposed shutdown/port-reuse races; the launcher now waits
for children and permits ordinary TCP restart reuse while refusing live listeners.
The final observation used `cs_mail_durable_check_1011_2` and
`cs_mail_durable_identity_check_1011_2`, with state in
`target/apple-durable-validation-2`. Its 12 deployment/evidence/OIDC key digests
were identical after restart, the reopened native account ID was identical, and
the account count remained one. The native check also passed encrypted SQLite
replacement/exclusivity, isolated real Keychain creation/reopen/overwrite refusal,
directory editing/discovery/unlisting, and custody-denial checks
(`/tmp/cs-mail-durable-native-final.log`). The existing interrupted-enrollment,
response-loss decision/publication and local-commit recovery suites passed in the
full eight-check quality gate (`/tmp/cs-mail-durable-gate.log`). The stably signed
app built (`/tmp/cs-mail-durable-final-build.log`), preserved its designated
requirement across rebuilds, and native inspection confirmed development labeling
and the saved 9693 enrollment endpoint. At that groundwork checkpoint no personal enrollment had been submitted;
the prepared personal database contained zero accounts. The completed cutover
below supersedes that status. Existing account stores
and the first validation attempt's databases remain preserved.

Groundwork limits at that time: persistent **personal** account quit/reopen and
credential access across an app rebuild still required actual enrollment; the native harness
used an isolated fictional account. Replacement-device recovery, aliases, active
counterpart provisioning, live banking/payments, public email and iOS runtime are
not established by these observations. The subsequent cutover adds checked mailbox claims; broader naming and
address-lifecycle policy remain separate work.


## Persistent owner-account cutover — 11 October 2026

Migration 0032 adds a unique normalized public mailbox to the address directory.
Rust's checked `MemberAddress` distinguishes it from `ProtocolIdentity`; a typed
`DirectoryReference` selects exact protocol or mailbox lookup through one storage
projection. The signed publication atomically claims the mailbox, saves the
profile/classes, and records its receipt. The protected absence/ownership check
and PostgreSQL uniqueness constraint exclude competing claims. This development
slice limits claims to the configured namespace (`csqd-mail.com`), reserves the
old generated-address prefix, and supports simple lowercase names containing
letters, digits, hyphens and periods. Once claimed, a mailbox cannot be removed or
renamed by profile editing. Additional naming and reclamation policies remain
future work. This is local address assignment, with no DNS or external delivery.

Public profile names populate native correspondence projections; Swift does not
hardcode live participant names. The Rust client now journals standing, rejection
and scoped express-lane evidence as distinct command alternatives and checks the
same provider receipts before clearing uncertainty. Service activation displays
canonical current coverage separately from command acknowledgement. Successful
publication refreshes cached Directory results.

Validation used the agreed existing gate/recovery suites and native checks, with
no new automated test cases. The existing cross-address PostgreSQL experiment was
extended to challenge exclusive mailbox claims, refusal without class effects,
and normalized direct lookup. All eight gate checks passed, including PostgreSQL
(`/tmp/cs-mail-account-cutover-gate.log`). The native SQLite/Keychain, interrupted
browser enrollment, directory/image/unlisting/reopen and custody-denial checks
passed against fresh isolated databases on 9793–9795
(`/tmp/cs-mail-cutover-native-check.log`).

Native interaction enrolled the owner with fictional evidence in Chrome, claimed
the requested mailbox, published an introduction class, activated simulated
coverage and read encrypted participant messages. Standing acceptance and an
express lane each showed a confirmed simulated 550-unit refund; rejection showed
50 units retained and 500 units pending forfeiture. Directory Back navigation,
unlisted exact mailbox lookup, unavailable Noah and publication editing were
checked. The owner account, contacts and unlisted setting survived a signed rebuild
and quit/reopen without a credential prompt. A publication attempted with the
backend stopped remained pending after offline quit/reopen, then replayed after
restart; its receipt count increased from two to three exactly once, restoring
listed visibility. All six account IDs and all 12 deployment/evidence/OIDC key
digests were unchanged. No canonical database record was written directly.

Not covered: replacement-device recovery, iOS runtime, live identity/banking or
payments, public hosting/DNS/email, additional address management, a full
accessibility audit, or paid sending from the Directory UI. The owner's enrollment
is a persistent local development account and supplies no verified real-world
identity or mailbox-delivery claim.


The final signed build reopened against the restarted personal backend. Native
inspection confirmed the listed six-profile directory, exact lookup of
`marcus@csqd-mail.com`, its introduction class, and the populated communication
space. The app was left on that screen. An older running copy was closed after
bundle replacement prevented further UI control; saved state was preserved and
the final copy reopened successfully.

The browser is used for development enrollment. The app opens the local OIDC
fixture; selecting Marcus authorizes a fictional login assertion, not legal
identity or bank ownership. Rust binds the returned code to the attempt, and the
identity service validates signed evidence and device proof. See the
[production boundary](../../docs/apple-client-architecture.md#browser-evidence-and-production-boundary).
Production authentication/verification providers and real payment integration
are not established by this simulated journey.
