#!/usr/bin/env python3
"""Explicit local schema-43 preservation plan. Never used by runtime readers."""
import argparse, copy, fcntl, hashlib, importlib.util, json, os, sqlite3, subprocess, time
from pathlib import Path
from urllib.parse import urlsplit
spec=importlib.util.spec_from_file_location('preservation',Path(__file__).with_name('external_receipts_upgrade.py'))
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
run,query,snapshot,save,files=(getattr(p,n) for n in ('run','query','snapshot','save','file_fingerprints'))
# Reviewed receiving choices for this one preserved local owner scenario.
# Refuse unknown aliases instead of deriving authority from a historical label.
RECEIVING_PLAN = {
    '191750318574314731376648966290922670891': ('daniel@csqd-mail.com', 'Bonded'),
    '269381521478073367553694967544091501631': ('maya@csqd-mail.com', 'Bonded'),
    '300992556416108990654926396239366104621': ('elena@csqd-mail.com', 'Bonded'),
    '334731753317584173398332649621964872932': ('jules@csqd-mail.com', 'Bonded'),
    '62831001': ('marcus.thomas@c-sqdmail.com', 'Bonded'),
    '62831002': ('mat7771@c-sqdmail.com', 'AliasScoped'),
    '62831003': ('m11a33t@c-sqdmail.com', 'AliasScoped'),
}
def rows(url,table): return [json.loads(v) for v in query(url,f'SELECT to_jsonb(t)::text FROM {table} t ORDER BY to_jsonb(t)::text').splitlines()]
def lit(value): return "'"+str(value).replace("'","''")+"'"
def js(value): return lit(json.dumps(value,separators=(',',':')))+'::jsonb'
def transform(value,plan):
    value=copy.deepcopy(value)
    def visit(v):
        if isinstance(v,list):
            for e in v: visit(e)
        elif isinstance(v,dict):
            if {'selected_class','quote_id','recipient','sender'} <= v.keys():
                assert 'receiving_alias' not in v
                v['receiving_alias']=plan['receiving'][str(v['recipient'])]
                v['return_route']={'Native':plan['receiving'][str(v['sender'])]}
            for e in v.values():visit(e)
    visit(value);return value

