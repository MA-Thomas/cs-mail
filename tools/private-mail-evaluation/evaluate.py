"""Local private-mail experiment. Never print correspondence or persist plaintext derivatives."""
from __future__ import annotations
import argparse
from collections import Counter
from datetime import datetime, timezone
from email import policy
from email.parser import BytesParser
from email.utils import parsedate_to_datetime
import hashlib
from html.parser import HTMLParser
import json
import logging
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import sys
import tempfile
import threading
import time
import textwrap
import zipfile
from zoneinfo import ZoneInfo

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path[:0] = [str(ROOT/'tools/embedding-evaluation'), str(ROOT/'tools/vector-store-evaluation')]
from common import digest
from sqlcipher_api import Database, LIB, WORK

PHASE = 'configuration'
PROBES = [
    'What travel arrangements did I discuss?', 'Find a receipt for a purchase.',
    'What appointment was scheduled?', 'Find correspondence about a refund.',
    'What documents did someone ask me to provide?', 'Find a subscription renewal notice.',
    'What did we decide about an upcoming meeting?', 'Find an invitation to an event.',
    'What delivery was delayed?', 'Find instructions for accessing an account.',
    'What payment was due?', 'Find a discussion about a research project.',
]
QUERY_PROMPT = 'task: search result | query: '
GRADE_PROMPT = 'Your rating [2/1/0/?/f]: '
ZONE = ZoneInfo('America/New_York')
LOW = int(datetime(2024,1,1,tzinfo=ZONE).timestamp())
HIGH = int(datetime(2025,1,1,tzinfo=ZONE).timestamp())
MODEL = 'google/embeddinggemma-2'
REVISION = '914f7f89142e33e77833254d9c9b90c3cef7303b'
MODEL_WORK = ROOT/'target/embedding-evaluation'
SNAPSHOT = MODEL_WORK/'huggingface/hub/models--google--embeddinggemma-2/snapshots'/REVISION

def write(path, value):
    data = json.dumps(value,indent=2,allow_nan=False)+'\n'
    with path.open('x',encoding='utf-8') as f: f.write(data)
    path.chmod(0o600)

class InertText(HTMLParser):
    hidden = {'script','style','head','template','noscript'}
    blocks = {'br','p','div','li','tr','table','section','article','blockquote','h1','h2','h3','h4','h5','h6'}
    def __init__(self):
        super().__init__(convert_charrefs=True); self.stack=[]; self.parts=[]
    def handle_starttag(self, tag, attrs):
        if tag in self.hidden: self.stack.append(tag)
        elif not self.stack and tag in self.blocks: self.parts.append('\n')
    def handle_startendtag(self, tag, attrs):
        if not self.stack and tag in self.blocks: self.parts.append('\n')
    def handle_endtag(self, tag):
        if tag in self.stack:
            at=len(self.stack)-1-self.stack[::-1].index(tag); del self.stack[at:]
        elif not self.stack and tag in self.blocks: self.parts.append('\n')
    def handle_data(self, data):
        if not self.stack: self.parts.append(data)
    def text(self):
        return '\n'.join(line for line in (re.sub(r'\s+',' ',p).strip() for p in ''.join(self.parts).splitlines()) if line)

def canonical(message):
    selected=message.get_body(preferencelist=('plain','html'))
    if selected is None: return '',None,None
    def decode(part):
        value=part.get_content()
        if not isinstance(value,str): raise ValueError('Unsupported body')
        if part.get_content_type()=='text/html':
            parser=InertText(); parser.feed(value); parser.close(); return parser.text()
        return value.replace('\r\n','\n').replace('\r','\n').strip()
    value=decode(selected)
    if not value.strip() and selected.get_content_type()=='text/plain':
        alternative=message.get_body(preferencelist=('html',))
        if alternative is not None:
            html=decode(alternative)
            if html.strip(): selected=alternative; value=html
    part=next(i for i,p in enumerate(message.walk()) if p is selected)
    return value,selected.get_content_type(),part

def messages(archive):
    with zipfile.ZipFile(archive) as z:
        names=[i for i in z.infolist() if i.filename.endswith('.mbox')]
        assert len(names)==1 and names[0].file_size<=64*1024**2
        assert not names[0].flag_bits&1
        with z.open(names[0]) as f:
            raw=bytearray(); found=False
            for line in f:
                if line.startswith(b'From '):
                    if found: yield bytes(raw)
                    raw=bytearray(); found=True
                elif found: raw.extend(line)
            if found: yield bytes(raw)

