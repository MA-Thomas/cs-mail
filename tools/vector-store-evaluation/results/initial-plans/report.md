# Encrypted vector-store experiment — 25 October 2026

**Status: isolated integration experiment completed; no production store has been selected or installed.**

The experiment connects Gemma 2 text vectors to SQLCipher, compares exact vector searches with eligibility/structured filters, and resolves results to checksum-verified fictional passages. It does not open the owner’s store or Keychain. The member app remains unchanged by this experiment.

## Frozen scope and reference

Reference: the owner’s MacBookPro18,2, M1 Max, 32 GB; macOS 26.6.2. The same 54 fictional occurrences, 48 queries and 512-dimensional normalized Gemma 2 vectors from the earlier model screen are used. Four occurrences deliberately fail account/retention/removal/expiry eligibility. These are known regression/engineering cases, not new unseen quality evidence.

Frozen at `2026-10-25T19:56:23.872564+00:00`. Compatibility profile fingerprint: `57d70c2fb8bae89b32a1759aaf6ebc4d2176dc20a323232260d93ae254ad3319`. Raw inputs, library/source hashes, versions and query plans are in [the results](../tools/vector-store-evaluation/results/results.json); the [protocol](../tools/vector-store-evaluation/README.md) names the counterexamples and acceptance targets before measurement.

Sources: SQLCipher 4.19.0 / SQLite 3.53.4, CommonCrypto; vec1 0.8 at pinned SQLite commit `4012bdca5f1599d5dd3d6671112f170bfd48a26f`; sqlite-vec 0.1.9. Both extensions are native ARM64 builds with -O3. The runtime reports NEON for vec1. SQLCipher is compiled with FTS5 and SQLITE_TEMP_STORE=3; WAL, FULL synchronous commits and secure deletion are enabled. Encryption keys are random per test database and held only in process memory.

## Correctness and lifecycle evidence

| Arm | Exact filtered query cases | Verified returned passages | Lifecycle/failure checks | Database/WAL checks |
|---|---:|---:|---|---|
| vec1 flat + streaming/source join | 48 | 434 | PASS | PASS |
| vec1 flat + metadata/source join | 48 | 434 | PASS | PASS |
| sqlite-vec vec0 + eligible rowids | 48 | 434 | PASS | PASS |

Exact-query results are checked against exhaustive NumPy cosine search over the same eligible set, with 1e-5 distance tolerance. Restrictive filters and an empty receiving scope are included. Every result resolves to the original fixture occurrence, canonical UTF-8 body range and checksum. A different model profile is refused even if it also has 512 dimensions.

The failure experiments challenge incomplete publication, removal followed by a failed cleanup transaction, reopen while cleanup is pending, attempted publication from both old/current generations after removal, expiry at query time, and requests captured before lock/reopen. Durable source removal and its work intent commit together; current-source joins suppress results while derived cleanup is pending. The final cleanup removes text, FTS and vector entries while preserving the terminal source marker.

The harness’s connection/generation checks demonstrate this experimental adapter only. They do not replace the Rust domain owners or independently validate production Keychain custody, concurrent app workers, encrypted synchronization, server backups, or all client lock races.

The encryption experiment checks fixture text/vector canaries in the database and live WAL, wrong-key refusal, refusal by the system SQLite CLI without a key, compile-time memory-only temporary storage, and a forced 10,000-row temporary sort. This is bounded evidence, not a proof about swap, crash dumps, forensic erasure, arbitrary extensions or a completed security audit.

## Engine cost

These workloads use separately generated seeded Gaussian unit vectors, not Gemma-generated passages. They isolate storage/query cost and do not measure model quality, realistic distribution, ANN recall or production mail scale. Completed first-pass queries are checked against the same exact eligible oracle. Normal scopes comprise 36 warm measurements and 12 oracle checks; a cooperative two-second SQLite deadline cancels over-budget scopes, whose actual completed counts are retained in the raw evidence. A cancelled scope fails the unchanged 250 ms target and establishes no parity for unfinished queries. Extension work must return control to SQLite for cancellation. Timings include the SQL/C API and filtering and exclude model inference. The filtered scope admits 0.1% of rows.

