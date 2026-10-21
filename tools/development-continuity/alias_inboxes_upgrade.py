#!/usr/bin/env python3
"""Offline preservation-checked schema 39 to 40 copy for the original owner.

No credential access or account conversion. Apply the receiving-route schema cutover only
in a fresh local copy; retain the untouched source as the rollback checkpoint.
"""
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
run, query, snapshot, file_fingerprints, save = (getattr(preservation, n) for n in ('run', 'query', 'snapshot', 'file_fingerprints', 'save'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-config', type=Path, required=True)
    parser.add_argument('--destination-url', required=True)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.source_config.read_text())
    source, destination = config['database_url'], args.destination_url
    for url in (source, destination):
        parsed = urlsplit(url)
        if parsed.scheme not in ('postgres', 'postgresql') or parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
            raise RuntimeError('Only explicit local databases are permitted')
    if urlsplit(source).path == urlsplit(destination).path:
        raise RuntimeError('A distinct fresh preservation copy is required')
    if query(source, 'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() != '0':
        raise RuntimeError('Stop the original service before taking the preservation snapshot')
    if query(source, 'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1, 40)]:
        raise RuntimeError('The source must be exactly schema 39')
    if query(source, 'SELECT count(*) FROM cs_alias_deliveries').strip() != '0':
        raise RuntimeError('Historical experimental route evidence needs a separately reviewed cutover')
    before, keys = snapshot(source), file_fingerprints(config)
    migration = Path(__file__).resolve().parents[2] / 'crates/cs-mail-storage-postgres/migrations/0040_alias_inboxes.sql'
    prepared = {'tables': before, 'keys': keys, 'migration': hashlib.sha256(migration.read_bytes()).hexdigest()}
    fresh = query(destination, "SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip() == '0'
    if args.directory.exists():
        if json.loads((args.directory / 'source-before.json').read_text()) != prepared:
            raise RuntimeError('Prepared source, keys or migration changed; preserve all artifacts')
    else:
        if not fresh:
            raise RuntimeError('Destination must be empty; no reset is permitted')
        args.directory.mkdir(parents=True, mode=0o700)
        save(args.directory / 'source-before.json', prepared)
    config['database_url'] = destination
    target_config = args.directory / 'maild.json'
    if target_config.exists():
        if json.loads(target_config.read_text()) != config:
            raise RuntimeError('The prepared configuration has changed')
    else:
        save(target_config, config)
    backup = args.directory / 'source-backup.dump'
    if not backup.exists():
        run('pg_dump', source, '--format=custom', '--file=' + str(backup))
    if fresh:
        run('pg_restore', '--exit-on-error', '--single-transaction', '--no-owner', '--dbname=' + destination, str(backup))
        run('psql', destination, '-X', '--single-transaction', '-v', 'ON_ERROR_STOP=1', '--file=' + str(migration))
    copied = snapshot(destination)
    if set(copied) != set(before):
        raise RuntimeError('Unexpected table set after the creation-format cutover')
    if {k: copied[k] for k in before} != before or snapshot(source) != before or file_fingerprints(config) != keys:
        raise RuntimeError('Preservation failed; retain both databases and all artifacts')
    if query(destination, 'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1, 41)]:
        raise RuntimeError('The destination must be exactly schema 40')
    old_migrations = query(source, 'SELECT row_to_json(t)::text FROM cs_schema_migrations t ORDER BY version')
    if old_migrations != query(destination, 'SELECT row_to_json(t)::text FROM cs_schema_migrations t WHERE version<40 ORDER BY version'):
        raise RuntimeError('Original migration history was changed')
    verified = {'source_schema': 39, 'destination_schema': 40, 'all_canonical_rows_unchanged': True,
                'source_unchanged': True, 'key_files_unchanged': True, 'tables': before}
    result = args.directory / 'verified.json'
    if result.exists():
        if json.loads(result.read_text()) != verified:
            raise RuntimeError('The existing verification result changed')
    else:
        save(result, verified)
    print(f'PASS: {len(before)} original table/sequence fingerprints and key files preserved; receiving-route constraints installed in a fresh copy.')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        import sys
        detail = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        print('Preservation did not complete: ' + detail, file=sys.stderr)
        sys.exit(1)