def read_corpus(archive):
    stats=Counter(); docs=[]; parser=BytesParser(policy=policy.default)
    for ordinal,raw in enumerate(messages(archive),1):
        stats['source_messages']+=1; message=parser.parsebytes(raw)
        try:
            date=parsedate_to_datetime(str(message.get('Date','')))
            if date.tzinfo is None: raise ValueError('Ambiguous date')
            sent=int(date.timestamp())
        except (ValueError,TypeError,OverflowError): stats['quarantined_date']+=1; continue
        if not LOW<=sent<HIGH: stats['excluded_outside_2024']+=1; continue
        stats['date_eligible_messages']+=1
        for part in message.walk():
            if part.get_content_disposition()=='attachment': stats['attachment_parts_not_extracted']+=1
        try: text,kind,part=canonical(message)
        except (LookupError,ValueError,UnicodeError,KeyError): stats['body_decode_failures']+=1; continue
        if not text.strip(): stats['empty_or_unsupported_bodies']+=1; continue
        if kind=='text/html' and message.get_body(preferencelist=('plain','html')).get_content_type()=='text/plain': stats['empty_plain_html_fallbacks']+=1
        stats['plain_bodies' if kind=='text/plain' else 'html_fallback_bodies']+=1
        stats['replacement_characters']+=text.count('\ufffd')
        stats['indexed_messages']+=1
        docs.append({'id':ordinal,'raw':raw,'source_hash':hashlib.sha256(raw).hexdigest(),
                     'text':text,'subject':str(message.get('Subject','')),'sent':sent,'part':part,
                     'sender_hash':hashlib.sha256(str(message.get('From','')).encode()).hexdigest()})
    return docs,dict(stats)

def chunk(docs, tokenizer):
    passages=[]
    for doc in docs:
        offsets=tokenizer(doc['text'],add_special_tokens=False,return_offsets_mapping=True)['offset_mapping']
        offsets=[(a,b) for a,b in offsets if b>a]
        assert offsets
        last_end=0
        for at in range(0,len(offsets),320):
            end_token=min(at+384,len(offsets))
            a=offsets[at][0] if at else 0
            b=len(doc['text']) if end_token==len(offsets) else offsets[end_token-1][1]
            assert 0<=a<b<=len(doc['text']) and not doc['text'][last_end:a].strip()
            text=doc['text'][a:b]; start=len(doc['text'][:a].encode()); end=start+len(text.encode())
            prepared='title: '+doc['subject']+' | text: '+text
            length=len(tokenizer.encode(prepared,add_special_tokens=True))
            assert length<=2048, 'Model input must never silently truncate'
            passages.append({'id':len(passages)+1,'doc':doc,'text':text,'prepared':prepared,
                             'start':start,'end':end,'checksum':hashlib.sha256(text.encode()).hexdigest(),'tokens':length})
            last_end=max(last_end,b)
            if end_token==len(offsets): break
        assert last_end==len(doc['text'])
    assert 0<len(passages)<=20000
    return passages

