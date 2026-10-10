-- Existing guest preparations have no trustworthy original-input fingerprint, and old
-- repeated revocations may have erased a cutoff. Do not invent either fact on upgrade.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_deployment)
    OR EXISTS (SELECT 1 FROM cs_relationship_aggregates)
    OR EXISTS (SELECT 1 FROM cs_product_accounts)
    OR EXISTS (SELECT 1 FROM cs_guest_sessions) THEN
  RAISE EXCEPTION 'Audit hardening requires a fresh development database; preserve the old database for inspection';
 END IF;
END $$;
-- Input identities outlive preparation attempts so retries cannot replace guest text.
CREATE TABLE cs_guest_request_inputs (
 sender TEXT NOT NULL,
 operation TEXT NOT NULL,
 fingerprint JSONB NOT NULL,
 PRIMARY KEY(sender, operation)
);
REVOKE ALL ON cs_guest_request_inputs FROM PUBLIC;
-- Processing faults are observations, never fabricated command outcomes.
ALTER TABLE cs_received_commands ADD COLUMN processing_failure JSONB;
ALTER TABLE cs_received_commands ADD COLUMN failed_attempts BIGINT NOT NULL DEFAULT 0 CHECK(failed_attempts >= 0);
INSERT INTO cs_schema_migrations(version) VALUES(27);
