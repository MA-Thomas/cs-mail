"""Explicit preservation-checked v2→v3 derived-index cutover. No account/copy/Keychain writes."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
def sha(data):return hashlib.sha256(data).hexdigest()
def unchanged(db):
    return {table:db.execute('SELECT * FROM '+table+' ORDER BY id').fetchall() for table in ('device_cache','member_workspace','personal_mailbox','retained_metadata','retained_copies')}
def rows(db):return dict(db.execute('SELECT id,ciphertext FROM retained_index'))
def main():
    p=argparse.ArgumentParser();p.add_argument('--device-store',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--tool',type=Path,required=True);p.add_argument('--helper',type=Path,required=True);a=p.parse_args()
    os.umask(0o077);a.directory.mkdir(parents=True,exist_ok=True,mode=0o700);plan_file=a.directory/'encrypted-plan.json'
    with (a.device_store.parent/'client.lock').open('r+') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with sqlite3.connect(a.device_store) as db:
            before=unchanged(db)
            backup=a.directory/'device-before.sqlite'
            if not backup.exists():
                with sqlite3.connect(backup) as saved:db.backup(saved)
            if not plan_file.exists():subprocess.run([str(a.helper.resolve()),str(a.tool.resolve()),str(plan_file.resolve()),str(a.device_store.resolve())],check=True)
            plan=json.loads(plan_file.read_text());assert sha(before['device_cache'][0][1])==plan['cache_sha256']
            after={bytes(r['id']):bytes(r['index']) for r in plan['records']};current=rows(db)
            if current!=after:
                assert {key:sha(value) for key,value in current.items()}=={bytes(r['id']):r['before_sha256'] for r in plan['records']}
                assert db.execute('SELECT revision FROM retained_progress WHERE id=1').fetchone()[0]==plan['expected_revision']
                def apply():
                    for key,value in after.items():db.execute('UPDATE retained_index SET ciphertext=? WHERE id=?',(value,key))
                    db.execute('UPDATE retained_progress SET revision=? WHERE id=1',(plan['expected_revision']+1,))
                db.execute('BEGIN IMMEDIATE');apply();assert unchanged(db)==before;db.rollback();assert rows(db)==current
                db.execute('BEGIN IMMEDIATE');apply();assert rows(db)==after and unchanged(db)==before;db.commit()
            assert rows(db)==after and unchanged(db)==before
            with sqlite3.connect('file:'+str(backup.resolve())+'?mode=ro',uri=True) as saved:assert unchanged(saved)==before
            result={'copies_preserved':plan['copies'],'index_records':len(after),'account_workspace_personal_metadata_copies_unchanged':True,'credential_sha256':plan['credential_sha256'],'credential_not_written':True,'rollback_passed':True,'retry_guard_implemented':True,'exact_retry_exercised':False,'backup_removal_condition':'remove after successful installed-app continuity and retained-search verification'}
            (a.directory/'verified.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PASS explicit derived-index cutover; original copies, account, writing and credential preserved; rollback verified; exact retry guarded but not exercised')
if __name__=='__main__':main()