class Index:
    def __init__(self,path,key,engine,profile,docs,passages,vectors):
        self.engine=engine; self.path=path; self.profile=profile
        self.db=Database(path,key,engine,True)
        self.db.execute('PRAGMA cache_size=-262144'); self.db.execute('PRAGMA mmap_size=0')
        assert ('TEMP_STORE=3',) in self.db.execute('PRAGMA compile_options')
        self.db.execute('CREATE TABLE profile(value TEXT NOT NULL)')
        self.db.execute('INSERT INTO profile VALUES(?)',(profile,))
        self.db.execute('CREATE TABLE messages(id INTEGER PRIMARY KEY,source_hash TEXT NOT NULL,part INTEGER NOT NULL,sent INTEGER NOT NULL,sender_hash TEXT NOT NULL,subject TEXT NOT NULL,body TEXT NOT NULL)')
        self.db.execute('CREATE TABLE passages(id INTEGER PRIMARY KEY,message INTEGER NOT NULL,start INTEGER NOT NULL,end INTEGER NOT NULL,checksum TEXT NOT NULL)')
        self.db.execute('CREATE VIRTUAL TABLE words USING fts5(title,body)')
        self.db.execute('CREATE VIRTUAL TABLE vectors USING '+('vec1(vector,message,sent)' if engine=='vec1' else 'vec0(vector float[512] distance_metric=cosine)'))
        self.db.execute('BEGIN IMMEDIATE')
        try:
            for d in docs: self.db.execute('INSERT INTO messages VALUES(?,?,?,?,?,?,?)',(d['id'],d['source_hash'],d['part'],d['sent'],d['sender_hash'],d['subject'],d['text']))
            for p,v in zip(passages,vectors):
                d=p['doc']; self.db.execute('INSERT INTO passages VALUES(?,?,?,?,?)',(p['id'],d['id'],p['start'],p['end'],p['checksum']))
                self.db.execute('INSERT INTO words(rowid,title,body) VALUES(?,?,?)',(p['id'],d['subject'],p['text']))
                self.db.execute('INSERT INTO vectors(rowid,vector'+(',message,sent' if engine=='vec1' else '')+') VALUES('+('?,?,?,?' if engine=='vec1' else '?,?')+')',(p['id'],v.tobytes(),d['id'],d['sent']) if engine=='vec1' else (p['id'],v.tobytes()))
            self.db.execute('COMMIT')
        except BaseException: self.db.execute('ROLLBACK'); raise
        if engine=='vec1': self.db.execute('INSERT INTO vectors(cmd,arg) VALUES(\'rebuild\',\'{index:"flat",distance:"cos"}\')')
        self.db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
    def query(self,vector,k=10,profile=None):
        if profile is not None and profile!=self.profile: raise ValueError('Wrong profile')
        assert self.db.execute('SELECT value FROM profile')==[(self.profile,)]
        if self.engine=='vec1':
            return self.db.execute("WITH nearest AS MATERIALIZED (SELECT v.rowid,v.distance FROM vectors(?,'{K:"+str(k)+",streaming:1}') v CROSS JOIN messages m ON m.id=v.message WHERE m.sent>=? AND m.sent<? AND v.sent>=? AND v.sent<? LIMIT "+str(k)+') SELECT rowid,distance FROM nearest ORDER BY distance,rowid',(vector.tobytes(),LOW,HIGH,LOW,HIGH))
        return self.db.execute('SELECT rowid,distance FROM vectors WHERE vector MATCH ? AND k='+str(k)+' AND rowid IN (SELECT p.id FROM passages p JOIN messages m ON m.id=p.message WHERE m.sent>=? AND m.sent<?) ORDER BY distance',(vector.tobytes(),LOW,HIGH))
    def verify(self,identifier,expected):
        rows=self.db.execute('SELECT m.id,m.source_hash,m.part,m.subject,m.body,p.start,p.end,p.checksum FROM passages p JOIN messages m ON m.id=p.message WHERE p.id=? AND m.sent>=? AND m.sent<?',(identifier,LOW,HIGH))
        assert len(rows)==1
        message,source,part,subject,body,start,end,checksum=rows[0]; original=expected[identifier-1]
        assert source==hashlib.sha256(original['doc']['raw']).hexdigest()
        assert message==original['doc']['id'] and part==original['doc']['part'] and body==original['doc']['text']
        selected=body.encode()[start:end]; assert hashlib.sha256(selected).hexdigest()==checksum==original['checksum']
        assert selected.decode()==original['text']
    def lexical(self,query):
        terms=sorted(set(re.findall(r'[^\W_]+',query.lower(),re.UNICODE)))
        if not terms: return []
        match=' OR '.join('"'+term.replace('"','""')+'"' for term in terms)
        return [r[0] for r in self.db.execute('SELECT rowid FROM words WHERE words MATCH ? ORDER BY bm25(words),rowid LIMIT 50',(match,))]
    def close(self): self.db.close()

def parity(rows,matrix,vector,np):
    scores=matrix@vector; n=min(10,len(matrix)); threshold=np.partition(1-scores,n-1)[n-1]
    assert len(rows)==n and len({i for i,_ in rows})==n
    for identifier,distance in rows:
        actual=1-float(scores[identifier-1]); assert abs(actual-distance)<1e-5 and actual<=threshold+1e-5

