"""Approved storage experiment; synthetic sources only, no member client adapter."""
from __future__ import annotations

import ast
import hashlib
import importlib.metadata
import json
import os
import re
import secrets
import shutil
import statistics
import subprocess
import sys
import time
import uuid
from datetime import datetime,timezone
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
sys.path.insert(0,str(ROOT/'tools/embedding-evaluation'))
from common import fixture,eligible,metrics,words,phrase,fused_rank,digest,write_json,check_frozen
from sqlcipher_api import WORK,LIB,Database
from store import Store
import numpy as np
import psutil

MODELPATH=ROOT/'target/embedding-evaluation'
raw,docs=fixture();model_result=json.loads((MODELPATH/'gemma2.json').read_text())
assert model_result['frozen']==check_frozen()
vectors=np.load(MODELPATH/'gemma2-fictional-vectors.npz')
document_vectors=vectors['documents'];query_vectors=vectors['queries']
PROFILE=hashlib.sha256(json.dumps({'model':model_result['model'],'revision':model_result['revision'],'profile':model_result['profile'],'artifacts':model_result['artifacts_sha256']},sort_keys=True).encode()).hexdigest()
input_paths=[HERE/name for name in ('README.md','store.py','sqlcipher_api.py','experiment.py','build.py')]+[ROOT/'tools/embedding-evaluation/fixture.json',MODELPATH/'gemma2.json',MODELPATH/'gemma2-fictional-vectors.npz',WORK/'vec1.c',WORK/'sqlite-vec/sqlite-vec.c',WORK/'sqlcipher/sqlite3.c',LIB,WORK/'vec1.dylib',WORK/'vec0.dylib']
inputs={str(p.relative_to(ROOT)):digest(p) for p in input_paths}
freeze=WORK/'frozen.json'
assert not freeze.exists(),'Do not overwrite an observed experiment; declare an amendment first'
write_json(freeze,{'frozen_at_utc':datetime.now(timezone.utc).isoformat(),'inputs':inputs,'profile':PROFILE})
run=WORK/('run-'+uuid.uuid4().hex);run.mkdir()
result={'frozen':json.loads(freeze.read_text()),'source_downloads':json.loads((WORK/'downloads.json').read_text()),'model':model_result['model'],'reference_device':model_result['system_before'],'arms':{},'limits':['fictional corpus; no owner credential','synthetic Gaussian scale vectors; no semantic scale claim','not production Rust or native app integration','no ANN, battery, iOS or swap/crash-dump audit']}


def blocked(call):
    try:call()
    except (ValueError,RuntimeError):return True
    raise AssertionError('An operation expected to be refused succeeded')


def parity(rows, matrix, query, ids):
    expected=sorted(ids,key=lambda i:(-float(matrix[i-1]@query),i))[:10]
    assert len(rows)==len(expected)
    threshold=1-float(matrix[expected[-1]-1]@query) if expected else 0
    for identifier,distance in rows:
        assert identifier in ids
        correct=1-float(matrix[identifier-1]@query)
        assert abs(distance-correct)<1e-5,(identifier,distance,correct)
        assert correct<=threshold+1e-5,'Missed nearer eligible neighbor'
    assert len({row[0] for row in rows})==len(rows)


def file_checks(store,matrix):
    marker='SQLCIPHER-VECTOR-CANARY-'+secrets.token_hex(40)
    store.db.execute('CREATE TABLE protection_check(value TEXT)')
    store.db.execute('INSERT INTO protection_check VALUES(?)',(marker,))
    # Force memory temporary storage with a low ordinary page-cache budget.
    assert ('TEMP_STORE=3',) in store.db.execute('PRAGMA compile_options')
    store.db.execute('PRAGMA cache_size=8')
    store.db.execute('CREATE TEMP TABLE forced_sort(value TEXT)')
    store.db.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO forced_sort SELECT printf('%06d',x)||? FROM n",(marker,))
    assert len(store.db.execute('SELECT value FROM forced_sort ORDER BY value DESC'))==10000
    store.db.execute('DROP TABLE forced_sort');store.db.execute('PRAGMA cache_size=-2000')
    probes=[marker.encode(),docs[8]['text'].encode(),matrix[0].tobytes()[:256]]
    checked=[]
    for path in (store.path,Path(str(store.path)+'-wal')):
        assert path.exists() and path.stat().st_size>0
        content=path.read_bytes();assert not any(probe in content for probe in probes)
        checked.append({'kind':'WAL' if str(path).endswith('-wal') else 'database','bytes':len(content)})
    blocked(lambda: Database(store.path,secrets.token_bytes(32),store.engine,False))
    standard=subprocess.run(['/usr/bin/sqlite3',str(store.path),'SELECT count(*) FROM sqlite_schema;'],capture_output=True)
    assert standard.returncode!=0
    return {'file_canaries_absent':checked,'wrong_key_refused':True,'system_sqlite_no_key_refused':True,'temp_store_compile_time_memory_only':True,'forced_sort_rows':10000}


