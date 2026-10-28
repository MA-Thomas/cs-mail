# Bounded cache and Rust worker experiment

Approved follow-up to the exact-store comparison. This is an isolated storage
worker experiment, not the member client's production worker or a store cutover.
No owner database or credential is opened. Source authority remains in the existing
Rust domain/client owners. The native C API is confined to this evaluation binary;
it is not an exception to the workspace's prohibition on unsafe Rust.

Freeze this protocol, scripts, Rust source, model-vector input, previous manifest
and native libraries before measurement. Preserve earlier failed latency results.
Use the pinned native builds and unchanged exact SQL plans from the preceding run.
Compare vec1 metadata and sqlite-vec eligible-rowid plans; the rejected streaming
join adds no new cache conjecture. No ANN or model-quality experiment is included.

Named conjectures and agreed validation scope:

1. Decrypted-page churn explains much of the 100k warm-query latency. Use cache
   suggestions of 2, 64, 256 and 512 MiB, 100,000 seeded Gaussian unit vectors,
   12 fixed queries, broad and 0.1% alias filters. Three timed passes follow one
   warmup per scope, giving 36 observations. Compare every first-pass top ten
   with the exhaustive eligible NumPy oracle (distance tolerance 1e-5). A changed
   neighbor falsifies preservation; p95 above 250 ms fails the unchanged target.
   Report first query separately: warm timings do not establish cold performance.
   Cache_size is advisory, not a hard process-memory limit. Record actual SQLite
   page-cache bytes, hits/misses, process RSS and the fixed ordering. Keep mmap off.
   A larger cache cannot be called a success without disclosing its memory cost.
2. A Rust-owned connection on a separate thread supports cancellation without the
   controller waiting for the query mutex. Use a one-slot bounded queue, explicit
   lifetime/request tickets, sqlite3_interrupt and a cooperative two-second
   progress deadline. Challenge active vector search, an expensive recursive SQL
   computation, a queued request, and a completed result awaiting consumption.
   Refuse stale work before SQL and again when consuming results. A deterministic
   progress-callback barrier holds a real SQLite step across invalidation. Any
   delivered stale row falsifies the claim; report raw interrupt status separately
   from suppression. Measure invalidation and drain time separately. A cancellation
   acknowledgment target of 50 ms concerns invalidation, not hard query termination.
3. The worker remains usable after query cancellation within a live lifetime;
   replacing a query must not revoke that lifetime. Lock is terminal for that
   worker: join it and close the connection, then reopen a new worker and reject
   the old ticket. A result from the old lifetime must never be accepted as new.

These are engineering counterexamples on known/synthetic data. They do not
establish app Keychain custody, simultaneous publication/deletion transactions,
foreground UI responsiveness, production worker integration, iOS, energy, realistic
semantic scale, encrypted synchronization, backup erasure or a general security
audit. Existing lifecycle/provenance checks remain earlier evidence and are not
relabelled as Rust production validation.

The runner builds one optimized Rust executable under the experiment's target
directory and checks that it links the pinned SQLCipher, never system SQLite.
Random keys and query blobs pass through stdin, never arguments or files. Each
worker owns one connection. The generated database is deleted after each arm;
only compact measurements, input hashes and build commands are retained. No
unrelated app, Rust workspace or PostgreSQL suite is required for this tool-only
slice.

References: [SQLite cache size](https://www.sqlite.org/pragma.html#pragma_cache_size),
[interrupt](https://www.sqlite.org/c3ref/interrupt.html),
[progress handler](https://www.sqlite.org/c3ref/progress_handler.html),
[database cache statistics](https://www.sqlite.org/c3ref/c_dbstatus_options.html).