def distinct(ids,passages):
    seen=set(); result=[]
    for identifier in ids:
        message=passages[identifier-1]['doc']['id']
        if message not in seen: result.append(identifier); seen.add(message)
    return result

def method_rankings(semantic_ids,lexical_ids,passages):
    semantic=distinct(semantic_ids,passages); lexical=distinct(lexical_ids,passages)
    score={}; representative={}
    for ranking in (semantic,lexical):
        for at,identifier in enumerate(ranking):
            message=passages[identifier-1]['doc']['id']
            representative.setdefault(message,identifier)
            score[message]=score.get(message,0)+1/(60+at+1)
    hybrid=[representative[message] for message in sorted(score,key=lambda message:(-score[message],message))]
    return {'semantic':semantic[:5],'fts5':lexical[:5],'hybrid_rrf60':hybrid[:5]}

class ReviewProgress:
    """Owner annotations only; no source text, addresses, query text or rankings."""
    def __init__(self,path,binding):
        self.path=path
        self.data={'version':1,'binding':binding,'grades':{}}
        if path.exists():
            loaded=json.loads(path.read_text())
            assert loaded['version']==1 and loaded['binding']==binding,'Review inputs changed'
            assert isinstance(loaded['grades'],dict)
            for query,grades in loaded['grades'].items():
                assert isinstance(query,str) and isinstance(grades,dict)
                for message,grade in grades.items():
                    assert message.isdecimal() and int(message)>0
                    assert grade is None or (type(grade) is int and grade in (0,1,2))
            self.data=loaded
    def grades(self,query): return {int(k):v for k,v in self.data['grades'].get(query,{}).items()}
    def record(self,query,message,grade):
        self.data['grades'].setdefault(query,{})[str(message)]=grade
        temporary=self.path.with_name(self.path.name+'.tmp-'+secrets.token_hex(8))
        try:
            fd=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
            with os.fdopen(fd,'w',encoding='utf-8') as f:
                json.dump(self.data,f,indent=2,allow_nan=False); f.write('\n'); f.flush(); os.fsync(f.fileno())
            os.replace(temporary,self.path)
            parent=os.open(self.path.parent,os.O_RDONLY)
            try: os.fsync(parent)
            finally: os.close(parent)
        finally: temporary.unlink(missing_ok=True)

def owner_review(index,passages,queries,query_vectors,progress=None):
    judgments=[]
    # A terminal is not seekable. Python's buffered update-mode stream (r+)
    # requires seeking; independent input/output streams work on a real TTY.
    with open('/dev/tty','r',encoding='utf-8') as terminal_input, open('/dev/tty','w',encoding='utf-8',buffering=1) as terminal_output:
        def show(text):
            terminal_output.write(''.join(c for c in text if c in '\n\t' or not (ord(c)<32 or 127<=ord(c)<160)))
            terminal_output.flush()
        def body(text):
            show('\n'.join(textwrap.fill(line,width=76,break_long_words=False,break_on_hyphens=False) if line.strip() else '' for line in text.splitlines())+'\n')
        show('\nPrivate local review. Each rating is saved immediately.\n')
        for query_number,(query,vector) in enumerate(zip(queries,query_vectors),1):
            methods=method_rankings([i for i,_ in index.query(vector,min(50,len(passages)))],index.lexical(query['text']),passages)
            # Merge occurrences of the same message before owner grading.
            pooled={}
            for ranking in methods.values():
                for identifier in ranking:
                    choices=pooled.setdefault(passages[identifier-1]['doc']['id'],[])
                    if identifier not in choices: choices.append(identifier)
            pool=list(pooled); import random; random.Random(20261025).shuffle(pool)
            show('\n\n'+'='*76+'\nSEARCH '+str(query_number)+' OF '+str(len(queries))+'\n'+'='*76+'\n\n')
            body(query['text'])
            show('\n2 useful   1 partly useful   0 irrelevant   ? unsure   f full message\n\n')
            grades={}; prior=progress.grades(query['id']) if progress else {}
            remaining=sum(message not in prior for message in pool)
            show(str(remaining)+' messages left to rate for this search.\n')
            for at,message_id in enumerate(pool,1):
                if message_id in prior: grades[message_id]=prior[message_id]; continue
                p=passages[pooled[message_id][0]-1]; d=p['doc']
                show('\n\n'+'-'*76+'\nMESSAGE '+str(at)+' OF '+str(len(pool))+'  |  '+datetime.fromtimestamp(d['sent'],ZONE).strftime('%Y-%m-%d')+'\n')
                body('Subject: '+d['subject']); show('-'*76+'\n\n')
                for identifier in pooled[message_id]:
                    index.verify(identifier,passages); body(passages[identifier-1]['text']); show('\n')
                while True:
                    show(GRADE_PROMPT)
                    raw=terminal_input.readline()
                    if raw=='': raise EOFError('Review ended; prior ratings saved')
                    value=raw.strip()
                    if value in ('0','1','2','?'): break
                    if value=='f': show('\n'+'-'*76+'\nFULL MESSAGE\n'+'-'*76+'\n\n'); body(d['text']); show('\n'); continue
                    show('Please enter 2, 1, 0 or ?, or f to read the full message.\n')
                grades[d['id']]=None if value=='?' else int(value)
                if progress: progress.record(query['id'],d['id'],grades[d['id']])
                show('Rating saved.\n')
            observed={}
            for method,ranking in methods.items():
                values=[grades[passages[i-1]['doc']['id']] for i in ranking]
                known=[v for v in values if v is not None]
                observed[method]={'returned':len(values),'judged':len(known),'useful_count':sum(v==2 for v in known),'partial_count':sum(v==1 for v in known),'useful_first':values[0]==2 if values and values[0] is not None else None,'useful_fraction_of_judged':sum(v==2 for v in known)/len(known) if known else None}
            judgments.append({'query_id':query['id'],'no_answer_expected':query.get('no_answer_expected',False),'methods':observed,'corpus_recall':'unknown'})
            show('\nSearch '+str(query_number)+' complete.\n')
    return judgments

