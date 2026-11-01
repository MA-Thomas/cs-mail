"""Apply an explicitly prepared ciphertext-only conversion under exclusive device ownership."""
import fcntl, hashlib, json, sqlite3, sys
from pathlib import Path
path, plan_path = map(Path, sys.argv[1:])
assert path.is_absolute() and plan_path.is_absolute()
plan = json.loads(plan_path.read_text())
with (path.parent / 'client.lock').open('rb') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    with sqlite3.connect(path) as db:
        db.execute('PRAGMA synchronous=FULL')
        db.execute('PRAGMA secure_delete=ON')
        db.execute('BEGIN IMMEDIATE')
        source = db.execute('SELECT ciphertext FROM device_cache WHERE id=1').fetchone()[0]
        assert hashlib.sha256(source).digest() == bytes(plan['source_digest'])
        protected = {}
        for table in ('member_workspace','personal_mailbox','retained_progress','retained_metadata','retained_index','retained_copies'):
            protected[table] = db.execute(f'SELECT * FROM {table} ORDER BY 1').fetchall()
        db.execute('CREATE TABLE IF NOT EXISTS mailbox_records(id BLOB PRIMARY KEY CHECK(length(id)=32),ciphertext BLOB NOT NULL) WITHOUT ROWID')
        assert db.execute('SELECT count(*) FROM mailbox_records').fetchone()[0] == 0
        keep = {bytes(value) for value in plan['keep']}
        assert len(keep) == len(plan['keep'])
        for row in plan['writes']:
            assert bytes(row['id']) in keep
            db.execute('INSERT INTO mailbox_records VALUES(?,?)',(bytes(row['id']),bytes(row['ciphertext'])))
        assert {r[0] for r in db.execute('SELECT id FROM mailbox_records')} == keep
        db.execute('UPDATE device_cache SET ciphertext=? WHERE id=1',(bytes(plan['head']),))
        for table, before in protected.items():
            assert db.execute(f'SELECT * FROM {table} ORDER BY 1').fetchall() == before
        db.commit()
print('PASS atomic mailbox conversion; writing, personal state, retained copies and search records unchanged')
