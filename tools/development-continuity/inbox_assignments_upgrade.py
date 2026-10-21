#!/usr/bin/env python3
"""Preservation-checked fixture inbox cutover; no runtime predecessor reader."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
from urllib.parse import urlsplit

spec = importlib.util.spec_from_file_location('preservation', Path(__file__).with_name('external_receipts_upgrade.py'))
preservation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preservation)
run, query, snapshot, file_fingerprints, save = (getattr(preservation, n) for n in ('run', 'query', 'snapshot', 'file_fingerprints', 'save'))

def digest(value):
    return hashlib.sha256(value).hexdigest()

def backend(args):
    config = json.loads(args.source_config.read_text())
    source, destination = config['database_url'], args.destination_url
    assert source != destination
    for url in (source, destination):
        assert urlsplit(url).hostname in ('localhost', '127.0.0.1', '::1')
    assert urlsplit(destination).path.startswith('/cs_mail_required_inboxes_')
    assert query(source, 'SELECT max(version) FROM cs_schema_migrations').strip() == '42'
    assert query(source, 'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() == '0', 'Stop the source service first'
    before, keys = snapshot(source), file_fingerprints(config)
    prior = args.directory/'source-before.json'
    assigned_at = json.loads(prior.read_text())['assigned_at'] if prior.exists() else time.time_ns()//1_000_000
    prepared = {'tables': before, 'keys': keys, 'assigned_at': assigned_at}
    fresh = query(destination, "SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip() == '0'
    if args.directory.exists():
        assert json.loads((args.directory/'source-before.json').read_text()) == prepared
    else:
        assert fresh
        args.directory.mkdir(parents=True, mode=0o700)
        save(args.directory/'source-before.json', prepared)
    backup = args.directory/'source-backup.dump'
    if not backup.exists():
        run('pg_dump', source, '--format=custom', '--file='+str(backup))
    if fresh:
        run('pg_restore', '--exit-on-error', '--single-transaction', '--no-owner', '--dbname='+destination, str(backup))
    plan = args.directory/'assignment-plan.json'
    if query(destination, 'SELECT max(version) FROM cs_schema_migrations').strip() == '42':
        run(str(args.tool.resolve()), 'backend', destination, str(plan.resolve()), 'rollback', str(assigned_at))
        assert snapshot(destination) == before, 'Rollback changed predecessor data'
        run(str(args.tool.resolve()), 'backend', destination, str(plan.resolve()), 'apply', str(assigned_at))
    after = snapshot(destination)
    allowed = {'cs_correspondence_messages', 'cs_conversations', 'cs_alias_routes', 'cs_alias_deliveries'}
    assert set(after)-set(before) == {'cs_receiving_cutover_evidence'}
    assert set(before)-set(after) == {'cs_alias_deliveries'}
    assert all(after[name] == before[name] for name in before if name not in allowed)
    for name in allowed:
        original = query(source, f'SELECT to_jsonb(t)::text FROM {name} t ORDER BY to_jsonb(t)::text').splitlines()
        archived = query(destination, f"SELECT record::text FROM cs_receiving_cutover_evidence WHERE source_table='{name}' ORDER BY record::text").splitlines()
        assert original == archived
    # Schema-generated columns may change row shape; original facts must not change.
    for name in ('cs_alias_routes', 'cs_conversations', 'cs_correspondence_messages'):
        old_rows = {row['id']: row for row in map(json.loads, query(source, f'SELECT to_jsonb(t)::text FROM {name} t').splitlines())}
        new_rows = {row['id']: row for row in map(json.loads, query(destination, f'SELECT to_jsonb(t)::text FROM {name} t').splitlines())}
        for identifier, original in old_rows.items():
            current = dict(new_rows[identifier])
            for column in ('participant_first', 'participant_second', 'receiving_alias', 'receiving_owner'):
                current.pop(column, None)
            if name == 'cs_correspondence_messages':
                current['record'] = dict(current['record'])
                assignment = current['record'].pop('receiving')
                assert 'DevelopmentAssignment' in assignment['provenance']
            assert current == original, 'An original message, conversation or alias fact changed'
    run(str(args.tool.resolve()), 'backend', destination, str(plan.resolve()), 'verify', str(assigned_at))
    assert snapshot(destination) == after
    assert snapshot(source) == before and file_fingerprints(config) == keys
    config['database_url'] = destination
    if not (args.directory/'maild.json').exists():
        save(args.directory/'maild.json', config)
    if not (args.directory/'verified.json').exists():
        save(args.directory/'verified.json', {'source_unchanged':True, 'original_evidence_archived':True, 'keys_unchanged':True, 'rollback_verified':True, 'exact_retry_verified':True, 'after':after})
    print('PASS explicit public-inbox assignment, untouched source/account/keys/history, rollback and exact retry')

def rows(db):
    return {table: db.execute('SELECT * FROM '+table+' ORDER BY id').fetchall() for table in ('device_cache','retained_metadata','retained_progress','retained_index','retained_copies','member_workspace','personal_mailbox')}

def verify_device(db, plan):
    assert db.execute('SELECT ciphertext FROM device_cache WHERE id=1').fetchone()[0] == bytes(plan['cache'])
    assert db.execute('SELECT ciphertext FROM retained_metadata WHERE id=1').fetchone()[0] == bytes(plan['metadata'])
    assert db.execute('SELECT revision FROM retained_progress WHERE id=1').fetchone()[0] == plan['revision']+1
    assert dict(db.execute('SELECT id,ciphertext FROM retained_copies')) == {bytes(row['id']):bytes(row['ciphertext']) for row in plan['copies']}

def apply_device(db, plan):
    assert digest(db.execute('SELECT ciphertext FROM device_cache WHERE id=1').fetchone()[0]) == plan['original_cache']
    assert digest(db.execute('SELECT ciphertext FROM retained_metadata WHERE id=1').fetchone()[0]) == plan['original_metadata']
    assert db.execute('SELECT revision FROM retained_progress WHERE id=1').fetchone()[0] == plan['revision']
    assert db.execute('SELECT count(*) FROM retained_copies').fetchone()[0] == len(plan['copies'])
    for row in plan['copies']:
        identifier = bytes(row['id'])
        assert digest(db.execute('SELECT ciphertext FROM retained_copies WHERE id=?',(identifier,)).fetchone()[0]) == row['before']
        assert db.execute('UPDATE retained_copies SET ciphertext=? WHERE id=?',(bytes(row['ciphertext']),identifier)).rowcount == 1
    db.execute('UPDATE device_cache SET ciphertext=? WHERE id=1',(bytes(plan['cache']),))
    db.execute('UPDATE retained_metadata SET ciphertext=? WHERE id=1',(bytes(plan['metadata']),))
    assert db.execute('UPDATE retained_progress SET revision=? WHERE id=1 AND revision=?',(plan['revision']+1,plan['revision'])).rowcount == 1
    verify_device(db,plan)

def device(args):
    args.directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    original = args.directory/'device-before.sqlite'
    encrypted = args.directory/'encrypted-device-plan.json'
    with (args.device_store.parent/'client.lock').open('r+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX|fcntl.LOCK_NB)
        with sqlite3.connect(args.device_store) as db:
            if not original.exists():
                with sqlite3.connect(original) as saved:
                    db.backup(saved)
            with sqlite3.connect('file:'+str(original.resolve())+'?mode=ro',uri=True) as saved:
                before = rows(saved)
            if not encrypted.exists():
                assert rows(db) == before
                subprocess.run([str(args.helper.resolve()),str(args.tool.resolve()),str(args.backend_plan.resolve()),str(encrypted.resolve()),str(args.device_store.resolve())],check=True)
            plan = json.loads(encrypted.read_text())
            if digest(db.execute('SELECT ciphertext FROM device_cache WHERE id=1').fetchone()[0]) == plan['original_cache']:
                assert rows(db) == before
                db.execute('BEGIN IMMEDIATE')
                apply_device(db,plan)
                db.rollback()
                assert rows(db) == before
                db.execute('BEGIN IMMEDIATE')
                apply_device(db,plan)
                db.commit()
            verify_device(db,plan)
            after = rows(db)
            for table in ('retained_index','member_workspace','personal_mailbox'):
                assert after[table] == before[table]
            verify_device(db,plan)
            result = args.directory/'device-verified.json'
            if not result.exists():
                save(result, {'account':plan['account'],'owner':plan['owner'],'copies':len(plan['copies']),'original_credential_not_written':True,'profile_unchanged':True,'writing_personal_index_unchanged':True,'rollback_verified':True,'exact_retry_verified':True})
            print('PASS original account/credential and authenticated content preserved; device rollback and exact retry')

def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode',required=True)
    back = sub.add_parser('backend')
    back.add_argument('--source-config',type=Path,required=True)
    back.add_argument('--destination-url',required=True)
    dev = sub.add_parser('device')
    dev.add_argument('--device-store',type=Path,required=True)
    dev.add_argument('--backend-plan',type=Path,required=True)
    dev.add_argument('--helper',type=Path,required=True)
    for command in (back,dev):
        command.add_argument('--directory',type=Path,required=True)
        command.add_argument('--tool',type=Path,required=True)
    args = parser.parse_args()
    (backend if args.mode=='backend' else device)(args)

if __name__=='__main__':
    main()
