DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_deployment) THEN
  RAISE EXCEPTION 'External receipt cutover requires a fresh environment or an explicit preservation-checked continuity upgrade';
 END IF;
END $$;
-- An attempted conversation need not exist: an authenticated refusal creates no route.
-- The account remains the owner of the operation and its durable receipt.
ALTER TABLE cs_external_receipts DROP CONSTRAINT cs_external_receipts_conversation_fkey;
INSERT INTO cs_schema_migrations(version) VALUES(37);