def hybrid(store,query,vector):
    term=phrase(query)
    match='"'+term.replace('"','""')+'"' if term else ' OR '.join('"'+w.replace('"','""')+'"' for w in sorted(set(words(query['text']))))
    lexical=[row[0] for row in store.keyword(match,query,raw['now'],store.epoch)] if match else []
    semantic=[row[0] for row in store.query(vector,query,raw['now'],store.epoch,PROFILE,limit=50)]
    ranked=fused_rank(lexical,semantic)
    if term is not None:
        ranked=[i for i in ranked if term in (docs[i-1]['title']+'\n\n'+docs[i-1]['text']).lower()]
    elif re.fullmatch(r'[A-Z]+(?:-[A-Z0-9]+)+',query['text']):
        exact=[i for i in lexical if query['text'] in (docs[i-1]['title']+'\n\n'+docs[i-1]['text'])]
        ranked=exact+[i for i in ranked if i not in exact]
    return ranked


def lifecycle(store,key):
    vector=query_vectors[38].tobytes();query=raw['queries'][38]
    # Failed full publication leaves no independently visible index components.
    blocked(lambda:store.publish(999,{**docs[2],'id':'failed-publication'},document_vectors[2].tobytes(),fail=True))
    for table,column in (('sources','id'),('passages','id'),('words','rowid'),('vectors','rowid')):
        assert store.db.execute('SELECT count(*) FROM '+table+' WHERE '+column+'=999')==[(0,)]
    # Removal is durable even if the derived cleanup checkpoint fails.
    store.remove(3);blocked(lambda:store.clean(3,fail=True))
    assert 3 not in [r[0] for r in store.query(vector,query,raw['now'],store.epoch,PROFILE)]
    blocked(lambda:store.passage(3,raw['now'],store.epoch))
    assert store.db.execute('SELECT id FROM cleanup WHERE id=3')==[(3,)]
    old_epoch=store.epoch;store.lock()
    blocked(lambda:store.query(vector,query,raw['now'],old_epoch,PROFILE))
    store.unlock(key)
    blocked(lambda:store.query(vector,query,raw['now'],old_epoch,PROFILE))
    assert 3 not in [r[0] for r in store.query(vector,query,raw['now'],store.epoch,PROFILE)]
    blocked(lambda:store.publish(3,docs[2],document_vectors[2].tobytes(),expected=1))
    blocked(lambda:store.publish(3,docs[2],document_vectors[2].tobytes(),expected=2))
    store.clean(3)
    for table,column in (('passages','id'),('words','rowid'),('vectors','rowid'),('cleanup','id')):
        assert store.db.execute('SELECT count(*) FROM '+table+' WHERE '+column+'=3')==[(0,)]
    assert store.db.execute('SELECT removed,generation FROM sources WHERE id=3')==[(1,2)]
    blocked(lambda:store.query(vector,query,raw['now'],store.epoch,'other-512d-profile'))
    blocked(lambda:Store(store.path,key,store.engine,'other-512d-profile',False,store.metadata))
    # Explicit time is checked on each query, not only when inserting the vector.
    assert store.query(vector,query,2000,store.epoch,PROFILE)==[]
    return {'failed_publication_atomic':True,'failed_cleanup_still_suppressed_warm_and_reopened':True,'old_and_current_generation_revival_refused':True,'old_lifetime_refused_after_unlock':True,'profile_substitution_refused':True,'expiry_checked_at_query':True}


