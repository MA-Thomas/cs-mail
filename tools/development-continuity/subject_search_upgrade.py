#!/usr/bin/env python3
"""Preserve schema-41 source; rehearse and verify the explicit subject/document conversion."""
import argparse
import hashlib
import importlib.util
import json
import os
import sqlite3
from pathlib import Path
from urllib.parse import urlsplit

spec = importlib.util.spec_from_file_location('preservation', Path(__file__).with_name('external_receipts_upgrade.py'))
preservation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preservation)
run, query, snapshot, file_fingerprints, save = (getattr(preservation, name) for name in ('run', 'query', 'snapshot', 'file_fingerprints', 'save'))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-config', type=Path, required=True)
    parser.add_argument('--destination-url', required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--tool', type=Path, required=True)
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--device-store', type=Path, required=True)
    parser.add_argument('--fixture-root', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.source_config.read_text())
    source, destination = config['database_url'], args.destination_url
    assert source != destination
    for url in (source, destination):
        assert urlsplit(url).hostname in ('localhost', '127.0.0.1', '::1')
    assert query(source, 'SELECT max(version) FROM cs_schema_migrations').strip() == '41'
    assert query(source, 'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() == '0', 'Stop the source service first'
    before, keys = snapshot(source), file_fingerprints(config)
    prepared = {'tables': before, 'keys': keys}
    fresh = query(destination, "SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip() == '0'
    if args.directory.exists():
        assert json.loads((args.directory / 'source-before.json').read_text()) == prepared
    else:
        assert fresh, 'A new empty destination is required'
        args.directory.mkdir(parents=True, mode=0o700)
        save(args.directory / 'source-before.json', prepared)
    backup = args.directory / 'source-backup.dump'
    if not backup.exists(): run('pg_dump', source, '--format=custom', '--file=' + str(backup))
    if fresh: run('pg_restore', '--exit-on-error', '--single-transaction', '--no-owner', '--dbname=' + destination, str(backup))
    original = args.directory / 'device-before.sqlite'
    with sqlite3.connect('file:' + str(args.device_store.resolve()) + '?mode=ro', uri=True) as device:
        if not original.exists():
            with sqlite3.connect(original) as saved: device.backup(saved)
        cache = device.execute('SELECT ciphertext FROM device_cache WHERE id=1').fetchone()[0]
        original_cache = hashlib.sha256(cache).hexdigest()
    os.environ['CS_MAIL_SUBJECT_DEVICE_STORE'] = str(args.device_store.resolve())
    tool_args = (str(args.tool.resolve()), destination, str(args.fixture_root.resolve()), str((args.directory / 'device-ready').resolve()))
    custody = config['members']['custody_secret_file']
    converted = query(destination, 'SELECT max(version) FROM cs_schema_migrations').strip() == '42'
    if not converted:
        # One helper session shares its in-memory input across rollback and conversion.
        run(str(args.helper.resolve()), *tool_args, 'batch', custody)
    after = snapshot(destination)
    changed = {'cs_conversations', 'cs_correspondence_messages', 'cs_correspondence_copies', 'cs_correspondence_recovery', 'cs_correspondence_receipts'}
    assert set(after) - set(before) == {'cs_subject_cutover_evidence'}
    assert all(after[name] == before[name] for name in before if name not in changed), 'Unrelated account, alias, permission or financial records changed'
    for table in changed:
        old = query(source, f'SELECT to_jsonb(t)::text FROM {table} t ORDER BY to_jsonb(t)::text').splitlines()
        archived = query(destination, f"SELECT record::text FROM cs_subject_cutover_evidence WHERE source_table='{table}' ORDER BY record::text").splitlines()
        assert old == archived, 'Original evidence was not archived verbatim'
    run(*tool_args, 'convert', custody)
    assert snapshot(destination) == after, 'Exact retry changed the converted database'
    assert snapshot(source) == before and file_fingerprints(config) == keys, 'Source or configured keys changed'
    assert json.loads((args.directory / 'device-ready' / 'converted.json').read_text())['original_cache_sha256'] == original_cache
    config['database_url'] = destination
    if not (args.directory / 'maild.json').exists(): save(args.directory / 'maild.json', config)
    if not (args.directory / 'verified.json').exists(): save(args.directory / 'verified.json', {'source_unchanged': True, 'key_files_unchanged': True, 'rollback_passed': True, 'exact_retry_passed': True, 'original_evidence_archived': True, 'unrelated_tables_and_sequences': len(before)-len(changed), 'after': after})
    print('PASS preserved source, account, keys, addresses and financial facts; full rollback, exact retry and original-evidence archive verified')

if __name__ == '__main__': main()