def backend(args):
    config=json.loads(args.source_config.read_text());source=config['database_url'];dest=args.destination_url
    assert source!=dest and all(urlsplit(u).hostname in ('localhost','127.0.0.1','::1') for u in (source,dest))
    assert urlsplit(dest).path.startswith('/cs_mail_unified_aliases_')
    assert query(source,'SELECT max(version) FROM cs_schema_migrations').strip()=='43'
    assert query(source,'SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()').strip()=='0','Stop the original service first'
    assert all(set(r['request']['lifecycle']) & {'Accepted','Rejected','Expired'} for r in rows(source,'cs_requests')),'Unsettled request requires a separate preservation plan'
    before=snapshot(source);keys=files(config);args.directory.mkdir(mode=0o700,parents=True,exist_ok=True)
    prepared={'before':before,'keys':keys}
    if (args.directory/'source-before.json').exists(): assert json.loads((args.directory/'source-before.json').read_text())==prepared
    else:save(args.directory/'source-before.json',prepared)
    backup=args.directory/'source.dump'
    if not backup.exists():run('pg_dump',source,'--format=custom','--file='+str(backup))
    fresh=query(dest,"SELECT count(*) FROM pg_tables WHERE schemaname='public'").strip()=='0'
    if fresh:run('pg_restore','--exit-on-error','--single-transaction','--no-owner','--dbname='+dest,str(backup))
    planfile=args.directory/'alias-plan.json'
    if query(dest,'SELECT max(version) FROM cs_schema_migrations').strip()=='44':
        assert (args.directory/'verified.json').exists();assert snapshot(dest)==json.loads((args.directory/'verified.json').read_text())['after'];print('PASS exact preservation retry; destination unchanged');return
    assert snapshot(dest)==before
    aliases=rows(source,'cs_alias_routes');entries=rows(source,'cs_address_directory')
    at=time.time_ns()//1_000_000
    owner=next(a['owner'] for a in aliases if a['address']=='marcus.thomas@c-sqdmail.com')
    account=query(source, f"SELECT account FROM cs_persona_owners WHERE identity={lit(owner)}").strip();assert account.isdigit()
    plan={'account':account,'aliases':{},'receiving':{},'publications':{},'new_aliases':[],'new_classes':[],'new_entries':[],'scopes':[],'new_requests':[],'at':at}
    # This is an explicit preservation table of the previous admitted routes, not
    # a runtime public/private reader. The plan is retained as upgrade provenance.
    assert {row['id'] for row in aliases} == set(RECEIVING_PLAN), 'Unreviewed source aliases'
    for row in aliases:
        old=row['record'];usage=old['usage'];address,policy=RECEIVING_PLAN[row['id']]
        assert row['address']==address, 'Receiving plan address differs'
        description=old['address'] if usage=='Public' else usage['Private']['description']
        new=copy.deepcopy(old);del new['usage'];new['configuration']={'description':description,'receiving':policy}
        plan['aliases'][row['id']]={'owner':old['owner'],'address':row['address'],'usage':usage,'configuration':new['configuration']}
        plan['new_aliases'].append(new)
        if policy=='Bonded' and old['lifecycle'] in ('Active',):
            assert row['owner'] not in plan['receiving'];plan['receiving'][row['owner']]=int(row['id'])
    # Preserve the actual existing publication address. Marcus's original protocol
    # address and his separate alias must not be collapsed or renamed.
    used={str(v['id']) for v in plan['new_aliases']}
    for entry in entries:
        owner=entry['address'];address=entry['mailbox'];assert address
        matches=[v for v in plan['new_aliases'] if v['address']==address and str(v['owner'])==owner]
        if matches:identifier=matches[0]['id']
        else:
            identifier=62831000 if address=='mtallis62831@c-sqdmail.com' else int(owner)
            assert str(identifier) not in used;used.add(str(identifier))
            new={'id':identifier,'owner':int(owner),'address':address,'configuration':{'description':address,'receiving':'Bonded'},'created_at':at,'changed_at':at,'revision':0,'lifecycle':'Active','permission':{'alias':identifier,'owner':int(owner),'revision':0,'changed_at':at,'state':'None'}}
            plan['new_aliases'].append(new)
        plan['publications'][owner]=identifier
        plan['receiving'].setdefault(owner,identifier)
        item=copy.deepcopy(entry['entry']);item['alias']=identifier;plan['new_entries'].append(item)
    sql=['CREATE TABLE cs_unified_alias_evidence(source_table TEXT NOT NULL, ordinal BIGINT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(source_table,ordinal));','REVOKE ALL ON cs_unified_alias_evidence FROM PUBLIC;']
    changed={'cs_alias_routes','cs_alias_commands','cs_address_directory','cs_directory_commands','cs_request_class_publications','cs_recipient_request_classes','cs_relationship_aggregates','cs_requests','cs_alias_sender_scopes','cs_contact_quotes'}
    for table in sorted(changed):sql.append(f'INSERT INTO cs_unified_alias_evidence SELECT {lit(table)},row_number() OVER(ORDER BY to_jsonb(t)::text),to_jsonb(t) FROM {table} t;')
    sql+=['ALTER TABLE cs_alias_routes DROP CONSTRAINT cs_alias_checked_record;','ALTER TABLE cs_address_directory ADD COLUMN alias TEXT;','ALTER TABLE cs_request_class_publications ADD COLUMN alias TEXT;','ALTER TABLE cs_recipient_request_classes ADD COLUMN alias TEXT;']
    for alias in plan['new_aliases']:
        sql.append(f"INSERT INTO cs_alias_routes(id,owner,address,record) VALUES({lit(alias['id'])},{lit(alias['owner'])},{lit(alias['address'])},{js(alias)}) ON CONFLICT(id) DO UPDATE SET record=EXCLUDED.record;")
    for row in entries:
        alias=plan['publications'][row['address']];v=copy.deepcopy(row['entry']);v['alias']=alias
        sql.append(f"UPDATE cs_address_directory SET alias={lit(alias)},entry={js(v)} WHERE address={lit(row['address'])};")
    for row in rows(source,'cs_recipient_request_classes'):
        alias=plan['publications'][row['recipient']];v=copy.deepcopy(row['classes']);v['alias']=alias;plan['new_classes'].append(v)
        sql.append(f"UPDATE cs_recipient_request_classes SET alias={lit(alias)},classes={js(v)} WHERE recipient={lit(row['recipient'])};")
    for row in rows(source,'cs_request_class_publications'):
        alias=plan['publications'][row['recipient']];v=copy.deepcopy(row['publication']['classes']);v['alias']=alias
        evidence={'Preserved':{'source_table':'cs_request_class_publications','recipient':row['recipient'],'version':row['version'],'classes':v}}
        sql.append(f"UPDATE cs_request_class_publications SET alias={lit(alias)},publication={js(evidence)} WHERE recipient={lit(row['recipient'])} AND version={row['version']};")
    for table,column in [('cs_requests','request'),('cs_relationship_aggregates','relationship_state')]:
        for row in rows(source,table):
            converted=transform(row[column],plan)
            if table=='cs_requests':plan['new_requests'].append(converted)
            if converted!=row[column]:
                where=f"aggregate_key={lit(row['aggregate_key'])}"+(f" AND request_id={lit(row['request_id'])}" if table=='cs_requests' else '')
                sql.append(f'UPDATE {table} SET {column}={js(converted)} WHERE {where};')
    assert not rows(source,'cs_alias_sender_scopes'),'Existing explicit scope requires a separate reviewed plan'
    for row in rows(source,'cs_relationship_aggregates'):
        relationship=row['relationship_state']['relationship'];state=relationship['state'];key=relationship['key']
        if state!='Accepted' and not (isinstance(state,dict) and 'ExpressLane' in state):continue
        lane=None
        if state!='Accepted':
            lane_rows=[json.loads(v) for v in query(source,f"SELECT lane::text FROM cs_capability_lanes WHERE aggregate_key={lit(row['aggregate_key'])}").splitlines()]
            assert len(lane_rows)==1
            grant=lane_rows[0]['grant'];lane=[grant['id'],grant['version']]
        scope={'revision':1,'established_at':at,'sender':key['sender'],'recipient':key['recipient'],'relationship_version':relationship['version'],'receiving':[plan['receiving'][str(key['recipient'])]],'lane':lane}
        plan['scopes'].append(scope)
        sql.append(f"INSERT INTO cs_alias_sender_scopes(recipient,sender,record) VALUES({lit(key['recipient'])},{lit(key['sender'])},{js(scope)});")
    assert query(source, f"SELECT count(*) FROM cs_contact_quotes WHERE expires_at > {at}").strip()=='0','Unexpired signed quote requires a separate explicit plan'
    sql+=['DELETE FROM cs_contact_quotes;','DELETE FROM cs_alias_commands;','DELETE FROM cs_directory_commands;']
    migration=(Path(__file__).resolve().parents[2]/'crates/cs-mail-storage-postgres/migrations/0044_unified_aliases.sql').read_text().split('DROP INDEX',1)[1]
    migration='DROP INDEX'+migration
    migration=migration.replace('ALTER TABLE cs_alias_routes DROP CONSTRAINT cs_alias_checked_record;','')
    for table in ['cs_address_directory','cs_request_class_publications','cs_recipient_request_classes']:
        migration=migration.replace(f'ALTER TABLE {table} ADD COLUMN alias TEXT NOT NULL;',f'ALTER TABLE {table} ALTER COLUMN alias SET NOT NULL;')
    sql.append(migration)
    for owner,alias in plan['receiving'].items():sql.append(f"INSERT INTO cs_correspondence_routes(owner,alias,revision) VALUES({lit(owner)},{lit(alias)},1);")
    sql.append(f"INSERT INTO cs_unified_alias_evidence VALUES('explicit-plan',1,{js(plan)});")
    save(planfile,plan)
    # Rust's current checked constructors validate the complete proposed plan.
    run(str(args.tool.resolve()),'validate-backend',str(planfile.resolve()))
    wrapper=args.directory/'upgrade.sql';wrapper.write_text('\n'.join(sql))
    run('psql',dest,'-X','-v','ON_ERROR_STOP=1','-c','BEGIN','-f',str(wrapper),'-c','ROLLBACK')
    assert snapshot(dest)==before,'Rollback changed predecessor records'
    run('psql',dest,'-X','--single-transaction','-v','ON_ERROR_STOP=1','-f',str(wrapper))
    after=snapshot(dest);assert set(after)-set(before)=={'cs_correspondence_routes','cs_unified_alias_evidence'}
    assert all(after[t]==before[t] for t in before if t not in changed),'Unrelated facts changed'
    assert snapshot(source)==before and files(config)==keys
    config['database_url']=dest;save(args.directory/'maild.json',config)
    save(args.directory/'verified.json',{'source_unchanged':True,'keys_unchanged':True,'original_evidence_archived':True,'rollback_verified':True,'after':after})
    print('PASS explicit alias mappings, archived originals, account/keys/financial and message preservation, full rollback')