def scale(engine,metadata,count):
    path=run/(engine+('-meta' if metadata else '')+'-'+str(count)+'.db')
    key=secrets.token_bytes(32);store=Store(path,key,engine,PROFILE,True,metadata)
    rng=np.random.default_rng(20261025)
    matrix=rng.standard_normal((count,512),dtype=np.float32);matrix/=np.linalg.norm(matrix,axis=1,keepdims=True)
    start=time.perf_counter();store.db.execute('BEGIN IMMEDIATE')
    s=store.db.statement('INSERT INTO sources VALUES(?,?,?,?,?,?,?,?,?,?,?,?)')
    v=store.db.statement('INSERT INTO vectors(rowid,vector'+(',profile,account,retained,removed,expires,alias,sender,kind,received' if metadata else '')+') VALUES('+','.join('?' for _ in range(11 if metadata else 2))+')')
    try:
        for i,vector in enumerate(matrix):
            alias=str(i%1000)
            s.execute((i+1,'scale-'+str(i),PROFILE,'reference',1,0,2000,alias,'fixture','message',900,1))
            data=(i+1,vector.tobytes())
            v.execute(data+(PROFILE,'reference',1,0,2000,alias,'fixture','message',900) if metadata else data)
    finally:s.close();v.close()
    store.db.execute('COMMIT');store.flat();store.db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
    insertion=time.perf_counter()-start
    queries=rng.standard_normal((12,512),dtype=np.float32);queries/=np.linalg.norm(queries,axis=1,keepdims=True)
    timings={};plans={}
    for label,filters in (('broad',{}),('alias_0.1_percent',{'alias':'7'})):
        print('Scale',engine,'metadata',metadata,count,label,flush=True)
        query={'filters':filters};ids=set(range(1,count+1)) if not filters else {i+1 for i in range(count) if i%1000==7}
        latency=[];verified=0;cancelled=False
        try:
            store.db.deadline(2)
            store.query(queries[0].tobytes(),query,1000,store.epoch,PROFILE)
            for repeat in range(3):
                for index,vector in enumerate(queries):
                    store.db.deadline(2)
                    begin=time.perf_counter();rows=store.query(vector.tobytes(),query,1000,store.epoch,PROFILE);latency.append(time.perf_counter()-begin)
                    if repeat==0:parity(rows,matrix,vector,ids);verified+=1
        except RuntimeError as error:
            if 'interrupted' not in str(error):raise
            cancelled=True
        finally:store.db.deadline()
        timings[label]={'p50_ms':float(np.percentile(latency,50)*1000) if latency else None,'p95_ms':float(np.percentile(latency,95)*1000) if latency else None,'samples':len(latency),'parity_queries':verified,'eligible_count':len(ids),'deadline_exceeded':cancelled,'deadline_seconds':2}
        plans[label]=store.query(queries[0].tobytes(),query,1000,store.epoch,PROFILE,True)
    size=path.stat().st_size;store.lock();begin=time.perf_counter();store.unlock(key);reopen=time.perf_counter()-begin
    store.query(queries[0].tobytes(),{},1000,store.epoch,PROFILE);store.lock()
    return {'count':count,'synthetic_distribution':'seeded Gaussian unit vectors, not model outputs','insert_and_index_seconds':insertion,'database_bytes':size,'reopen_seconds':reopen,'timings':timings,'plans':plans}


