#!/usr/bin/env python3
"""Offline, preservation-checked copy for external receipt schema 37.

The source deployment and original credentials are never modified. The destination
must be a fresh local database. Runtime code has no old-format fallback reader.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit


def run(*args):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def query(database, sql):
    return run('psql', database, '-XAt', '-v', 'ON_ERROR_STOP=1', '-c', sql).decode()


def snapshot(database):
    names = query(database, "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename <> 'cs_schema_migrations' ORDER BY tablename").splitlines()
    result = {}
    for name in names:
        identifier = '"' + name.replace('"', '""') + '"'
        rows = query(database, f'SELECT row_to_json(t)::text FROM public.{identifier} t ORDER BY row_to_json(t)::text')
        result[name] = {'rows': len(rows.splitlines()), 'sha256': hashlib.sha256(rows.encode()).hexdigest()}
    for name in query(database, "SELECT sequencename FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename").splitlines():
        identifier = '"' + name.replace('"', '""') + '"'
        value = query(database, f'SELECT last_value,is_called FROM public.{identifier}')
        result['@sequence:' + name] = {'sha256': hashlib.sha256(value.encode()).hexdigest()}
    return result


def file_fingerprints(config):
    result = {}
    def visit(value):
        if isinstance(value, dict):
            for name, item in value.items():
                if name.endswith('_file') and isinstance(item, str):
                    path = Path(item)
                    result[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
                else:
                    visit(item)
        elif isinstance(value, list):
            for item in value:
                visit(item)
    visit(config)
    return result


def save(path, value):
    with path.open('x') as out:
        json.dump(value, out, sort_keys=True, indent=2)
    path.chmod(0o600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-config', type=Path, required=True)
    parser.add_argument('--destination-url', required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--server-bin', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.source_config.read_text())
    source = config['database_url']
    destination = args.destination_url
    for url in (source, destination):
        parsed = urlsplit(url)
        if parsed.scheme not in ('postgres', 'postgresql') or parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
            raise RuntimeError('Only explicit local PostgreSQL databases are allowed')
    if urlsplit(source).path == urlsplit(destination).path:
        raise RuntimeError('Source and destination must be distinct databases')
    if query(source, "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid <> pg_backend_pid()").strip() != '0':
        raise RuntimeError('Source is not offline; stop its service and database sessions first')
    if query(source, 'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1, 37)]:
        raise RuntimeError('Source must have exactly schema versions 1 through 36')
    before = snapshot(source)
    keys = file_fingerprints(config)
    fresh = query(destination, "SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip() == '0'
    prepared = args.directory / 'source-before.json'
    if args.directory.exists():
        recorded = json.loads(prepared.read_text())
        if recorded != {'tables': before, 'keys': keys}:
            raise RuntimeError('Prepared source or keys changed; preserve the earlier artifacts')
    else:
        if not fresh:
            raise RuntimeError('Destination is not a fresh empty database; no reset is permitted')
        args.directory.mkdir(parents=True, exist_ok=False, mode=0o700)
        save(prepared, {'tables': before, 'keys': keys})
    if not (args.directory / 'source-backup.dump').exists():
        run('pg_dump', source, '--format=custom', '--file=' + str(args.directory / 'source-backup.dump'))
    if not (args.directory / 'preserved-data.dump').exists():
        run('pg_dump', source, '--format=custom', '--data-only', '--exclude-table=public.cs_schema_migrations', '--file=' + str(args.directory / 'preserved-data.dump'))
    config['database_url'] = destination
    target_config = args.directory / 'maild.json'
    if target_config.exists():
        if json.loads(target_config.read_text()) != config:
            raise RuntimeError('Prepared configuration changed; do not substitute another destination')
    else:
        save(target_config, config)
    if not fresh:
        # Recover a lost final acknowledgement only for an exact, fully restored copy.
        if snapshot(destination) != before or query(destination, 'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1, 38)]:
            raise RuntimeError('Destination is incomplete or changed; preserve it and use another fresh database')
        verify(args.directory, source, destination, before, keys, config)
        return
    run(str(args.server_bin.resolve()), 'migrate', '--config', str(target_config.resolve()))
    # Retained offline history is deliberately outside runtime migrations; preserve it too.
    target_tables = query(destination, "SELECT tablename FROM pg_tables WHERE schemaname='public'").splitlines()
    source_tables = [name for name in before if not name.startswith('@sequence:')]
    extras = set(source_tables) - set(target_tables)
    if extras - {'cs_development_continuity_archive'}:
        raise RuntimeError('Unrecognized extra source tables; preserve the snapshot and review their schema')
    for name in sorted(extras):
        run('pg_restore', '--exit-on-error', '--no-owner', '--schema-only', '--table=' + name,
            '--dbname=' + destination, str(args.directory / 'source-backup.dump'))
    # Only fresh migration seed rows exist here. Replace them with the exact source facts.
    names = query(destination, "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename <> 'cs_schema_migrations' ORDER BY tablename").splitlines()
    identifiers = ','.join('"' + name.replace('"', '""') + '"' for name in names)
    query(destination, 'TRUNCATE ' + identifiers)
    run('pg_restore', '--exit-on-error', '--single-transaction', '--no-owner', '--data-only',
        '--dbname=' + destination, str(args.directory / 'preserved-data.dump'))
    verify(args.directory, source, destination, before, keys, config)


def verify(directory, source, destination, before, keys, config):
    copied = snapshot(destination)
    unchanged = snapshot(source)
    if before != copied or before != unchanged or keys != file_fingerprints(config):
        raise RuntimeError('Preservation failed; keep both databases and all artifacts; do not start the destination')
    if query(destination, 'SELECT version FROM cs_schema_migrations ORDER BY version').splitlines() != [str(v) for v in range(1, 38)]:
        raise RuntimeError('Destination did not establish the exact current schema')
    record = {'all_canonical_rows_unchanged': True, 'source_unchanged': True,
        'key_files_unchanged': True, 'source_schema': 36, 'destination_schema': 37, 'tables': copied}
    verified = directory / 'verified.json'
    if verified.exists():
        if json.loads(verified.read_text()) != record:
            raise RuntimeError('Existing preservation result changed')
    else:
        save(verified, record)
    print(f'PASS preservation: {len(copied)} table/sequence fingerprints, original source and configured keys unchanged; fresh schema 37 copy ready.')


if __name__ == '__main__':
    try:
        main()
    except (subprocess.CalledProcessError, OSError, RuntimeError) as error:
        # Never emit provider configuration, PostgreSQL URLs or raw database output.
        print('Upgrade refused or incomplete. Preserve its databases and artifacts; no success is assumed.')
        if isinstance(error, RuntimeError):
            print(str(error))
        raise SystemExit(1)
