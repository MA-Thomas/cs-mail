-- Old received lane grants retained checked contents but not the signature needed
-- to restore activation authority. No compatibility reader or inferred proof.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_deployment)
    OR EXISTS (SELECT 1 FROM cs_relationship_aggregates)
    OR EXISTS (SELECT 1 FROM cs_product_accounts)
    OR EXISTS (SELECT 1 FROM cs_received_commands)
    OR EXISTS (SELECT 1 FROM cs_capability_lanes) THEN
  RAISE EXCEPTION 'Inbound and lane authority requires a fresh development database; preserve the old database for inspection';
 END IF;
END $$;
INSERT INTO cs_schema_migrations(version) VALUES(29);