for label,engine,metadata in (('vec1_streaming','vec1',False),('vec1_metadata','vec1',True),('sqlite_vec','vec0',False)):
    print('Start',label,flush=True)
    key=secrets.token_bytes(32);store=Store(run/(label+'.db'),key,engine,PROFILE,True,metadata)
    arm={'versions':{'sqlite':store.db.execute('SELECT sqlite_version()'),'sqlcipher':store.db.execute('PRAGMA cipher_version'),'crypto':store.db.execute('PRAGMA cipher_provider'),'extension':store.db.execute('SELECT '+('vec1_info()' if engine=='vec1' else 'vec_debug()'))},'compile_options':store.db.execute('PRAGMA compile_options'),'queries':[]}
    for i,doc in enumerate(docs):store.publish(i+1,doc,document_vectors[i].tobytes())
    store.flat()
    for i,query in enumerate(raw['queries']):
        ids={j+1 for j,doc in enumerate(docs) if eligible(doc,query,raw['now'])}
        rows=store.query(query_vectors[i].tobytes(),query,raw['now'],store.epoch,PROFILE)
        parity(rows,document_vectors,query_vectors[i],ids)
        for identifier,distance in rows:
            passage=store.passage(identifier,raw['now'],store.epoch)
            assert passage['text']==docs[identifier-1]['text'] and passage['name']==docs[identifier-1]['id']
        ranked=hybrid(store,query,query_vectors[i].tobytes())
        names=[docs[j-1]['id'] for j in ranked]
        arm['queries'].append({'id':query['id'],'split':query['split'],'category':query['category'],'vector_ids':[docs[j-1]['id'] for j,_ in rows],'hybrid_ids':names[:10],'hybrid_metrics':metrics(names,query['relevant'])})
    arm['sql_plan']=store.query(query_vectors[34].tobytes(),raw['queries'][34],1000,store.epoch,PROFILE,True)
    arm['protection']=file_checks(store,document_vectors)
    arm['lifecycle']=lifecycle(store,key)
    store.lock()
    arm['scale']=[scale(engine,metadata,count) for count in (10000,100000)]
    arm['process_rss_after_scale_bytes']=psutil.Process().memory_info().rss
    result['arms'][label]=arm
    print('Complete',label,flush=True)

# One actual inference request exercises the full path, separately from store timings.
os.environ.update(HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',DO_NOT_TRACK='1',TOKENIZERS_PARALLELISM='false')
os.environ.pop('PYTORCH_ENABLE_MPS_FALLBACK',None)
import torch
from sentence_transformers import SentenceTransformer
torch.set_num_threads(4);torch.set_num_interop_threads(1)
snapshot=MODELPATH/'huggingface/hub/models--google--embeddinggemma-2/snapshots'/model_result['revision']
start=time.perf_counter();model=SentenceTransformer(str(snapshot),device='mps',local_files_only=True,trust_remote_code=False,model_kwargs={'dtype':torch.float32},config_kwargs={'vision_config':None,'audio_config':None});torch.mps.synchronize()
result['live_model_load_seconds']=time.perf_counter()-start
model.max_seq_length=2048
with torch.inference_mode():model.encode(['Local fictional correspondence warmup.'],prompt='task: search result | query: ',show_progress_bar=False)
torch.mps.synchronize()
result['live_query']={}
query=raw['queries'][1]
for label,engine,metadata in (('vec1_streaming','vec1',False),('vec1_metadata','vec1',True),('sqlite_vec','vec0',False)):
    key=secrets.token_bytes(32);store=Store(run/(label+'-live.db'),key,engine,PROFILE,True,metadata)
    for i,doc in enumerate(docs):store.publish(i+1,doc,document_vectors[i].tobytes())
    store.flat();start=time.perf_counter()
    with torch.inference_mode():vector=model.encode([query['text']],prompt='task: search result | query: ',normalize_embeddings=False,show_progress_bar=False,convert_to_numpy=True)[0,:512].astype(np.float32)
    torch.mps.synchronize();vector/=np.linalg.norm(vector)
    assert np.allclose(vector,query_vectors[1],atol=2e-5)
    ranked=hybrid(store,query,vector.tobytes());passages=[store.passage(i,1000,store.epoch)['name'] for i in ranked[:5]]
    result['live_query'][label]={'query':query['id'],'seconds_encode_search_passage':time.perf_counter()-start,'passages':passages,'vector_matches_prior_profile':True};store.lock()

assert all(digest(ROOT/path)==sha for path,sha in inputs.items()),'Frozen inputs changed during run'
result['compiler']=subprocess.check_output(['clang','--version'],text=True)
result['timestamp_utc']=datetime.now(timezone.utc).isoformat()
result['max_process_rss_bytes']=__import__('resource').getrusage(__import__('resource').RUSAGE_SELF).ru_maxrss
result['packages']={p:importlib.metadata.version(p) for p in ('numpy','torch','sentence-transformers','transformers','psutil')}
write_json(WORK/'results.json',result)
destination=HERE/'results';destination.mkdir(exist_ok=True)
shutil.copyfile(WORK/'results.json',destination/'results.json');shutil.copyfile(freeze,destination/'frozen.json')
shutil.rmtree(run)  # This exact directory was created above and contains only test DBs.
print('Saved results; removed temporary encrypted databases. No key was persisted.',flush=True)