| Arm | Vectors | Broad p50 / p95 ms | Filtered p50 / p95 ms | Insert + index s | DB MiB | Reopen s | 100k target |
|---|---:|---:|---:|---:|---:|---:|---|
| vec1 flat + streaming/source join | 10,000 | 120.76 / 122.66 | 120.23 / 121.86 | 0.84 | 40.6 | 0.100 | — |
| vec1 flat + streaming/source join | 100,000 | Cancelled: >2 s cooperative budget | Cancelled: >2 s cooperative budget | 7.92 | 406.0 | 0.101 | FAIL |
| vec1 flat + metadata/source join | 10,000 | 122.67 / 125.47 | 33.17 / 36.08 | 0.88 | 40.6 | 0.100 | — |
| vec1 flat + metadata/source join | 100,000 | Cancelled: >2 s cooperative budget | 312.38 / 322.11 | 8.38 | 406.0 | 0.100 | FAIL |
| sqlite-vec vec0 + eligible rowids | 10,000 | 38.64 / 39.78 | 33.34 / 33.89 | 0.83 | 22.1 | 0.099 | — |
| sqlite-vec vec0 + eligible rowids | 100,000 | 385.66 / 412.08 | 324.12 / 337.70 | 8.74 | 216.8 | 0.102 | FAIL |

Reopen includes SQLCipher connection setup/key verification and extension initialization, not a cold filesystem cache flush. The scale database contains vector and eligibility metadata; it does not include a populated FTS corpus or the owner’s encrypted workspace. Insertion uses one bulk transaction. The separate lifecycle cases use bounded source transactions and injected failures.

Whole experiment maximum process RSS: 2.35 GiB. This includes NumPy oracles, generated vectors, SQLite caches and the later loaded model; it is not an isolated engine memory footprint. Arms run in a fixed order in one process. Energy/battery, native foreground responsiveness and iOS were not measured.

## Keyword and passage integration

FTS5 and vector retrieval share the encrypted database and explicit source/profile identity. The experiment uses fixed reciprocal-rank fusion (60), quoted-phrase constraints and exact-identifier precedence. This is an explicit retrieval rule, not model tuning or a revision of the earlier frozen model scores.

| Arm | Positive test cases | Hybrid relevant first | Hybrid Recall@5 |
|---|---:|---:|---:|
| vec1 flat + streaming/source join | 35 | 88.6% | 94.3% |
| vec1 flat + metadata/source join | 35 | 88.6% | 94.3% |
| sqlite-vec vec0 + eligible rowids | 35 | 88.6% | 94.3% |

A fresh query embedding is also computed offline using the pinned Gemma 2 text-only model, checked against the earlier vector profile, then searched and resolved to passages. Model kernels are warmed first. The following timings include one query encoding, FTS/vector search and passage resolution against the small fixture, excluding model load. Single observations are path checks, not latency percentiles.

| Arm | Fresh-query path ms | Returned fixture passages |
|---|---:|---|
| vec1 flat + streaming/source join | 49.9 | d04, d03, d36, d42, d41 |
| vec1 flat + metadata/source join | 42.0 | d04, d03, d36, d42, d41 |
| sqlite-vec vec0 + eligible rowids | 40.2 | d04, d03, d36, d42, d41 |

## Decision boundary

Exact-search engine selection should follow this numerical/filter/lifecycle evidence and the measured plans, not the model leaderboard. Metadata copies are optimization evidence; current source eligibility remains authoritative. The next production slice must move the chosen contract behind the Rust application/storage boundary, derive the index key through existing custody, preserve atomic source/vector/FTS consequences, and package the model runtime for the supported Apple app. No existing store has been migrated and no compatibility/fallback reader has been introduced.

ANN/training/compression should be a separately justified experiment against these exact results if supported corpus sizes require it. Source expiry, removal and lock remain mandatory regardless of the chosen engine. Raw scores still do not establish an answer exists.

Temporary test databases and their keys were removed after capture. Official-source builds remain reproducible via `tools/vector-store-evaluation/build.py`; disposable build inputs should be removed when the storage/runtime choice is resolved. The compact result manifest remains current review evidence.

Primary references: [vec1 manual](https://www.sqlite.org/vec1doc.html), [sqlite-vec KNN/filter behavior](https://alexgarcia.xyz/sqlite-vec/features/knn.html), [SQLCipher file/temp protection](https://www.zetetic.net/sqlcipher/design/).