def device(args):
    args.directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    store=args.device_store;lock=(store.parent/'client.lock').open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    db=sqlite3.connect(store);backup=args.directory/'member-before.sqlite'
    if not backup.exists():
        target=sqlite3.connect(backup);db.backup(target);target.close()
    old={t:db.execute('SELECT * FROM '+t).fetchall() for t in ['device_cache','member_workspace','retained_metadata','retained_progress','retained_index','retained_copies','personal_mailbox']}
    output=args.directory/'device-plan.json'
    if not output.exists():run(str(args.helper.resolve()),str(args.tool.resolve()),str(args.plan.resolve()),str(output.resolve()),str(store.resolve()))
    plan=json.loads(output.read_text())
    if old['retained_progress'][0][1] == plan['revision']+1:
        assert (args.directory/'verified.json').exists()
        original=sqlite3.connect(backup)
        for name,table in [('cache','device_cache'),('metadata','retained_metadata'),('workspace','member_workspace')]:
            if plan[name] is not None: assert old[table][0][1] == bytes(plan[name])
        for table in ['retained_index','retained_copies','personal_mailbox']: assert old[table] == original.execute('SELECT * FROM '+table).fetchall()
        print('PASS exact device preservation retry; no credential read or writes');return
    assert old['retained_progress'][0][1]==plan['revision']
    def apply():
        for key,table in [('cache','device_cache'),('metadata','retained_metadata'),('workspace','member_workspace')]:
            if plan[key] is not None:assert db.execute('UPDATE '+table+' SET ciphertext=? WHERE id=1',(bytes(plan[key]),)).rowcount==1
        assert db.execute('UPDATE retained_progress SET revision=revision+1 WHERE id=1 AND revision=?',(plan['revision'],)).rowcount==1
    db.execute('BEGIN IMMEDIATE');apply();db.rollback()
    assert all(db.execute('SELECT * FROM '+t).fetchall()==v for t,v in old.items())
    with db:apply()
    for t in ['retained_index','retained_copies','personal_mailbox']:assert db.execute('SELECT * FROM '+t).fetchall()==old[t]
    save(args.directory/'verified.json',{'account':plan['account'],'atomic_rollback':True,'retained_copies_indexes_personal_unchanged':True,'original_sqlite_backup':str(backup)})
    print('PASS atomic device conversion with untouched retained content, index records and personal organization; existing credential never written')

def main():
    os.umask(0o077);parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='mode',required=True)
    b=sub.add_parser('backend');b.add_argument('--source-config',type=Path,required=True);b.add_argument('--destination-url',required=True);b.add_argument('--directory',type=Path,required=True);b.add_argument('--tool',type=Path,required=True)
    d=sub.add_parser('device');d.add_argument('--device-store',type=Path,required=True);d.add_argument('--directory',type=Path,required=True);d.add_argument('--helper',type=Path,required=True);d.add_argument('--tool',type=Path,required=True);d.add_argument('--plan',type=Path,required=True)
    args=parser.parse_args();(backend if args.mode=='backend' else device)(args)
if __name__=='__main__':main()
