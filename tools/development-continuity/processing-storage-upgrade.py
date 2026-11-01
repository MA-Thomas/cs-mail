"""Prepare a fresh, preservation-checked local copy for observation and processing-owner storage."""
import hashlib, json, os, subprocess, sys
from pathlib import Path
from urllib.parse import urlsplit
source_config, destination, output = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3])
assert source_config.is_absolute() and output.is_absolute() and not output.exists()
config = json.loads(source_config.read_text())
source = config['database_url']
assert all(urlsplit(url).hostname in ('localhost','127.0.0.1','::1') for url in (source,destination))
assert urlsplit(source).path != urlsplit(destination).path
os.umask(0o077)
def run(*args):
    return subprocess.run(args,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE).stdout

def query(url,sql):
    return run('psql',url,'-XAt','-v','ON_ERROR_STOP=1','-c',sql).decode()
assert query(source,'SELECT max(version) FROM cs_schema_migrations').strip() == '44'
assert query(source,'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip() == '0'
name = urlsplit(destination).path.lstrip('/')
assert name.replace('_','').isalnum()
assert query('postgresql://localhost/postgres',"SELECT count(*) FROM pg_database WHERE datname='"+name+"'").strip() == '0'
tables = query(source,"SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'cs_schema_migrations' ORDER BY tablename").splitlines()
sequences = query(source,"SELECT sequencename FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename").splitlines()
def snapshot(url):
    result = {}
    for table in tables:
        ident = '"'+table.replace('"','""')+'"'
        # The new owner fence is added metadata, not a rewritten original command fact.
        expression = "to_jsonb(t)-'ordering_scopes'" if table == 'cs_received_commands' else 'to_jsonb(t)'
        rows = query(url,f'SELECT ({expression})::text FROM {ident} t ORDER BY ({expression})::text')
        result[table] = {'rows':len(rows.splitlines()),'sha256':hashlib.sha256(rows.encode()).hexdigest()}
    for sequence in sequences:
        rows = query(url,'SELECT last_value,is_called FROM "'+sequence+'"')
        result['@sequence:'+sequence] = hashlib.sha256(rows.encode()).hexdigest()
    return result

def keys(value):
    found = {}
    if isinstance(value,dict):
        for key,item in value.items():
            if key.endswith('_file') and isinstance(item,str):
                found[item] = hashlib.sha256(Path(item).read_bytes()).hexdigest()
            else: found.update(keys(item))
    elif isinstance(value,list):
        for item in value: found.update(keys(item))
    return found
before, key_hashes = snapshot(source), keys(config)
output.mkdir(mode=0o700)
backup = output/'source.dump'
run('pg_dump','--format=custom','--file='+str(backup),source)
run('createdb','--maintenance-db=postgresql://localhost/postgres',name)
run('pg_restore','--exit-on-error','--no-owner','--no-privileges','--dbname='+destination,str(backup))
root = Path(__file__).resolve().parents[2]
sql = output/'upgrade.sql'
sql.write_text('\n'.join((root/'crates/cs-mail-storage-postgres/migrations'/file).read_text() for file in ('0045_external_observations.sql','0046_processing_owners.sql')))
run('psql',destination,'-X','-v','ON_ERROR_STOP=1','--single-transaction','--file='+str(sql))
assert snapshot(destination) == before and snapshot(source) == before and keys(config) == key_hashes
config['database_url'] = destination
(output/'maild.json').write_text(json.dumps(config,indent=2)+'\n')
(output/'verified.json').write_text(json.dumps({'source':source,'destination':destination,'original_tables_and_sequences':before,'key_files':key_hashes,'schema':46,'source_unchanged':True,'original_facts_unchanged':True},indent=2)+'\n')
print(f'PASS {len(tables)} original tables and {len(sequences)} sequences preserved; account/key/address facts unchanged')
