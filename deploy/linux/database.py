#!/usr/bin/env python3
"""Database-only backup/restore. Uses standard libpq PG* environment variables.

Restore requires a new empty database with the service stopped and no concurrent
writers. It never drops a database or cleans an existing schema. Config and secret
key custody are separate from the database archive.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile


def run(*args):
    # Do not echo a libpq diagnostic: it can include connection credentials.
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed (exit {result.returncode}); inspect connection and tool versions locally")
    return result.stdout


def backup(destination):
    if not os.environ.get("PGDATABASE"):
        raise RuntimeError("backup requires explicit PGDATABASE")
    destination = Path(destination)
    if destination.exists():
        raise RuntimeError("backup destination already exists")
    fd, temporary = tempfile.mkstemp(prefix=".cs-mail-backup-", dir=destination.parent)
    os.close(fd)
    try:
        run("pg_dump", "--no-password", "--format=custom", "--no-owner", "--no-privileges", "--file", temporary)
        run("pg_restore", "--list", temporary)
        with open(temporary, "rb") as archive:
            os.fsync(archive.fileno())
        # Publish only a complete archive, without overwriting a concurrent backup.
        os.link(temporary, destination)
        directory = os.open(destination.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        os.unlink(temporary)


def restore(archive):
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", os.environ.get("PGDATABASE", "")):
        raise RuntimeError("restore requires PGDATABASE as a plain database name; use other PG* variables for connection settings")
    # Reject application objects, including non-public schemas. System-owned
    # extension objects in the public schema are refused too, conservatively.
    count = run("psql", "--no-password", "-XAt", "--set=ON_ERROR_STOP=1", "--command", """
        SELECT (SELECT count(*) FROM pg_namespace WHERE nspname <> 'public'
                  AND nspname <> 'information_schema' AND left(nspname, 3) <> 'pg_')
             + (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
                  WHERE n.nspname = 'public')
             + (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
                  WHERE n.nspname = 'public')
             + (SELECT count(*) FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace
                  WHERE n.nspname = 'public');
    """)
    if count.strip() != b"0":
        raise RuntimeError("restore refused: target is not empty")
    run("pg_restore", "--no-password", "--exit-on-error", "--single-transaction", "--no-owner",
        "--no-privileges", "--dbname", os.environ["PGDATABASE"], str(archive))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["backup", "restore"])
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    if args.operation == "backup":
        backup(args.archive)
    else:
        restore(args.archive)
    print(f"{args.operation} complete")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError) as error:
        raise SystemExit(str(error)) from None