def main():
    global PHASE
    parser=argparse.ArgumentParser()
    parser.add_argument('--archive',type=Path,required=True); parser.add_argument('--output-directory',type=Path,required=True)
    parser.add_argument('--query-file',type=Path); parser.add_argument('--review',action='store_true')
    parser.add_argument('--review-progress',type=Path)
    args=parser.parse_args(); archive=args.archive.resolve(); output=args.output_directory.resolve()
    assert ROOT not in output.parents and output!=ROOT and output!=archive.parent and not output.exists()
    assert not args.review or args.query_file
    if args.review_progress:
        progress_path=args.review_progress.resolve()
        assert progress_path!=ROOT and ROOT not in progress_path.parents and progress_path.parent.exists()
    os.umask(0o077); output.mkdir(parents=True,mode=0o700)
    os.environ.update(HF_HOME=str(MODEL_WORK/'huggingface'),HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',HF_HUB_DISABLE_PROGRESS_BARS='1',DO_NOT_TRACK='1',TOKENIZERS_PARALLELISM='false',TRANSFORMERS_VERBOSITY='error')
    os.environ.pop('PYTORCH_ENABLE_MPS_FALLBACK',None)
    def deny_network(*args,**kwargs): raise PermissionError('Network disabled for private evaluation')
    socket.socket.connect=deny_network; socket.socket.connect_ex=deny_network; socket.create_connection=deny_network
    logging.disable(logging.WARNING)
    baseline=json.loads((MODEL_WORK/'gemma2.json').read_text())
    assert baseline['model']==MODEL and baseline['revision']==REVISION
    for name,expected in baseline['artifacts_sha256'].items(): assert digest(SNAPSHOT/name)==expected
    profile={'model':MODEL,'revision':REVISION,'dimensions':512,'dtype':'float32','normalization':'truncate_then_l2','query_prompt':QUERY_PROMPT,'document':'title: {subject} | text: {passage}','body_tokens':384,'overlap_tokens':64,'canonicalizer':'private-mail-evaluation-v2-empty-plain-html-fallback','scope':'2024 America/New_York','inference_batch':4}
    profile_id=hashlib.sha256(json.dumps(profile,sort_keys=True).encode()).hexdigest()
    frozen={'utc':datetime.now(timezone.utc).isoformat(),'archive_sha256':digest(archive),'profile':profile,'profile_id':profile_id,'inputs':{str(p.relative_to(ROOT)):digest(p) for p in [Path(__file__),HERE/'README.md',LIB,WORK/'vec1.dylib',WORK/'vec0.dylib',ROOT/'tools/vector-store-evaluation/sqlcipher_api.py']},'queries_sha256':digest(args.query_file) if args.query_file else None,'probes':PROBES,'model_artifacts_verified':True}
    write(output/'frozen.json',frozen)
    summary={'profile_id':profile_id,'privacy':{'network_disabled':True,'correspondence_output_to_stdout':False,'plaintext_working_files':False,'owner_store_or_keychain_accessed':False},'limits':['body text only; attachments/OCR not extracted','canonical HTML text, not original HTML DOM coordinates','small December sample, not scale validation','generic probes have no relevance judgments','no app integration, concurrent lifecycle, battery, iOS or swap/crash-dump audit']}
    start=time.perf_counter(); PHASE='mail parsing'; docs,counts=read_corpus(archive); summary['corpus']=counts
    summary['parse_seconds']=time.perf_counter()-start; assert docs
    print('Private corpus parsed; counts only:',json.dumps(counts),flush=True)
    PHASE='model initialization'
    import numpy as np
    import psutil
    import torch
    from sentence_transformers import SentenceTransformer
    torch.set_num_threads(4); torch.set_num_interop_threads(1); assert torch.backends.mps.is_available()
    state={'rss':0,'metal_driver':0}; done=threading.Event(); process=psutil.Process()
    def monitor():
        while not done.wait(.05):
            state['rss']=max(state['rss'],process.memory_info().rss); state['metal_driver']=max(state['metal_driver'],torch.mps.driver_allocated_memory())
    thread=threading.Thread(target=monitor,daemon=True); thread.start()
    start=time.perf_counter()
    model=SentenceTransformer(str(SNAPSHOT),device='mps',trust_remote_code=False,local_files_only=True,model_kwargs={'dtype':torch.float32},config_kwargs={'vision_config':None,'audio_config':None})
    model.max_seq_length=2048; torch.mps.synchronize(); summary['model_load_seconds']=time.perf_counter()-start
    assert sum(p.numel() for p in model.parameters())==271002624
    def encode(texts,query=False):
        prompt=QUERY_PROMPT if query else ''
        lengths=[len(model.tokenizer.encode(prompt+t,add_special_tokens=True)) for t in texts]; assert max(lengths)<=2048
        with torch.inference_mode(): values=model.encode(texts,prompt=prompt,batch_size=4,show_progress_bar=False,normalize_embeddings=False,convert_to_numpy=True)
        torch.mps.synchronize(); values=values[:,:512].astype(np.float32); norms=np.linalg.norm(values,axis=1,keepdims=True)
        assert np.isfinite(values).all() and (norms>0).all(); return values/norms
    PHASE='passage mapping'; start=time.perf_counter(); passages=chunk(docs,model.tokenizer)
    summary['chunk_seconds']=time.perf_counter()-start; summary['passages']=len(passages)
    summary['passage_input_tokens']={'min':min(p['tokens'] for p in passages),'max':max(p['tokens'] for p in passages),'total':sum(p['tokens'] for p in passages)}
    encode(['Local correspondence query warmup.'],True)
    PHASE='document inference'; start=time.perf_counter(); vectors=encode([p['prepared'] for p in passages])
    summary['document_inference_seconds']=time.perf_counter()-start
    print('Private passage inference completed:',len(passages),'passages',flush=True)
    probe_vectors=encode(PROBES,True)
    owner_queries=json.loads(args.query_file.read_text()) if args.query_file else []
    assert not owner_queries or (isinstance(owner_queries,list) and all(isinstance(q['text'],str) and q['text'].strip() and len(q['text'])<=1024 for q in owner_queries))
    owner_vectors=encode([q['text'] for q in owner_queries],True) if owner_queries else None
    summary['owner_queries_supplied']=len(owner_queries); summary['quality_judged']=False; summary['engines']={}
    PHASE='encrypted index and probes'; temporary=Path(tempfile.mkdtemp(prefix='cs-mail-private-eval-')); temporary.chmod(0o700)
    opened=[]
    try:
        for engine in ('vec1','vec0'):
            key=secrets.token_bytes(32); path=temporary/(engine+'.db'); start=time.perf_counter()
            index=Index(path,key,engine,profile_id,docs,passages,vectors); opened.append(index)
            cost={'index_seconds':time.perf_counter()-start,'database_bytes':path.stat().st_size,'cache_suggestion_mib':256}
            for p in passages: index.verify(p['id'],passages)
            index.query(probe_vectors[0],min(10,len(passages)))
            times=[]; total=[]; first_rows=[]
            for repeat in range(3):
                for i,query in enumerate(PROBES):
                    start=time.perf_counter(); vector=encode([query],True)[0]; query_ms=(time.perf_counter()-start)*1000
                    start=time.perf_counter(); rows=index.query(vector,min(10,len(passages)))
                    for identifier,_ in rows: index.verify(identifier,passages)
                    retrieval_ms=(time.perf_counter()-start)*1000; times.append(retrieval_ms); total.append(query_ms+retrieval_ms)
                    if repeat==0: parity(rows,vectors,vector,np); first_rows.append([i for i,_ in rows])
            cost.update({'query_samples':len(times),'oracle_checks':12,'all_stored_passages_verified':len(passages),'retrieval_and_resolution_p50_ms':float(np.percentile(times,50)),'retrieval_and_resolution_p95_ms':float(np.percentile(times,95)),'encoding_search_resolution_p50_ms':float(np.percentile(total,50)),'encoding_search_resolution_p95_ms':float(np.percentile(total,95))})
            if engine=='vec1': reference_rows=first_rows
            else: assert reference_rows==first_rows
            try: index.query(probe_vectors[0],profile='wrong-profile')
            except ValueError: cost['wrong_profile_refused']=True
            else: raise AssertionError('Profile substitution accepted')
            marker='PRIVATE-INDEX-CANARY-'+secrets.token_hex(32)
            index.db.execute('CREATE TABLE canary(value TEXT)'); index.db.execute('INSERT INTO canary VALUES(?)',(marker,))
            probes=[marker.encode(),vectors[0].tobytes()[:256]]+[p['text'].encode() for p in passages if len(p['text'].encode())>=128][:3]
            for file in (path,Path(str(path)+'-wal')):
                data=file.read_bytes(); assert not any(p in data for p in probes)
            try: Database(path,secrets.token_bytes(32),engine,False)
            except RuntimeError: cost['wrong_key_refused']=True
            else: raise AssertionError('Wrong encryption key accepted')
            cost['database_and_live_wal_canaries_absent']=True
            if args.review and engine=='vec1':
                PHASE='owner review'
                progress=ReviewProgress(args.review_progress.resolve() if args.review_progress else output/'review-progress.json',{'archive_sha256':frozen['archive_sha256'],'profile_id':profile_id,'queries_sha256':frozen['queries_sha256']})
                summary['judgments']=owner_review(index,passages,owner_queries,owner_vectors,progress); summary['quality_judged']=True; PHASE='encrypted index and probes'
            if owner_queries and not args.review:
                # Validate owner-query paths without publishing contents/rankings.
                for query,vector in zip(owner_queries,owner_vectors):
                    rows=index.query(vector,min(10,len(passages))); parity(rows,vectors,vector,np)
                    for identifier,_ in rows: index.verify(identifier,passages)
                    index.lexical(query['text'])
                cost['owner_query_path_checks']=len(owner_queries)
            summary['engines'][engine]=cost; index.close(); opened.remove(index); del key
            print('Private engine validation completed:',engine,flush=True)
    finally:
        for index in opened: index.close()
        shutil.rmtree(temporary)
        done.set(); thread.join()
    summary['process_sampled_peak_rss_bytes']=state['rss']; summary['metal_driver_sampled_peak_bytes']=state['metal_driver']
    summary['all_temporary_indices_removed']=not temporary.exists(); summary['engines_same_probe_rankings']=True
    PHASE='aggregate report'; write(output/'summary.json',summary)
    print(json.dumps(summary,indent=2),flush=True)

if __name__=='__main__':
    try: main()
    except BaseException as error:
        # Library exceptions may contain source text: never print their message or traceback.
        print('Private evaluation stopped during '+PHASE+' ('+type(error).__name__+'); no source text logged.',file=sys.stderr)
        import traceback
        print(json.dumps([{'file':Path(f.filename).name,'line':f.lineno,'function':f.name} for f in traceback.extract_tb(error.__traceback__)]),file=sys.stderr)
        raise SystemExit(1) from None
