#!/usr/bin/env python3
"""Offline schema-40 to 41 preservation copy. Never accesses Keychain or writes the source."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
from urllib.parse import urlsplit

spec = importlib.util.spec_from_file_location('preservation', Path(__file__).with_name('external_receipts_upgrade.py'))
preservation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preservation)
run, query, snapshot, file_fingerprints, save = (getattr(preservation, n) for n in ('run','query','snapshot','file_fingerprints','save'))

def rows(database, table):
    return [json.loads(v) for v in query(database, f'SELECT row_to_json(t) FROM {table} t ORDER BY row_to_json(t)::text').splitlines()]

def canonical(values):
    return sorted(json.dumps(v, sort_keys=True, separators=(',',':')) for v in values)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-config',type=Path,required=True)
    parser.add_argument('--destination-url',required=True)
    parser.add_argument('--directory',type=Path,required=True)
    parser.add_argument('--validator',type=Path,required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.source_config.read_text())
    source, destination = config['database_url'], args.destination_url
    for url in (source,destination):
        value = urlsplit(url)
        if value.scheme not in ('postgresql','postgres') or value.hostname not in ('localhost','127.0.0.1','::1'):
            raise RuntimeError('Explicit local databases required')
    if urlsplit(source).path == urlsplit(destination).path:
        raise RuntimeError('A distinct preservation copy is required')
    if query(source,'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() != '0':
        raise RuntimeError('Stop the source service before the preservation copy')
    if query(source,'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1,41)]:
        raise RuntimeError('Source must be exactly schema 40')
    before, keys = snapshot(source), file_fingerprints(config)
    original = {name: rows(source,name) for name in ('cs_alias_routes','cs_alias_commands','cs_alias_deliveries')}
    migration = Path(__file__).resolve().parents[2]/'crates/cs-mail-storage-postgres/migrations/0041_alias_permission.sql'
    prepared = {'tables': before,'keys': keys,'migration':hashlib.sha256(migration.read_bytes()).hexdigest()}
    fresh = query(destination,"SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip() == '0'
    if args.directory.exists():
        if json.loads((args.directory/'source-before.json').read_text()) != prepared: raise RuntimeError('Source or migration changed')
    else:
        if not fresh: raise RuntimeError('Destination must be empty; no reset is permitted')
        args.directory.mkdir(parents=True,mode=0o700)
        save(args.directory/'source-before.json',prepared)
    backup = args.directory/'source-backup.dump'
    if not backup.exists(): run('pg_dump',source,'--format=custom','--file='+str(backup))
    sql = "SET LOCAL cs_mail.alias_permission_preservation='on';\n"+migration.read_text()
    wrapper = args.directory/'preservation.sql'
    if wrapper.exists() and wrapper.read_text() != sql: raise RuntimeError('Preservation SQL changed')
    wrapper.write_text(sql)
    if fresh:
        run('pg_restore','--exit-on-error','--single-transaction','--no-owner','--dbname='+destination,str(backup))
        # Challenge complete rollback after all migration writes, then apply exactly once.
        run('psql',destination,'-X','-v','ON_ERROR_STOP=1','-c','BEGIN','-f',str(wrapper),'-c','ROLLBACK')
        if snapshot(destination) != before: raise RuntimeError('Rollback left partial migration effects')
        run('psql',destination,'-X','--single-transaction','-v','ON_ERROR_STOP=1','-f',str(wrapper))
    copied = snapshot(destination)
    changed = {'cs_alias_routes','cs_alias_commands','cs_alias_deliveries'}
    if set(copied)-set(before) != {'cs_alias_permission_cutover_evidence'}: raise RuntimeError('Unexpected table set')
    if any(copied[n] != before[n] for n in before if n not in changed): raise RuntimeError('Unrelated rows changed')
    archive = rows(destination,'cs_alias_permission_cutover_evidence')
    for name, originals in original.items():
        if canonical([r['record'] for r in archive if r['source_table']==name]) != canonical(originals): raise RuntimeError('Original evidence was not preserved')
    for old,new in zip(sorted(original['cs_alias_routes'],key=lambda r:r['id']),sorted(rows(destination,'cs_alias_routes'),key=lambda r:r['id'])):
        expected = json.loads(json.dumps(old))
        record = expected['record']; lane = record.pop('lane')
        record['permission'] = {'alias':record['id'],'owner':record['owner'],'revision':1 if lane else 0,'changed_at':record['created_at'],'state':{'Broad':lane} if lane else 'None'}
        if expected != new: raise RuntimeError('Address or permission preservation mismatch')
    expected_deliveries = json.loads(json.dumps(original['cs_alias_deliveries']))
    for row in expected_deliveries:
        if row['evidence']['basis'] == 'AliasLane':
            row['evidence']['basis'] = {'AliasLane': {'permission_revision': 1}}
    if canonical(expected_deliveries) != canonical(rows(destination,'cs_alias_deliveries')): raise RuntimeError('Receiving history changed beyond its explicit format conversion')
    if rows(destination,'cs_alias_commands'): raise RuntimeError('New-format replay table was not empty')
    run(str(args.validator.resolve()),destination)
    if snapshot(source) != before or file_fingerprints(config) != keys: raise RuntimeError('Source or keys changed')
    if query(source,'SELECT row_to_json(t) FROM cs_schema_migrations t ORDER BY version') != query(destination,'SELECT row_to_json(t) FROM cs_schema_migrations t WHERE version<=40 ORDER BY version'): raise RuntimeError('Migration history changed')
    config['database_url'] = destination
    output = args.directory/'maild.json'
    if output.exists() and json.loads(output.read_text()) != config: raise RuntimeError('Destination configuration changed')
    if not output.exists(): save(output,config)
    verified = {'source_schema':40,'destination_schema':41,'source_unchanged':True,'key_files_unchanged':True,'rollback_passed':True,'original_evidence_archived':True,'preserved_unrelated_tables_and_sequences':len(before)-len(changed),'tables':copied}
    output=args.directory/'verified.json'
    if output.exists() and json.loads(output.read_text()) != verified: raise RuntimeError('Verification result changed')
    if not output.exists(): save(output,verified)
    print('PASS offline preservation, rollback and repeat verification; original accounts, addresses, messages, keys and signed evidence retained')

if __name__ == '__main__':
    main()
