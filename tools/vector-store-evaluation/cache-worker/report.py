"""Generate the review report from preserved cache/worker measurements."""
from pathlib import Path
import json
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0,str(ROOT/'tools/embedding-evaluation'))
from common import digest, write_json

result = json.loads((HERE/'results/results.json').read_text())
assert result['count']==100000 and len(result['arms'])==8
for path, expected in result['frozen']['inputs'].items():
    assert digest(ROOT/path)==expected, 'Measured source changed: '+path
assert result['generated_databases_removed']
checks = measurements = 0
rankings = {}
for arm in result['arms']:
    c = arm['cancellation']
    assert c['deadline_code']==c['vector_interrupt_code']==c['recursive_interrupt_code']==9
    assert c['completed_result_suppressed'] and c['replacement_query_succeeded'] and not c['queued_sql_started']
    assert c['old_ticket_refused_after_reopen'] and c['reopened_query_succeeded']
    assert c['invalidate_ms']<50
    for scope in arm['scopes']:
        assert scope['oracle_checks']==12 and len(scope['measurements'])==36
        assert scope['target_passed']==(scope['p95_ms']<=250)
        rows = [[row[0] for row in m['rows']] for m in scope['measurements']]
        key = scope['filtered']
        if key in rankings: assert rows == rankings[key]
        else: rankings[key] = rows
        ordered = sorted(m['roundtrip_ms'] for m in scope['measurements'])
        assert abs(scope['p95_ms']-(ordered[33]*.75+ordered[34]*.25))<1e-8
        checks += scope['oracle_checks']; measurements += len(scope['measurements'])
assert not list((ROOT/'target/vector-store-evaluation').glob('run-*'))
write_json(HERE/'results/validation.json',{'frozen_hashes_match':True,'arms':8,'oracle_checks':checks,'warm_measurements':measurements,'all_cancellation_cases_passed':True,'databases_removed':True,'member_app_integration':False,'all_cache_and_engine_rankings_identical':True,'p95_independently_recomputed':True,'no_generated_database_run_directories_remain':True})
names={'vec1':'vec1 metadata','vec0':'sqlite-vec'}
lines=['# Bounded cache and Rust vector worker — 25 October 2026','',
'**Completed isolated experiment. Production client integration and store selection remain open.**','',
'The follow-up uses a Rust thread that owns the pinned SQLCipher connection, a one-slot request queue, request/lifetime tickets, interruption and consumption-time stale-result checks. It runs outside the member app; no owner database, account or Keychain was accessed. It does not replace the production client’s existing search lifetime or establish its custody rules.','',
f"The protocol and source hashes were frozen at `{result['frozen']['frozen_at_utc']}`. See the [declared counterexamples](../tools/vector-store-evaluation/cache-worker/README.md), [raw measurements](../tools/vector-store-evaluation/cache-worker/results/results.json), and [build/link evidence](../tools/vector-store-evaluation/cache-worker/results/build.json). The previous experiment’s frozen inputs were verified unchanged. Its failed latency results remain [available](encrypted-vector-store-evaluation.md).",'',
'## Cache and exact-search results','',
'Reference: the owner’s M1 Max MacBook Pro, 32 GB. Native versions, model profile, 100,000 seeded Gaussian 512d unit vectors, exact SQL plans and eligibility filters match the preceding experiment. This is storage evidence, not retrieval-quality evidence at realistic mail scale. Each cache setting uses a fresh Rust child process and connection. Arms run vec1 then sqlite-vec, cache suggestions ascending, broad before filtered. Timing includes dispatch, Rust execution and response delivery; it excludes embedding inference.','',
'Each scope has one separately reported first query followed by 36 warm observations and 12 exhaustive eligible-oracle checks (1e-5 tolerance). The fixed warm p95 target remains 250 ms. The first query is after connection initialization on a newly generated, OS-cached file: it is not a cold-disk or model-start measurement. The filtered scope admits 0.1% of rows.','',
'| Engine | Cache suggestion MiB | Broad first / p95 ms | Filtered first / p95 ms | Largest observed SQLite cache MiB | Sampled process RSS MiB | Both p95 targets |',
'|---|---:|---:|---:|---:|---:|---|']
for a in result['arms']:
    b,f=a['scopes']
    lines.append(f"| {names[a['engine']]} | {a['cache_kib']/1024:.0f} | {b['first_query_ms']:.1f} / {b['p95_ms']:.1f} | {f['first_query_ms']:.1f} / {f['p95_ms']:.1f} | {max(b['max_cache_bytes'],f['max_cache_bytes'])/2**20:.1f} | {a['process_sampled_peak_rss_bytes']/2**20:.1f} | {'PASS' if a['both_scopes_passed'] else 'FAIL'} |")
