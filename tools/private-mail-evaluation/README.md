# Private correspondence evaluation

Owner-authorized, local-only sample experiment. This is development support, not
another member app, a production import or an authority/retention decision.
The owner's original ZIP stays in place. Corpus text, headers, vectors, query
text and rankings never enter repository files or tool stdout. Only generic code
and this protocol belong here. Run outputs belong outside the repository.

Freeze the script, pinned model/native artifacts, source ZIP checksum and protocol
before processing content. Use the previously measured offline EmbeddingGemma 2
text encoder, MPS float32, 512d truncate-then-L2, batch four. Documents use
`title: {subject} | text: {passage}`; queries use the existing search-result prompt.
Preprocessing is a new, explicitly fingerprinted passage profile. No old vectors
are reused under a changed profile.

Validation scope and consequential counterexamples:

1. Date/header selection preserves occurrences: import messages dated in 2024 in
   America/New_York, quarantine undated/ambiguous dates, exclude later messages.
   Do not merge equal bodies or threads. Choose one MIME body (plain preferred,
   HTML fallback, including when the preferred plain body is empty), decode locally and never retrieve remote resources. Record
   skipped/empty/decoding cases and attachment counts. HTML becomes inert canonical
   text with script/style/head/template content suppressed. This is not a DOM map,
   attachment/OCR extraction or complete arbitrary-MIME/visibility support.
2. Passage evidence is exact within that canonical text: token-offset windows of
   384 body tokens with 64-token overlap, retaining original message digest, MIME
   part ordinal, byte coordinates and checksum. Never silently truncate the model
   input. Verify coverage of non-whitespace text, every stored passage, every
   returned passage, and encrypted-store profile matching. A wrong source, invalid
   range or silently lost model input falsifies this claim.
3. Real model vectors survive encrypted storage: compare vec1 exact search and
   sqlite-vec exact search with exhaustive eligible NumPy cosine search (1e-5).
   Use the declared 256 MiB cache suggestion, 12 fixed generic latency probes and
   three warm passes per engine. Record model load, tokenization/chunking, inference,
   indexing, full query encoding+search+resolution, storage and process memory.
   These generic probes measure cost and parity, not relevance or no-answer ability.
4. Private derivatives stay off plaintext working files: stream the ZIP member
   into memory; create random-key SQLCipher databases only in a mode-0700 temporary
   directory. Use memory-only temporary storage, WAL/FULL and secure deletion.
   Keep keys in memory only, test wrong-key refusal and plaintext canary absence
   in live database/WAL, then close and remove the generated databases on success
   or failure. Disable model networking and Python socket connections. This does
   not establish swap, crash-dump or forensic-erasure protection. Aggregate reports
   and empty query templates may persist outside the repository.
5. Usefulness requires independently supplied searches and owner judgments. Do
   not invent expected answers from imported content, tune prompts on these
   results, or claim recall/accuracy without judgments. For owner queries, compare
   semantic, FTS5 and fixed RRF-60 results, collapsing passages to distinct messages.
   A private Terminal review can show pooled top-five messages with method names
   withheld until grading; grades are irrelevant/partial/useful. Report only judged
   precision/usefulness and explicitly retain unknown corpus-wide recall.

`evaluate.py --archive PATH --output-directory PATH` runs cost/parity checks and
writes aggregate JSON only. `--query-file PATH --review` is for the owner to run
directly in a private Terminal: it displays original subjects/passages there, asks
for grades and writes aggregate judgments, never correspondence text, into the
output directory. It rebuilds ephemeral indices rather than retaining a key or
creating/replacing any Keychain credential. Terminal scrollback is presentation,
not encrypted durable storage; close it after review. The script must not be run
with `--review` through an agent-captured terminal.

No app build, unrelated Rust/PostgreSQL suites, model downloads, server calls,
mailbox modification, production store migration or account/Keychain operation is
part of this scope. Temporary indices are discarded; the original source archive
and owner-authored query file remain under the owner's control.

## Explicit preprocessing amendment after first sample pass

The first pass selected an empty plain alternative in four messages, three of
which also had substantial HTML alternatives. Its aggregate result and original
source/protocol snapshots are retained privately outside the repository. Version 2
prefers a nonempty inert HTML body when the plain alternative is empty. This is
an observed extraction repair, not a relevance-driven model/prompt change. The
archive, calendar scope, owner searches, token windows and metrics are unchanged;
the profile fingerprint changes and all vectors are regenerated. Challenge the
repair with a synthetic empty-plain/nonempty-HTML message before rerunning.

Before owner grading, challenge that two matching passages from one message cannot
inflate a ranked list or relevance score. RRF combines ranks by message occurrence,
then selects a representative passage; the pool also shows distinct passages
selected by the other methods and permits full-message reading. This review-only
refinement does not alter the completed body-extraction/model/cost measurements.

## Terminal I/O repair

The first owner-run review failed before showing candidates: Python's `r+` text
stream requires seeking, which `/dev/tty` does not support. Separate read-only and
write-only terminal streams replace that mode, with explicit prompt flushing.
Challenge this repair in a real controlling pseudo-terminal using fictional
messages: the prompt must appear, full-message expansion and grade entry must work,
and the review must complete with distinct-message aggregate judgments. A mocked
file handle does not establish terminal compatibility. Do not run private message
review through the agent-captured terminal as a regression check.

## Durable ratings and readable review

A blank input line must repeat the rating prompt, not end the review. Distinguish
that line from true EOF. Save every owner rating atomically before advancing;
resuming must skip saved ratings only when archive, preprocessing/model profile
and query-file hashes match. Ratings may persist outside the repository under
opaque query/occurrence numbers: no source text, addresses, query text or ranked
lists are saved in that checkpoint. This owner-annotation checkpoint is separate
from ephemeral content/vector indices. Reject changed-input resumes rather than
silently carrying grades to a different corpus.

Use numbered search banners, dividers, whitespace and wrapped paragraphs; repeat
the rating legend for each search. In the real-PTY fictional regression, challenge
blank input, full-message expansion, grade persistence across an interrupted review,
resuming without repeated candidates, and refusal of mismatched input bindings.
These repair checks do not relabel the interrupted private review as completed.
