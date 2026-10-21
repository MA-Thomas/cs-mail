-- Explicit format cutover: never guess private descriptions or rewrite signed history.
-- A populated legacy alias deployment needs a separately designed preservation upgrade.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_alias_routes) OR EXISTS (SELECT 1 FROM cs_alias_commands) THEN
  RAISE EXCEPTION 'Alias creation cutover requires empty legacy alias metadata; preserve the deployment for explicit upgrade';
 END IF;
END $$;
ALTER TABLE cs_alias_routes ADD CONSTRAINT cs_alias_checked_record CHECK (
 record->>'id'=id AND record->>'owner'=owner AND record->>'address'=address
 AND record ? 'usage' AND NOT record ? 'kind'
);
CREATE UNIQUE INDEX cs_alias_one_current_public ON cs_alias_routes(owner)
 WHERE record->>'usage'='Public'
 AND (record->>'lifecycle'='Active' OR record->'lifecycle' ? 'Restricted');
INSERT INTO cs_schema_migrations(version) VALUES(39);
