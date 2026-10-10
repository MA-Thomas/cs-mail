-- Old sessions contain no link to their consumed verification. No backfill can
-- establish that provenance. Preserve old development databases for inspection.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_deployment)
    OR EXISTS (SELECT 1 FROM cs_relationship_aggregates)
    OR EXISTS (SELECT 1 FROM cs_product_accounts)
    OR EXISTS (SELECT 1 FROM cs_guest_verifications)
    OR EXISTS (SELECT 1 FROM cs_guest_sessions) THEN
  RAISE EXCEPTION 'Guest verification requires a fresh development database; preserve the old database for inspection';
 END IF;
END $$;
ALTER TABLE cs_guest_sessions DROP COLUMN session;
ALTER TABLE cs_guest_sessions ADD COLUMN verification_id TEXT NOT NULL
 UNIQUE REFERENCES cs_guest_verifications(id);
COMMENT ON TABLE cs_guest_sessions IS
 'One token per consumed verification. Session facts are owned by the verification record.';
INSERT INTO cs_schema_migrations(version) VALUES(28);
