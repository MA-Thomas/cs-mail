"""Offline numerical/storage adapter. Rust owns sources, profiles, publication and ranking."""
import hashlib
import json
import logging
import os
from pathlib import Path
import re
import socket
import struct
import sys

ROOT=Path(__file__).resolve().parent
os.environ.update(HF_HOME=str(ROOT/'model'),HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',HF_HUB_DISABLE_PROGRESS_BARS='1',DO_NOT_TRACK='1',TOKENIZERS_PARALLELISM='false',TRANSFORMERS_VERBOSITY='error',PYTHONDONTWRITEBYTECODE='1')
os.environ.pop('PYTORCH_ENABLE_MPS_FALLBACK',None)
def denied(*args,**kwargs): raise PermissionError('Offline runtime')
socket.socket.connect=denied; socket.socket.connect_ex=denied; socket.create_connection=denied
logging.disable(logging.WARNING)
sys.path.insert(0,str(ROOT))
from sqlcipher import Database
import numpy as np
import torch
from sentence_transformers import SentenceTransformer
torch.set_num_threads(4); torch.set_num_interop_threads(1)
assert torch.backends.mps.is_available()
model=None; database=None; known={}; active_profile=None; dirty=False; prepared_cache=None; session_directory=None

def encoder():
    global model
    if model is None:
        model=SentenceTransformer(str(ROOT/'model'),device='mps',trust_remote_code=False,local_files_only=True,model_kwargs={'dtype':torch.float32},config_kwargs={'vision_config':None,'audio_config':None})
        model.max_seq_length=2048
        assert sum(p.numel() for p in model.parameters())==271002624
    return model

def encode(texts,query=False):
    encoder(); prompt='task: search result | query: ' if query else ''
    assert texts and all(len(model.tokenizer.encode(prompt+t,add_special_tokens=True))<=2048 for t in texts)
    with torch.inference_mode(): values=model.encode(texts,prompt=prompt,batch_size=4,show_progress_bar=False,normalize_embeddings=False,convert_to_numpy=True)
    torch.mps.synchronize(); values=values[:,:512].astype(np.float32)
    norms=np.linalg.norm(values,axis=1,keepdims=True)
    assert np.isfinite(values).all() and (norms>0).all()
    return values/norms

def prepare(request):
    global prepared_cache
    tokenizer=encoder().tokenizer; windows=[]
    title_offsets=tokenizer(request['title'],add_special_tokens=False,return_offsets_mapping=True)['offset_mapping']
    title=request['title'][:title_offsets[127][1]] if len(title_offsets)>128 else request['title']
    identity=hashlib.sha256(json.dumps(request['parts'],ensure_ascii=False).encode()).digest()
    parts=request['parts'] if prepared_cache is None or prepared_cache[0]!=identity else []
    for part,text in enumerate(parts):
        offsets=[(a,b) for a,b in tokenizer(text,add_special_tokens=False,return_offsets_mapping=True)['offset_mapping'] if b>a]
        for at in range(0,len(offsets),320):
            end=min(at+384,len(offsets)); a=offsets[at][0] if at else 0; b=len(text) if end==len(offsets) else offsets[end-1][1]
            if text[a:b].strip(): windows.append((part,len(text[:a].encode()),len(text[:b].encode()),text[a:b]))
            if end==len(offsets): break
    if parts: prepared_cache=(identity,windows)
    elif prepared_cache is not None and prepared_cache[0]==identity: windows=prepared_cache[1]
    assert len(windows)<=20000 and 0<=request['after']<=len(windows)
    chosen=windows[request['after']:request['after']+4]
    values=encode(['title: '+title+' | text: '+text for _,_,_,text in chosen]) if chosen else []
    return {'chunks':[{'part':part,'start':a,'end':b,'values':v.tolist()} for (part,a,b,_),v in zip(chosen,values)],'complete':request['after']+len(chosen)==len(windows)}

def handle(request):
    global database,active_profile,dirty,session_directory
    if request['op']=='open':
        assert database is None
        active_profile=request['profile']; session_directory=Path(request['directory'])
        database=Database(Path(request['directory'])/'projection.sqlite',bytes(request['key']),'vec1',True)
        database.execute('PRAGMA cache_size=-262144'); database.execute('PRAGMA mmap_size=0')
        assert ('TEMP_STORE=3',) in database.execute('PRAGMA compile_options')
        database.execute('CREATE VIRTUAL TABLE vectors USING vec1(vector,alias)')
        database.execute('CREATE VIRTUAL TABLE words USING fts5(text)')
        database.execute('CREATE TEMP TABLE permitted(id INTEGER PRIMARY KEY)')
        database.execute('CREATE TEMP TABLE inventory(id INTEGER PRIMARY KEY)')
        database.execute('CREATE TABLE profile(value TEXT NOT NULL)')
        database.execute('INSERT INTO profile VALUES(?)',(json.dumps(active_profile),))
        return {}
    assert database is not None
    if request['op']=='prepare': return prepare(request)
    if request['op']=='upsert':
        assert len(request['rows'])<=128
        database.execute('BEGIN IMMEDIATE')
        try:
            changed={}
            for row in request['rows']:
                fingerprint=hashlib.sha256(json.dumps(row,sort_keys=True).encode()).digest()
                if known.get(row['id'])==fingerprint: continue
                v=np.asarray(row['values'],dtype=np.float32)
                assert v.shape==(512,) and np.isfinite(v).all() and abs(float(v@v)-1)<.01
                database.execute('DELETE FROM vectors WHERE rowid=?',(row['id'],))
                database.execute('DELETE FROM words WHERE rowid=?',(row['id'],))
                database.execute('INSERT INTO vectors(rowid,vector,alias) VALUES(?,?,?)',(row['id'],v.tobytes(),row['alias']))
                database.execute('INSERT INTO words(rowid,text) VALUES(?,?)',(row['id'],row['text']))
                changed[row['id']]=fingerprint
            database.execute('COMMIT'); known.update(changed); dirty=dirty or bool(changed)
        except BaseException: database.execute('ROLLBACK'); raise
        return {}
    if request['op']=='query':
        assert database.execute('SELECT value FROM profile')==[(json.dumps(active_profile),)]
        database.execute('BEGIN IMMEDIATE')
        try:
            for table,ids in [('inventory',request['all']),('permitted',request['allowed'])]:
                database.execute('DELETE FROM '+table)
                statement=database.statement('INSERT INTO '+table+' VALUES(?)')
                try:
                    for identifier in ids: statement.execute((identifier,))
                finally: statement.close()
            stale=set(known)-set(request['all'])
            for identifier in stale:
                database.execute('DELETE FROM vectors WHERE rowid=?',(identifier,)); database.execute('DELETE FROM words WHERE rowid=?',(identifier,))
            database.execute('COMMIT')
            for identifier in stale: del known[identifier]
            dirty=dirty or bool(stale)
        except BaseException: database.execute('ROLLBACK'); raise
        query=request['query']; vector=encode([query],True)[0]
        if dirty:
            database.execute('INSERT INTO vectors(cmd,arg) VALUES(\'rebuild\',\'{index:"flat",distance:"cos"}\')');dirty=False
        database.deadline(2)
        try:
            constraint=' AND v.alias=?' if request.get('scope') is not None else ''
            parameters=(vector.tobytes(),request['scope']) if constraint else (vector.tobytes(),)
            semantic=database.execute("WITH nearest AS MATERIALIZED (SELECT v.rowid,v.distance FROM vectors(?,'{K:100,streaming:1}') v CROSS JOIN permitted p ON p.id=v.rowid WHERE 1=1"+constraint+" LIMIT 100) SELECT rowid,distance FROM nearest ORDER BY distance,rowid",parameters)
            literal=query.strip()
            terms=re.findall(r'[^\W_]+',query.lower(),re.UNICODE)
            match='"'+literal[1:-1].replace('"','""')+'"' if literal.startswith('"') and literal.endswith('"') else ' OR '.join('"'+t.replace('"','""')+'"' for t in sorted(set(terms)))
            lexical=[r[0] for r in database.execute('SELECT w.rowid FROM words w JOIN permitted p ON p.id=w.rowid WHERE words MATCH ? ORDER BY bm25(words),w.rowid LIMIT 100',(match,))] if match else []
        finally: database.deadline()
        return {'semantic':semantic,'lexical':lexical,'vector':vector.tolist()}
    raise ValueError('Unknown operation')

def read_exact(n):
    result=bytearray()
    while len(result)<n:
        data=sys.stdin.buffer.read(n-len(result))
        if not data: raise EOFError()
        result.extend(data)
    return result

try:
    while True:
        size=struct.unpack('>I',read_exact(4))[0]; assert size<=64*1024**2
        try: response=handle(json.loads(read_exact(size)))
        except BaseException: response={'error':'Local retrieval unavailable'}
        data=json.dumps(response,separators=(',',':'),allow_nan=False).encode()
        assert len(data)<=64*1024**2
        sys.stdout.buffer.write(struct.pack('>I',len(data))+data); sys.stdout.buffer.flush()
except EOFError:
    pass
finally:
    if database: database.close()
    # Parent exit closes the anonymous pipe. Its normal app termination need not
    # run Rust destructors; the child therefore removes its own encrypted cache.
    if session_directory is not None:
        for name in ('projection.sqlite','projection.sqlite-wal','projection.sqlite-shm','projection.sqlite-journal'):
            try: (session_directory/name).unlink(missing_ok=True)
            except OSError: pass
        try: session_directory.rmdir()
        except OSError: pass
