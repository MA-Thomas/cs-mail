-- Receiving evidence is distinct from message content and scoped by transport.
-- The preceding management-only implementation never committed delivery evidence.
-- Refuse populated experimental rows rather than inventing provenance for them.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_alias_deliveries) THEN
  RAISE EXCEPTION 'Populated receiving evidence requires an explicit preservation-checked cutover';
 END IF;
END $$;
ALTER TABLE cs_alias_deliveries ADD COLUMN transport TEXT NOT NULL DEFAULT 'native' CHECK (transport IN ('native','external'));
ALTER TABLE cs_alias_deliveries ALTER COLUMN transport DROP DEFAULT;
ALTER TABLE cs_alias_deliveries DROP CONSTRAINT cs_alias_deliveries_pkey;
ALTER TABLE cs_alias_deliveries ADD PRIMARY KEY (message,transport);
CREATE INDEX cs_alias_inbox ON cs_alias_deliveries(alias,admitted_at,message);
INSERT INTO cs_schema_migrations(version) VALUES(40);
