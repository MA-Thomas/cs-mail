# Bounded cache and Rust vector worker — 25 October 2026

**Completed isolated experiment. Production client integration and store selection remain open.**

The follow-up uses a Rust thread that owns the pinned SQLCipher connection, a one-slot request queue, request/lifetime tickets, interruption and consumption-time stale-result checks. It runs outside the member app; no owner database, account or Keychain was accessed. It does not replace the production client’s existing search lifetime or establish its custody rules.

The protocol and source hashes were frozen at `2026-10-25T20:15:31.715437+00:00`. See the [declared counterexamples](../tools/vector-store-evaluation/cache-worker/README.md), [raw measurements](../tools/vector-store-evaluation/cache-worker/results/results.json), and [build/link evidence](../tools/vector-store-evaluation/cache-worker/results/build.json). The previous experiment’s frozen inputs were verified unchanged. Its failed latency results remain [available](encrypted-vector-store-evaluation.md).

## Cache and exact-search results

Reference: the owner’s M1 Max MacBook Pro, 32 GB. Native versions, model profile, 100,000 seeded Gaussian 512d unit vectors, exact SQL plans and eligibility filters match the preceding experiment. This is storage evidence, not retrieval-quality evidence at realistic mail scale. Each cache setting uses a fresh Rust child process and connection. Arms run vec1 then sqlite-vec, cache suggestions ascending, broad before filtered. Timing includes dispatch, Rust execution and response delivery; it excludes embedding inference.

Each scope has one separately reported first query followed by 36 warm observations and 12 exhaustive eligible-oracle checks (1e-5 tolerance). The fixed warm p95 target remains 250 ms. The first query is after connection initialization on a newly generated, OS-cached file: it is not a cold-disk or model-start measurement. The filtered scope admits 0.1% of rows.

| Engine | Cache suggestion MiB | Broad first / p95 ms | Filtered first / p95 ms | Largest observed SQLite cache MiB | Sampled process RSS MiB | Both p95 targets |
|---|---:|---:|---:|---:|---:|---|
| vec1 metadata | 2 | 313.3 / 321.8 | 302.9 / 326.7 | 2.1 | 25.8 | FAIL |
| vec1 metadata | 64 | 342.0 / 350.4 | 320.8 / 369.2 | 65.8 | 101.6 | FAIL |
| vec1 metadata | 256 | 485.9 / 25.5 | 20.3 / 11.3 | 223.8 | 291.5 | PASS |
| vec1 metadata | 512 | 352.9 / 25.2 | 12.3 / 11.3 | 223.8 | 290.9 | PASS |
| sqlite-vec | 2 | 398.9 / 527.2 | 285.8 / 304.2 | 2.1 | 17.7 | FAIL |
| sqlite-vec | 64 | 427.5 / 454.9 | 301.8 / 303.5 | 65.8 | 97.0 | FAIL |
| sqlite-vec | 256 | 477.6 / 99.2 | 8.3 / 8.6 | 226.9 | 290.3 | PASS |
| sqlite-vec | 512 | 439.6 / 97.3 | 8.9 / 8.9 | 226.9 | 291.6 | PASS |

Cache_size is a per-connection suggestion, not a hard application memory cap. The recorded page-cache bytes include allocator overhead, while sampled RSS includes extension/connection allocations and allocator retention across worker close/reopen. RSS was sampled every 5 ms and may miss peaks; this is not an engine-only allocation measurement. No embedding model is loaded in these child processes. Mmap is disabled. The 2 MiB control is 2,048 KiB, compared with the prior default 2,000 KiB.

All 192 eligible-oracle comparisons passed across 576 warm measurements. Cache hits/misses and individual rows are retained, rather than inferring correctness or cache residency from latency. These are finite engineering checks on generated data; fixed arm order and the lack of randomized repeats limit causal/performance conclusions.

## Cancellation and worker boundary

Each of the eight settings exercised a 10 ms cooperative deadline on expensive recursive SQL, a real vector step paused in a progress callback while its replacement was queued and the worker locked, unpaused recursive SQL interrupted from the controller, a completed response invalidated before consumption, query replacement without closing the lifetime, and close/join/reopen with rejection of an old ticket. Interrupted operations returned SQLITE_INTERRUPT (9); the stale queued request never started SQL. Successful queries followed deadline cancellation, replacement and reopening.

The vector barrier establishes safe invalidation across an active SQLite step. It does not measure interruption in the middle of an extension’s uninterrupted native kernel. Progress checks and SQLite interruption are cooperative; the two-second query budget is not a hard real-time termination bound. Invalidation and subsequent drain are reported separately.

| Engine | Cache MiB | Vector invalidation / drain ms | Recursive invalidation / drain ms | Deadline observed ms |
|---|---:|---:|---:|---:|
| vec1 metadata | 2 | 0.000 / 0.025 | 0.000 / 0.009 | 10.01 |
| vec1 metadata | 64 | 0.000 / 0.025 | 0.000 / 0.007 | 10.01 |
| vec1 metadata | 256 | 0.000 / 0.014 | 0.001 / 0.010 | 10.01 |
| vec1 metadata | 512 | 0.000 / 0.023 | 0.001 / 0.010 | 10.01 |
| sqlite-vec | 2 | 0.000 / 0.011 | 0.000 / 0.019 | 10.01 |
| sqlite-vec | 64 | 0.000 / 0.017 | 0.001 / 0.008 | 10.05 |
| sqlite-vec | 256 | 0.000 / 0.011 | 0.000 / 0.014 | 10.05 |
| sqlite-vec | 512 | 0.000 / 0.012 | 0.000 / 0.005 | 10.02 |

The initial small harness check exposed interruption during statement preparation; that path was corrected before the frozen full run. The successful 10k harness evidence and the earlier diagnostic remain recorded separately. No observed failing 100k setting was erased or tuned into a pass.

## Implementation consequence

vec1 metadata first meets both warm targets at the measured 256 MiB cache suggestion. This is a provisional configuration for further validation, not a minimum established for other datasets or a production memory commitment.

sqlite-vec first meets both warm targets at the measured 256 MiB cache suggestion. This is a provisional configuration for further validation, not a minimum established for other datasets or a production memory commitment.

Exact search should not be rejected, or ANN introduced, solely because the earlier tiny-cache plans missed their target. Carry the measured latency/memory tradeoff forward. A production slice still needs a safe Rust SQLCipher adapter behind existing client authority, cancellation wired to the current client lifetime, bounded model/extraction work, atomic index publication/deletion, final disclosure checks, supported Apple packaging and continuity-preserving native validation. The evaluation binary’s explicit unsafe C binding is not production code and is not a workspace lint exception.

No member-app executable or store was changed, so there is no new app installation to assess. No unrelated Rust/PostgreSQL suite was run. Existing fixture provenance, encryption and sequential lifecycle checks remain prior evidence; this run does not add production custody, concurrent source mutation, sync/backup, battery, iOS or UI-responsiveness validation. Before/after power/thermal snapshots are context only, not energy measurement. All generated databases were deleted; experiment keys were never written to files.

Primary references: [SQLite cache semantics](https://www.sqlite.org/pragma.html#pragma_cache_size), [thread-safe interruption and close requirements](https://www.sqlite.org/c3ref/interrupt.html), [cooperative progress checks](https://www.sqlite.org/c3ref/progress_handler.html), [cache statistics](https://www.sqlite.org/c3ref/c_dbstatus_options.html).