lines += ['',
'Cache_size is a per-connection suggestion, not a hard application memory cap. The recorded page-cache bytes include allocator overhead, while sampled RSS includes extension/connection allocations and allocator retention across worker close/reopen. RSS was sampled every 5 ms and may miss peaks; this is not an engine-only allocation measurement. No embedding model is loaded in these child processes. Mmap is disabled. The 2 MiB control is 2,048 KiB, compared with the prior default 2,000 KiB.','',
f"All {checks} eligible-oracle comparisons passed across {measurements} warm measurements. Cache hits/misses and individual rows are retained, rather than inferring correctness or cache residency from latency. These are finite engineering checks on generated data; fixed arm order and the lack of randomized repeats limit causal/performance conclusions.",'',
'## Cancellation and worker boundary','',
'Each of the eight settings exercised a 10 ms cooperative deadline on expensive recursive SQL, a real vector step paused in a progress callback while its replacement was queued and the worker locked, unpaused recursive SQL interrupted from the controller, a completed response invalidated before consumption, query replacement without closing the lifetime, and close/join/reopen with rejection of an old ticket. Interrupted operations returned SQLITE_INTERRUPT (9); the stale queued request never started SQL. Successful queries followed deadline cancellation, replacement and reopening.','',
'The vector barrier establishes safe invalidation across an active SQLite step. It does not measure interruption in the middle of an extension’s uninterrupted native kernel. Progress checks and SQLite interruption are cooperative; the two-second query budget is not a hard real-time termination bound. Invalidation and subsequent drain are reported separately.','',
'| Engine | Cache MiB | Vector invalidation / drain ms | Recursive invalidation / drain ms | Deadline observed ms |',
'|---|---:|---:|---:|---:|']
for a in result['arms']:
    c=a['cancellation']
    lines.append(f"| {names[a['engine']]} | {a['cache_kib']/1024:.0f} | {c['invalidate_ms']:.3f} / {c['drain_ms']:.3f} | {c['recursive_invalidate_ms']:.3f} / {c['recursive_drain_ms']:.3f} | {c['deadline_ms']:.2f} |")
lines += ['',
'The initial small harness check exposed interruption during statement preparation; that path was corrected before the frozen full run. The successful 10k harness evidence and the earlier diagnostic remain recorded separately. No observed failing 100k setting was erased or tuned into a pass.','',
'## Implementation consequence','']
for engine in ['vec1','vec0']:
    passing=[a for a in result['arms'] if a['engine']==engine and a['both_scopes_passed']]
    if passing:
        first=min(passing,key=lambda a:a['cache_kib'])
        lines.append(f"{names[engine]} first meets both warm targets at the measured {first['cache_kib']/1024:.0f} MiB cache suggestion. This is a provisional configuration for further validation, not a minimum established for other datasets or a production memory commitment.")
    else: lines.append(f"{names[engine]} does not meet both warm targets at any declared cache suggestion.")
    lines.append('')
lines += [
'Exact search should not be rejected, or ANN introduced, solely because the earlier tiny-cache plans missed their target. Carry the measured latency/memory tradeoff forward. A production slice still needs a safe Rust SQLCipher adapter behind existing client authority, cancellation wired to the current client lifetime, bounded model/extraction work, atomic index publication/deletion, final disclosure checks, supported Apple packaging and continuity-preserving native validation. The evaluation binary’s explicit unsafe C binding is not production code and is not a workspace lint exception.','',
'No member-app executable or store was changed, so there is no new app installation to assess. No unrelated Rust/PostgreSQL suite was run. Existing fixture provenance, encryption and sequential lifecycle checks remain prior evidence; this run does not add production custody, concurrent source mutation, sync/backup, battery, iOS or UI-responsiveness validation. Before/after power/thermal snapshots are context only, not energy measurement. All generated databases were deleted; experiment keys were never written to files.','',
'Primary references: [SQLite cache semantics](https://www.sqlite.org/pragma.html#pragma_cache_size), [thread-safe interruption and close requirements](https://www.sqlite.org/c3ref/interrupt.html), [cooperative progress checks](https://www.sqlite.org/c3ref/progress_handler.html), [cache statistics](https://www.sqlite.org/c3ref/c_dbstatus_options.html).','']
(ROOT/'docs/vector-cache-worker-evaluation.md').write_text('\n'.join(lines))
print('Verified inputs and generated report')
