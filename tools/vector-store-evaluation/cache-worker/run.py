"""Frozen cache/worker follow-up. Synthetic databases only; keys stay in pipes/memory."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import struct
import subprocess
import sys
import time
import threading
import uuid
from datetime import datetime, timezone

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path[:0] = [str(HERE.parent), str(ROOT/'tools/embedding-evaluation')]
from common import digest, write_json
from store import Store
from sqlcipher_api import WORK, LIB
import numpy as np
import psutil

args = argparse.ArgumentParser()
args.add_argument('--smoke', action='store_true')
options = args.parse_args()
OUT = HERE/'results'
OUT.mkdir(exist_ok=True)
BASELINE = HERE.parent/'results/frozen.json'
baseline = json.loads(BASELINE.read_text())
for relative, expected in baseline['inputs'].items():
    assert digest(ROOT/relative) == expected, 'Baseline input changed: '+relative
PROFILE = baseline['profile']
binary = WORK/'cache-worker'
rust = HERE/'src/main.rs'
commands = [
    ['rustc', '--edition=2024', '-O', str(rust), '-L', 'native='+str(LIB.parent), '-l', 'dylib=sqlite3', '-C', 'link-arg=-Wl,-headerpad_max_install_names', '-o', str(binary)],
    ['install_name_tool', '-change', '/usr/local/lib/libsqlite3.dylib', str(LIB), str(binary)],
    ['codesign', '--force', '--sign', '-', str(binary)],
]
for command in commands:
    subprocess.run(command, check=True, capture_output=True)
links = subprocess.check_output(['otool', '-L', str(binary)], text=True)
assert str(LIB) in links and '/usr/lib/libsqlite3' not in links and '/usr/local/lib/libsqlite3' not in links
write_json(OUT/'build.json', {'commands':commands, 'links':links, 'rustc':subprocess.check_output(['rustc','--version'],text=True).strip()})
paths = [HERE/'README.md', HERE/'run.py', rust, BASELINE, HERE.parent/'store.py', HERE.parent/'sqlcipher_api.py', LIB, WORK/'vec1.dylib', WORK/'vec0.dylib', ROOT/'target/embedding-evaluation/gemma2-fictional-vectors.npz', binary]
frozen = {'frozen_at_utc':datetime.now(timezone.utc).isoformat(), 'inputs':{str(p.relative_to(ROOT)):digest(p) for p in paths}, 'profile':PROFILE, 'baseline_inputs_verified':True, 'scope':'smoke' if options.smoke else 'declared-100k'}
if not options.smoke:
    assert not (OUT/'frozen.json').exists(), 'Do not overwrite measured evidence; declare an amendment'
    write_json(OUT/'frozen.json', frozen)
count = 10000 if options.smoke else 100000
caches = [65536] if options.smoke else [2048,65536,262144,524288]
run = WORK/('run-cache-'+uuid.uuid4().hex)
run.mkdir()
result = {'frozen':frozen,'count':count,'ordering':'vec1 metadata then sqlite-vec; cache suggestions ascending; broad then filtered; fresh child process per cache', 'arms':[]}
def host():
    return {'utc':datetime.now(timezone.utc).isoformat(), 'power':re.sub(r'\(id=\d+\)','(identifier omitted)', subprocess.check_output(['pmset','-g','batt'],text=True)), 'thermal':subprocess.run(['pmset','-g','therm'],capture_output=True,text=True).stdout}
result['host_before'] = host()

def sql(store, filtered, vector):
    class Capture:
        handle = True
        def execute(self, statement, values):
            self.statement = statement
            return []
    capture = Capture(); original = store.db; store.db = capture
    try: store.query(vector, {'filters':{'alias':'7'} if filtered else {}}, 1000, store.epoch, PROFILE)
    finally: store.db = original
    return capture.statement

def verify(rows, matrix, vector, ids):
    scores = matrix[ids-1]@vector
    threshold = np.partition(1-scores, 9)[9]
    assert len(rows) == 10 and len({r[0] for r in rows}) == 10
    allowed = set(map(int, ids))
    for identifier, distance in rows:
        assert identifier in allowed
        actual = 1-float(matrix[identifier-1]@vector)
        assert abs(actual-distance)<1e-5 and actual<=threshold+1e-5

try:
    for engine, metadata in [('vec1',True),('vec0',False)]:
        rng = np.random.default_rng(20261025)
        matrix = rng.standard_normal((count,512),dtype=np.float32)
        matrix /= np.linalg.norm(matrix,axis=1,keepdims=True)
        queries = rng.standard_normal((12,512),dtype=np.float32)
        queries /= np.linalg.norm(queries,axis=1,keepdims=True)
        path = run/(engine+'.db'); key = secrets.token_bytes(32)
        store = Store(path,key,engine,PROFILE,True,metadata)
        print('Build synthetic database',engine,count,flush=True)
        store.db.execute('BEGIN IMMEDIATE')
        sources = store.db.statement('INSERT INTO sources VALUES(?,?,?,?,?,?,?,?,?,?,?,?)')
        extra = ',profile,account,retained,removed,expires,alias,sender,kind,received' if metadata else ''
        vectors = store.db.statement('INSERT INTO vectors(rowid,vector'+extra+') VALUES('+','.join('?' for _ in range(11 if metadata else 2))+')')
        try:
            for i, vector in enumerate(matrix):
                alias = str(i%1000)
                sources.execute((i+1,'scale-'+str(i),PROFILE,'reference',1,0,2000,alias,'fixture','message',900,1))
                values = (i+1,vector.tobytes())
                vectors.execute(values+(PROFILE,'reference',1,0,2000,alias,'fixture','message',900) if metadata else values)
        finally: sources.close(); vectors.close()
        store.db.execute('COMMIT'); store.flat(); store.db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
        assert ('THREADSAFE=1',) in store.db.execute('PRAGMA compile_options')
        sqls = [sql(store,filtered,queries[0].tobytes()) for filtered in [False,True]]
        plans = [store.query(queries[0].tobytes(),{'filters':{'alias':'7'} if filtered else {}},1000,store.epoch,PROFILE,True) for filtered in [False,True]]
        store.lock()
        payload = key+b''.join(struct.pack('<I',len(s.encode()))+s.encode() for s in sqls)+queries.tobytes()
        for cache in caches:
            print('Measure Rust worker',engine,'cache MiB',cache/1024,flush=True)
            start = time.perf_counter()
            child = subprocess.Popen([str(binary),str(path),str(WORK/(engine+'.dylib')),str(cache),PROFILE,engine],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            rss = [0]
            def sample_rss():
                process = psutil.Process(child.pid)
                while child.poll() is None:
                    try: rss[0] = max(rss[0],process.memory_info().rss)
                    except psutil.NoSuchProcess: break
                    time.sleep(0.005)
            monitor = threading.Thread(target=sample_rss); monitor.start()
            stdout,stderr = child.communicate(payload,timeout=180)
            monitor.join()
            process = subprocess.CompletedProcess(child.args,child.returncode,stdout,stderr)
            if process.returncode:
                (OUT/('failed-'+engine+'-'+str(cache)+'.stdout')).write_bytes(process.stdout)
                (OUT/('failed-'+engine+'-'+str(cache)+'.stderr')).write_bytes(process.stderr)
                raise RuntimeError('Rust worker failed; retained diagnostic output')
            observed = json.loads(process.stdout)
            assert observed['sql'] == sqls
            assert rss[0]>0
            observed.update({'engine':engine,'metadata':metadata,'process_seconds':time.perf_counter()-start,'process_sampled_peak_rss_bytes':rss[0],'rss_sample_interval_ms':5,'database_bytes':path.stat().st_size,'query_plans':plans})
            for scope in observed['scopes']:
                ids = np.arange(8,count+1,1000) if scope['filtered'] else np.arange(1,count+1)
                measurements = scope['measurements']; assert len(measurements)==36
                for i, measurement in enumerate(measurements[:12]): verify(measurement['rows'],matrix,queries[i],ids)
                scope['oracle_checks'] = 12
                scope['p50_ms'] = float(np.percentile([m['roundtrip_ms'] for m in measurements],50))
                scope['p95_ms'] = float(np.percentile([m['roundtrip_ms'] for m in measurements],95))
                scope['target_passed'] = scope['p95_ms']<=250
                scope['max_cache_bytes'] = max(m['cache_bytes'] for m in measurements)
            observed['both_scopes_passed'] = all(s['target_passed'] for s in observed['scopes'])
            result['arms'].append(observed)
            write_json(OUT/('smoke.json' if options.smoke else 'results.json'),result)
        for suffix in ['', '-wal', '-shm']: Path(str(path)+suffix).unlink(missing_ok=True)
        del key,payload
    result['host_after'] = host()
    result['generated_databases_removed'] = True
    write_json(OUT/('smoke.json' if options.smoke else 'results.json'),result)
finally:
    shutil.rmtree(run)
print('Completed; temporary databases removed',flush=True)
