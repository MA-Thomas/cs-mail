# Gemma 2 → encrypted SQLite experiment

This is isolated development evidence, not a second member app or a production
storage adapter. It never opens the owner's database, Keychain or retained mail.
The approved scope is the next integration experiment described in the chat:
Gemma 2 vectors, encrypted SQLite, exact vec1/sqlite-vec comparison, structured
filters, deletion/lock, and keyword/passages. The selected production engine must
eventually sit behind a Rust adapter; these Python/C-API experiments do not provide
that integration or establish the real client's authority/custody boundary.

## Frozen claims before measurement

Use the existing frozen fictional corpus and measured Gemma 2 512d unit vectors.
Pin SQLCipher 4.19.0 (SQLite 3.53.4), SQLite's vec1 0.8 at source commit
`4012bdca5f1599d5dd3d6671112f170bfd48a26f`, and sqlite-vec 0.1.9. Compile both
extensions with clang -O3 for ARM64/NEON; do not mix system SQLite into the process.
Use SQLCipher's CommonCrypto provider, FTS5, WAL, secure deletion and
SQLITE_TEMP_STORE=3 (memory only). Keys are random experiment keys held only in
memory; no owner credentials are accessed. Record source/build hashes and versions.

Named conjectures and meaningful counterexamples:

1. Exact storage search preserves the numerical Gemma result. Falsification:
   a top-ten eligible neighbor differs from the NumPy exhaustive oracle outside
   float tolerance (1e-5), including a restrictive receiving/sender/time filter.
   Do not hide missed eligible neighbors by filtering a fixed global top K.
2. Source/profile identity remains attached to a result. Falsification: a different
   512d model profile is accepted, or a returned checksum/range does not verify
   against the original canonical fixture text. Source identity is not deduplicated
   by equal text. Metadata filters are fixture inputs, not product authorization.
3. Removal cannot be undone by stale publication or failed index cleanup.
   Falsification: after the durable removal/work intent, a warm or reopened query
   returns its text, or an old-generation job republishes it. A failed publication
   transaction must leave neither an orphan vector nor an orphan FTS row.
4. Lock stops disclosure. Falsification: a captured old lifetime can query after
   lock/reopen. Exercise the harness's closed connection and generation guard;
   do not claim this independently validates the app's existing lock implementation.
5. Content-bearing SQLite files are protected. Falsification: wrong/no-key schema
   access succeeds, or fixture text/vector canaries occur in the database or live
   WAL. Compile-time memory-only temporary storage must be verified, with a forced
   temporary sort. This is a bounded check, not a swap/crash-dump/security audit.

Compare exact vec1 flat/cosine with streaming joins and with metadata prefilters
(plus current-source joins), and sqlite-vec vec0 cosine plus eligible-rowid
filtering. Metadata copies change in the same vector-publication transaction;
the current-source join still excludes revoked generations. Preserve SQL plans. If an engine cannot
apply the same criteria safely, report that outcome rather than substituting an
approximate/overfetch query. Compare hybrid FTS5/vector passage results with fixed
RRF(60), retaining quoted-phrase constraints and exact-identifier precedence.
This is a new storage experiment; the earlier model experiment remains unchanged.

Performance workloads: the real 54 fictional occurrences and separately 10,000 /
100,000 deterministic seeded normalized Gaussian vectors (512d), broad and 0.1%
alias-filtered top ten. Synthetic vectors measure engine cost only, not embedding
quality or realistic passage distributions. Repeat fixed queries, record warm p50/
p95, reopen cost, insertion time, database size and process RSS. Targets: numerical
parity and all lifecycle checks must pass; exploratory warm 100k p95 <=250 ms broad
and filtered. No post-result tuning of these criteria. ANN/training/quantization,
battery, iOS and the running native app are outside this experiment.

Technical setup amendment before the completed comparison: an initial trial was
interrupted to correct sqlite-vec's missing SQLITE_VEC_ENABLE_NEON build flag.
Its output is not accepted as comparative evidence. Large queries now have a
two-second SQLite progress deadline; exceeding it fails the latency target and
stops that scope rather than running unbounded work. Normal scopes retain 36
timings and 12 oracle checks; cancelled scopes report their actual completed
counts and no successful parity claim for unexecuted queries. SQL extension work
must return to SQLite before its progress callback can cancel; this is cooperative,
not a hard real-time deadline. The original <=250 ms target is unchanged.

Query-plan amendment after the first completed measurements: retain the initial
results under `results/initial-plans/` (including their missed latency targets).
Vec1 now limits a materialized, current-eligible candidate CTE before the final
ORDER BY, avoiding consuming the entire stream for sorting. Both engines receive
the same covering source-filter index. Fixture, model vectors, query sets, cache
budget, deadlines and acceptance targets remain unchanged. Numerical parity must
be re-established; this is an engineering revision on known data, not fresh held-out
quality evidence. No losing result is erased or reclassified as a pass.

The experiment records a fresh Gemma query through the encrypted index as a live
end-to-end check, separately from cached-vector engine timing. FTS5/exact-match
precedence are not represented as improvements to the earlier frozen model scores.

Build/download inputs are under `target/vector-store-evaluation`; compact results
and source manifests are retained here as current review evidence. Remove temporary
databases and superseded builds once results have been captured; keep model weights
only until the model/runtime decision is resolved. No database key is written out.

Sources: [vec1](https://www.sqlite.org/vec1doc.html),
[sqlite-vec](https://alexgarcia.xyz/sqlite-vec/features/knn.html),
[SQLCipher](https://www.zetetic.net/sqlcipher/design/).
