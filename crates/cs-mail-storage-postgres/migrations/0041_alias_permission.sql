-- Explicit address/permission cutover. No runtime predecessor reader exists.
DO $$ BEGIN
 IF (EXISTS (SELECT 1 FROM cs_alias_routes) OR EXISTS (SELECT 1 FROM cs_alias_commands))
    AND current_setting('cs_mail.alias_permission_preservation',true) IS DISTINCT FROM 'on' THEN
  RAISE EXCEPTION 'Populated aliases require an offline preservation-checked copy';
 END IF;
END $$;
-- Original bytes remain evidence, never fallback authority or a replay implementation.
CREATE TABLE cs_alias_permission_cutover_evidence (
 source_table TEXT NOT NULL,
 identity TEXT NOT NULL,
 record JSONB NOT NULL,
 PRIMARY KEY(source_table,identity)
);
INSERT INTO cs_alias_permission_cutover_evidence
 SELECT 'cs_alias_routes',id,to_jsonb(a) FROM cs_alias_routes a;
INSERT INTO cs_alias_permission_cutover_evidence
 SELECT 'cs_alias_commands',account || ':' || operation,to_jsonb(a) FROM cs_alias_commands a;
INSERT INTO cs_alias_permission_cutover_evidence
 SELECT 'cs_alias_deliveries',message || ':' || transport,to_jsonb(a) FROM cs_alias_deliveries a;
UPDATE cs_alias_routes SET record = (record - 'lane') || jsonb_build_object('permission',
 jsonb_build_object('alias',record->'id','owner',record->'owner',
  'revision',CASE WHEN record->'lane' IS NOT NULL AND record->'lane'<>'null'::jsonb THEN 1 ELSE 0 END,
  'changed_at',record->'created_at',
  'state',CASE WHEN record->'lane' IS NOT NULL AND record->'lane'<>'null'::jsonb
   THEN jsonb_build_object('Broad',record->'lane') ELSE '"None"'::jsonb END));
-- v2 signed requests/receipts are preserved verbatim above; v3 has one current replay owner.
DELETE FROM cs_alias_commands;
UPDATE cs_alias_deliveries SET evidence=jsonb_set(evidence,'{basis}',
 '{"AliasLane":{"permission_revision":1}}'::jsonb) WHERE evidence->>'basis'='AliasLane';
ALTER TABLE cs_alias_routes ADD CONSTRAINT cs_alias_permission_record CHECK (
 record ? 'permission' AND NOT record ? 'lane'
 AND record#>'{permission,alias}'=record->'id'
 AND record#>'{permission,owner}'=record->'owner'
);
REVOKE ALL ON cs_alias_permission_cutover_evidence FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(41);
