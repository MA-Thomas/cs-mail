"""Experimental SQL plans/lifetimes over explicit fictional source facts, not app authority."""
from __future__ import annotations

import hashlib
import json

from sqlcipher_api import Database


class Store:
    def __init__(self, path, key, engine, profile, create=False, metadata=False):
        self.path, self.engine, self.profile = path, engine, profile
        self.metadata=metadata
        self.epoch = 1
        self.db = Database(path, key, engine, create)
        if create:
            self.db.execute("CREATE TABLE config(profile TEXT NOT NULL)")
            self.db.execute("INSERT INTO config VALUES(?)", (profile,))
            self.db.execute("CREATE TABLE sources(id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL, profile TEXT NOT NULL, account TEXT NOT NULL, retained INTEGER NOT NULL, removed INTEGER NOT NULL, expires INTEGER NOT NULL, alias TEXT NOT NULL, sender TEXT NOT NULL, kind TEXT NOT NULL, received INTEGER NOT NULL, generation INTEGER NOT NULL)")
            self.db.execute("CREATE INDEX source_scope ON sources(profile,account,retained,removed,alias,expires,received,sender,kind)")
            self.db.execute("CREATE TABLE passages(id INTEGER PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL, checksum TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL)")
            self.db.execute("CREATE TABLE cleanup(id INTEGER PRIMARY KEY, generation INTEGER NOT NULL)")
            self.db.execute("CREATE VIRTUAL TABLE words USING fts5(title,body)")
            extra=',profile,account,retained,removed,expires,alias,sender,kind,received' if metadata else ''
            self.db.execute("CREATE VIRTUAL TABLE vectors USING " + (("vec1(vector"+extra+")") if engine == "vec1" else "vec0(vector float[512] distance_metric=cosine)"))
        if self.db.execute("SELECT profile FROM config") != [(profile,)]:
            self.lock()
            raise ValueError("Incompatible model profile")

    def check(self, epoch, profile):
        if not self.db.handle or epoch != self.epoch:
            raise ValueError("Closed/stale search lifetime")
        if profile != self.profile:
            raise ValueError("Incompatible model profile")

    def publish(self, identifier, doc, vector, expected=0, fail=False):
        self.db.execute("BEGIN IMMEDIATE")
        try:
            old = self.db.execute("SELECT generation,removed FROM sources WHERE id=?", (identifier,))
            if old and (old[0][0] != expected or old[0][1]):
                raise ValueError("Stale/removed source publication")
            if not old and expected != 0:
                raise ValueError("Unknown source generation")
            self.db.execute("INSERT OR REPLACE INTO sources VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", (identifier,doc['id'],self.profile,doc['account'],int(doc['retained']),int(doc['removed']),doc['expires_at'],doc['alias'],doc['sender'],doc['kind'],doc['received_at'],expected+1))
            text = doc['text']
            self.db.execute("INSERT OR REPLACE INTO passages VALUES(?,?,?,?,?,?)", (identifier,doc['title'],text,hashlib.sha256(text.encode()).hexdigest(),0,len(text.encode())))
            self.db.execute("DELETE FROM words WHERE rowid=?",(identifier,))
            self.db.execute("INSERT INTO words(rowid,title,body) VALUES(?,?,?)",(identifier,doc['title'],text))
            if fail:
                raise RuntimeError("Injected checkpoint failure before vector publication")
            self.db.execute("DELETE FROM vectors WHERE rowid=?",(identifier,))
            if self.metadata:
                self.db.execute("INSERT INTO vectors(rowid,vector,profile,account,retained,removed,expires,alias,sender,kind,received) VALUES(?,?,?,?,?,?,?,?,?,?,?)",(identifier,vector,self.profile,doc['account'],int(doc['retained']),int(doc['removed']),doc['expires_at'],doc['alias'],doc['sender'],doc['kind'],doc['received_at']))
            else:
                self.db.execute("INSERT INTO vectors(rowid,vector) VALUES(?,?)",(identifier,vector))
            self.db.execute("COMMIT")
        except BaseException:
            self.db.execute("ROLLBACK")
            raise

    def flat(self):
        if self.engine == "vec1":
            self.db.execute("INSERT INTO vectors(cmd,arg) VALUES('rebuild','{index:" + '"flat",distance:"cos"' + "}')")

    def predicates(self, query, now):
        sql = "s.profile=? AND s.account=? AND s.retained=1 AND s.removed=0 AND s.expires>?"
        values = [self.profile,"reference",now]
        for key,value in query.get('filters',{}).items():
            if key in ('alias','sender','kind'):
                sql += ' AND s.'+key+'=?'
            elif key == 'before':
                sql += ' AND s.received<?'
            elif key == 'after':
                sql += ' AND s.received>?'
            else:
                raise ValueError('Unknown fixture filter')
            values.append(value)
        return sql,values

    def query(self, vector, query, now, epoch, profile, explain=False, limit=10):
        self.check(epoch,profile)
        assert 0 < limit <= 50
        where,values=self.predicates(query,now)
        if self.engine == 'vec1':
            candidate="SELECT s.id,v.distance FROM vectors(?,'{K:"+str(limit)+",streaming:1}') v CROSS JOIN sources s ON s.id=v.rowid WHERE "+where
            if self.metadata:
                candidate+=' AND '+where.replace('s.','v.')
                values=values+values
            # NN streaming supplies distance-ordered candidates. Stop after enough
            # eligible rows; an outer ORDER BY must sort only this bounded set.
            sql='WITH nearest AS MATERIALIZED ('+candidate+' LIMIT '+str(limit)+') SELECT id,distance FROM nearest ORDER BY distance,id'
        else:
            sql="SELECT rowid,distance FROM vectors WHERE vector MATCH ? AND k="+str(limit)+" AND rowid IN (SELECT s.id FROM sources s WHERE "+where+") ORDER BY distance"
        rows=self.db.execute(('EXPLAIN QUERY PLAN ' if explain else '')+sql,[vector,*values])
        self.check(epoch,profile)
        return rows

    def keyword(self, match, query, now, epoch):
        self.check(epoch,self.profile)
        where,values=self.predicates(query,now)
        return self.db.execute("SELECT s.id,bm25(words) FROM words JOIN sources s ON s.id=words.rowid WHERE words MATCH ? AND "+where+" ORDER BY bm25(words),s.id LIMIT 50",[match,*values])

    def passage(self, identifier, now, epoch):
        self.check(epoch,self.profile)
        where,values=self.predicates({},now)
        rows=self.db.execute("SELECT p.title,p.body,p.checksum,p.start,p.end,s.name,s.generation FROM passages p JOIN sources s ON s.id=p.id WHERE p.id=? AND "+where,[identifier,*values])
        self.check(epoch,self.profile)
        if not rows:
            raise ValueError('Source unavailable')
        title,text,checksum,start,end,name,generation=rows[0]
        selected=text.encode()[start:end]
        if hashlib.sha256(selected).hexdigest()!=checksum:
            raise ValueError('Invalid passage provenance')
        return {'name':name,'generation':generation,'title':title,'text':selected.decode(),'checksum':checksum,'profile':self.profile}

    def remove(self, identifier):
        # Durable removal and its cleanup intent belong to one source transaction.
        self.db.execute('BEGIN IMMEDIATE')
        try:
            self.db.execute('UPDATE sources SET removed=1,generation=generation+1 WHERE id=? AND removed=0',(identifier,))
            self.db.execute('INSERT OR REPLACE INTO cleanup SELECT id,generation FROM sources WHERE id=? AND removed=1',(identifier,))
            self.db.execute('COMMIT')
        except BaseException:
            self.db.execute('ROLLBACK');raise

    def clean(self,identifier,fail=False):
        self.db.execute('BEGIN IMMEDIATE')
        try:
            assert self.db.execute('SELECT id FROM cleanup WHERE id=?',(identifier,))
            self.db.execute('DELETE FROM words WHERE rowid=?',(identifier,))
            if fail:
                raise RuntimeError('Injected failure during removal cleanup')
            self.db.execute('DELETE FROM vectors WHERE rowid=?',(identifier,))
            self.db.execute('DELETE FROM passages WHERE id=?',(identifier,))
            self.db.execute('DELETE FROM cleanup WHERE id=?',(identifier,))
            self.db.execute('COMMIT')
        except BaseException:
            self.db.execute('ROLLBACK');raise

    def lock(self):
        self.epoch+=1
        self.db.close()

    def unlock(self,key):
        assert not self.db.handle
        self.db=Database(self.path,key,self.engine,False)
        self.epoch+=1
