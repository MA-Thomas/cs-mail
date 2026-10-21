#!/usr/bin/env python3
"""Explicit offline conversion of retained search records. Never writes Keychain or account state."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess


def digest(value):
    return hashlib.sha256(value).hexdigest()


def unchanged(db):
    return {table: db.execute('SELECT * FROM '+table+' ORDER BY id').fetchall()
            for table in ('device_cache','member_workspace','personal_mailbox')}


def apply(db,plan):
    db.execute('CREATE TABLE retained_progress(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL CHECK(revision>0))')
    db.execute('CREATE TABLE retained_metadata(id INTEGER PRIMARY KEY CHECK(id=1),ciphertext BLOB NOT NULL)')
    for name in ('retained_index','retained_copies'):
        db.execute('CREATE TABLE '+name+'(id BLOB PRIMARY KEY CHECK(length(id)=32),ciphertext BLOB NOT NULL) WITHOUT ROWID')
    db.execute('INSERT INTO retained_progress VALUES(1,1)')
    db.execute('INSERT INTO retained_metadata VALUES(1,?)',(bytes(plan['metadata']),))
    for row in plan['records']:
        identifier=bytes.fromhex(row['id'])
        assert len(identifier)==32
        db.execute('INSERT INTO retained_index VALUES(?,?)',(identifier,bytes(row['index'])))
        if row['copy'] is not None:
            db.execute('INSERT INTO retained_copies VALUES(?,?)',(identifier,bytes(row['copy'])))
    assert db.execute('SELECT count(*) FROM retained_copies').fetchone()[0]==plan['copies']
    assert db.execute('SELECT count(*) FROM retained_index').fetchone()[0]==plan['copies']+plan['exclusions']
    db.execute('DROP TABLE retained_mail')


def verify(db,plan):
    assert db.execute("SELECT count(*) FROM sqlite_master WHERE name='retained_mail'").fetchone()[0]==0
    assert db.execute('SELECT revision FROM retained_progress WHERE id=1').fetchone()[0]==1
    assert db.execute('SELECT ciphertext FROM retained_metadata WHERE id=1').fetchone()[0]==bytes(plan['metadata'])
    index=dict(db.execute('SELECT id,ciphertext FROM retained_index'))
    copies=dict(db.execute('SELECT id,ciphertext FROM retained_copies'))
    assert index=={bytes.fromhex(row['id']):bytes(row['index']) for row in plan['records']}
    assert copies=={bytes.fromhex(row['id']):bytes(row['copy']) for row in plan['records'] if row['copy'] is not None}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--device-store',type=Path,required=True)
    parser.add_argument('--directory',type=Path,required=True)
    parser.add_argument('--helper',type=Path,required=True)
    parser.add_argument('--tool',type=Path,required=True)
    args=parser.parse_args()
    os.umask(0o077)
    args.directory.mkdir(mode=0o700,parents=True,exist_ok=True)
    backup=args.directory/'device-before.sqlite'
    plan_path=args.directory/'encrypted-record-plan.json'
    with (args.device_store.parent/'client.lock').open('r+') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with sqlite3.connect(args.device_store) as db:
            before=unchanged(db)
            if not backup.exists():
                with sqlite3.connect(backup) as saved: db.backup(saved)
            if not plan_path.exists():
                # The helper reads the original credential exactly once. All subsequent work uses ciphertext.
                subprocess.run([str(args.helper.resolve()),str(args.tool.resolve()),str(plan_path.resolve()),str(args.device_store.resolve())],check=True)
            plan=json.loads(plan_path.read_text())
            assert digest(before['device_cache'][0][1])==plan['cache_sha256']
            with sqlite3.connect('file:'+str(backup.resolve())+'?mode=ro',uri=True) as saved:
                assert unchanged(saved)==before
                original=saved.execute('SELECT ciphertext FROM retained_mail WHERE id=1').fetchone()[0]
                assert digest(original)==plan['source_sha256']
            if db.execute("SELECT count(*) FROM sqlite_master WHERE name='retained_mail'").fetchone()[0]:
                assert digest(db.execute('SELECT ciphertext FROM retained_mail WHERE id=1').fetchone()[0])==plan['source_sha256']
                db.execute('BEGIN IMMEDIATE')
                apply(db,plan)
                assert unchanged(db)==before
                db.rollback()
                assert digest(db.execute('SELECT ciphertext FROM retained_mail WHERE id=1').fetchone()[0])==plan['source_sha256']
                assert db.execute("SELECT count(*) FROM sqlite_master WHERE name IN ('retained_progress','retained_metadata','retained_index','retained_copies')").fetchone()[0]==0
                db.execute('BEGIN IMMEDIATE')
                apply(db,plan)
                verify(db,plan)
                assert unchanged(db)==before
                db.commit()
            verify(db,plan)
            assert unchanged(db)==before
            # Exact retry is a read-only verification, requiring no credential or writes.
            verify(db,plan)
            result={'account':plan['account'],'persona':plan['persona'],'copies':plan['copies'],'exclusions':plan['exclusions'],'account_cache_workspace_personal_unchanged':True,'rollback_verified':True,'exact_retry_verified':True,'original_credential_not_written':True}
            (args.directory/'verified.json').write_text(json.dumps(result,indent=2))
            print('PASS independent encrypted records, rollback and exact retry; original account, writing and personal state unchanged')


if __name__=='__main__': main()
