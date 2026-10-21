#!/usr/bin/env python3
"""Prepare a fresh owner-preserving local copy for the requested alias scenario.

Run only after stopping the current owner app and server. No Keychain/cache access.
The old deployment is retained intact. This is not an address-renaming product API.
"""
import argparse
import json
import os
from pathlib import Path
import external_receipts_upgrade as preservation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    os.umask(0o077)
    root = Path(__file__).resolve().parents[2]
    config = json.loads((root / 'target/alias-inboxes-owner-20261016/owner-ready/maild.json').read_text())
    source = config['database_url']
    assert source == 'postgresql://localhost/cs_mail_alias_inboxes_owner_20261016'
    name = 'cs_mail_alias_assessment_' + ('check' if args.check else 'owner') + '_20261016'
    destination = 'postgresql://localhost/' + name
    q, run, snapshot = preservation.query, preservation.run, preservation.snapshot
    assert q(source, 'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() == '0', 'Stop the source service first'
    assert q(source, 'SELECT max(version) FROM cs_schema_migrations').strip() == '40'
    assert not args.directory.exists(), 'Preserve existing preparation; use explicit reconciliation, never reset'
    before, keys = snapshot(source), preservation.file_fingerprints(config)
    args.directory.mkdir(parents=True, mode=0o700)
    preservation.save(args.directory / 'source-before.json', {'tables': before, 'keys': keys})
    backup = args.directory / 'source-backup.dump'
    run('pg_dump', source, '--format=custom', '--file=' + str(backup))
    run('createdb', name)  # Refuses an existing destination.
    run('pg_restore', '--exit-on-error', '--single-transaction', '--no-owner', '--dbname=' + destination, str(backup))
    assert snapshot(destination) == before, 'Fresh copy mismatch'
    tool = root / 'tools/development-continuity/target/debug/alias-assessment'
    plan = args.directory / 'scenario-plan.json'
    # Challenge the named claim that aliases and address correction are one atomic
    # change: rollback after every write must leave the whole original snapshot.
    print(run(str(tool), destination, str(plan), 'rollback').decode().strip())
    assert snapshot(destination) == before, 'Rollback changed the copy'
    print(run(str(tool), destination, str(plan), 'apply').decode().strip())
    after = snapshot(destination)
    changed = {key for key in before if before[key] != after[key]}
    assert changed == {'cs_address_directory', 'cs_alias_routes'}, changed
    assert after['cs_alias_routes']['rows'] == 3 and before['cs_alias_routes']['rows'] == 0
    assert after['cs_address_directory']['rows'] == before['cs_address_directory']['rows']
    # Exact retry must reconcile rather than duplicate routes or advance revisions.
    print(run(str(tool), destination, str(plan), 'apply').decode().strip())
    assert snapshot(destination) == after, 'Retry changed the scenario'
    assert snapshot(source) == before, 'Source was changed'
    assert preservation.file_fingerprints(config) == keys, 'Key files changed'
    config['database_url'] = destination
    config['directory_domain'] = 'c-sqdmail.com'
    config['external']['mail_domain'] = 'c-sqdmail.com'
    preservation.save(args.directory / 'maild.json', config)
    preservation.save(args.directory / 'verified.json', {
        'source': source, 'destination': destination, 'source_unchanged': True,
        'key_files_unchanged': True, 'rollback_unchanged': True, 'exact_retry_unchanged': True,
        'unchanged_tables_and_sequences': len(before) - 2,
        'changed_tables': sorted(changed), 'after': after,
        'scope': 'Local fixture only; same original account, persona and endpoint keys. No message backfill.'})
    print(f'PASS {len(before) - 2} table/sequence fingerprints unchanged; only directory mailbox and three checked alias records differ.')


if __name__ == '__main__':
    main()
