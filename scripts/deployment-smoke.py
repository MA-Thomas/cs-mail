#!/usr/bin/env python3
"""Local callback-only deployment recovery experiment; never contacts providers.

Requires production cs-maild/admin binaries and PostgreSQL client tools on PATH.
PGHOST/PGPORT/PGUSER/PGDATABASE select an isolated test cluster and maintenance DB.
Creates and drops only two uniquely named databases of its own. Requires CREATEDB.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("database", ROOT / "deploy/linux/database.py")
database = importlib.util.module_from_spec(spec)
spec.loader.exec_module(database)


def sql(statement):
    return database.run("psql", "-XAt", "--set=ON_ERROR_STOP=1", "--command", statement)


def write(path, value):
    path.write_text(json.dumps(value))


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def http(port_number, route, post=False):
    # Ignore proxy environment variables: these are loopback-only experiments.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    request = urllib.request.Request(f"http://127.0.0.1:{port_number}{route}",
                                     data=b"{}" if post else None)
    try:
        with opener.open(request, timeout=2) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    parser.add_argument("--allow-create-test-databases", action="store_true", required=True)
    args = parser.parse_args()
    maild, admin = [str(args.bin_dir.resolve() / name) for name in ["cs-maild", "cs-mail-admin"]]
    if not os.environ.get("PGDATABASE"):
        raise RuntimeError("set PGDATABASE to the test maintenance database")
    maintenance = os.environ["PGDATABASE"]
    names = ["cs_mail_deploy_" + secrets.token_hex(8) for _ in range(2)]
    created = []
    process = None
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix="cs-mail-deployment-") as directory:
        work = Path(directory)
        log = (work / "serve.log").open("wb")
        try:
            for name in names:
                sql(f'CREATE DATABASE "{name}" TEMPLATE template0')
                created.append(name)
            os.environ["PGDATABASE"] = names[0]
            keys = {}
            for name in ["provider", "scheduler", "operator", "processor", "bank", "identity"]:
                keys[name] = json.loads(database.run(admin, "keygen", "--out", str(work / f"{name}.json")))["verifying_key"]
            for name in ["relationship", "principal", "external-content", "webhook"]:
                write(work / f"{name}.json", list(secrets.token_bytes(32)))
            domain = list(secrets.token_bytes(32))
            record = {
                "provider": 30, "deployment_domain": domain, "unit": 1,
                "financial_scope": {"deployment_domain": domain, "operator": 30, "program": 1,
                                    "payment_account": list(secrets.token_bytes(32)), "protocol_version": 2},
                "provider_key": {"reference": 3, "verifying_key": keys["provider"]},
                "scheduler_key": {"reference": 4, "verifying_key": keys["scheduler"]},
                "operators": {"1": keys["operator"]},
                "payment": {"processor_key": keys["processor"], "bank_authority": keys["bank"]},
                "identity": {"issuer": "https://identity.localhost", "product": "cs-mail/deployment-test",
                             "decision_key": keys["identity"]}}
            write(work / "bootstrap.json", record)
            config = json.loads((ROOT / "deploy/linux/maild.json.example").read_text())
            main_port, callback_port = port(), port()
            while callback_port == main_port:
                callback_port = port()
            config["listen"] = f"127.0.0.1:{main_port}"
            config["external"]["feedback"]["listen"] = f"127.0.0.1:{callback_port}"
            for field, stem in [("provider_secret_file", "provider"), ("relationship_secret_file", "relationship"), ("principal_secret_file", "principal")]:
                config["members"][field] = str(work / f"{stem}.json")
            config["external"]["content_secret_file"] = str(work / "external-content.json")
            config["external"]["feedback"]["secret_file"] = str(work / "webhook.json")

            def configure(name):
                # tokio-postgres understands these libpq-style parameters, but does
                # not obtain them from the client's PG* environment itself.
                def quoted(value):
                    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"
                values = {"dbname": name, "application_name": names[0]}
                for variable, key in [("PGHOST", "host"), ("PGPORT", "port"), ("PGUSER", "user"), ("PGPASSWORD", "password")]:
                    if os.environ.get(variable):
                        values[key] = os.environ[variable]
                if "user" not in values:
                    raise RuntimeError("set PGUSER explicitly for this experiment")
                config["database_url"] = " ".join(f"{k}={quoted(v)}" for k, v in values.items())
                write(work / "maild.json", config)

            def start():
                nonlocal process
                process = subprocess.Popen([maild, "serve", "--config", str(work / "maild.json")], stdout=log, stderr=log)
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError("service exited before readiness")
                    try:
                        if http(main_port, "/ready") == 200:
                            assert http(callback_port, "/v1/external/smtp2go/events", True) == 401
                            assert http(callback_port, "/ready") == 404
                            assert http(main_port, "/v1/external/smtp2go/events", True) == 404
                            return
                    except (OSError, urllib.error.URLError):
                        pass
                    time.sleep(0.1)
                raise RuntimeError("service did not become ready")

            def stop():
                nonlocal process
                process.send_signal(signal.SIGINT)
                assert process.wait(timeout=15) == 0, "graceful shutdown failed"
                process = None

            def snapshot():
                return sql("SELECT json_build_object('deployment', (SELECT json_agg(d) FROM cs_deployment d), 'commands', (SELECT json_agg(c ORDER BY family,idempotency_key) FROM cs_operator_commands c), 'schema', (SELECT max(version) FROM cs_schema_migrations));")

            configure(names[0])
            database.run(maild, "migrate", "--config", str(work / "maild.json"))
            for _ in range(2):
                database.run(maild, "bootstrap", "--config", str(work / "maild.json"), "--deployment", str(work / "bootstrap.json"))
            start()
            write(work / "admin.json", {"server": f"http://127.0.0.1:{main_port}", "operator": 1,
                  "operator_secret_file": str(work / "operator.json"), "deployment_domain": domain})
            write(work / "pricing.json", {"PublishRequestPricing": {"version": 1, "unit": 1,
                  "processing_charge": 50, "collateral_min": 100, "collateral_max": 2500}})
            database.run(admin, "configure", "--config", str(work / "admin.json"), "--command", str(work / "pricing.json"))
            before = snapshot()
            data = json.loads(before)
            assert data["schema"] == 30 and len(data["deployment"]) == 1 and len(data["commands"]) == 1
            process.kill()
            process.wait(timeout=15)
            process = None
            start()
            assert snapshot() == before, "durable facts changed after process crash"
            # Terminate only this experiment's tagged connections, not the server.
            terminated = sql(f"SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = '{names[0]}' AND datname = '{names[0]}'")
            assert terminated.splitlines() and all(result == b"t" for result in terminated.splitlines()), "fault injection did not terminate service connections"
            assert process.wait(timeout=15) != 0, "lost database connection did not fail the process"
            process = None
            start()
            stop()
            archive = work / "database.dump"
            database.backup(archive)
            original = archive.read_bytes()
            try:
                database.backup(archive)
            except RuntimeError:
                pass
            else:
                raise RuntimeError("backup overwrote an existing archive")
            assert archive.read_bytes() == original
            os.environ["PGDATABASE"] = names[1]
            damaged = work / "damaged.dump"
            damaged.write_bytes(original[:len(original) // 2])
            try:
                database.restore(damaged)
            except RuntimeError:
                pass
            else:
                raise RuntimeError("restore accepted an incomplete archive")
            assert sql("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'").strip() == b"0", "failed restore left partial application objects"
            database.restore(archive)
            assert snapshot() == before, "restored deployment/audit/schema differ"
            try:
                database.restore(archive)
            except RuntimeError:
                pass
            else:
                raise RuntimeError("restore accepted a populated database")
            assert snapshot() == before, "refused restore modified durable facts"
            configure(names[1])
            start()
            stop()
            log.flush()
            logs = (work / "serve.log").read_text()
            compact = "".join(logs.split())
            for name in ["provider", "operator", "relationship", "principal", "external-content", "webhook"]:
                assert "".join((work / f"{name}.json").read_text().split()) not in compact, "secret in service log"
            assert config["database_url"] not in logs, "database URL in service log"
            print("PASS: callback isolation, crash/restart, database-loss exit, graceful shutdown, backup/restore, overwrite refusal, log redaction (schema 30)")
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait(timeout=15)
            log.close()
            os.environ["PGDATABASE"] = maintenance
            for name in created:
                sql(f'DROP DATABASE "{name}" WITH (FORCE)')


if __name__ == "__main__":
    main()
