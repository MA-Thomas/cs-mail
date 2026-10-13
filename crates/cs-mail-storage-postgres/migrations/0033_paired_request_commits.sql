-- A received command may change both directed relationships. Both records share
-- one causal position and commit boundary; a second allocated position could
-- overtake an already received command and make cursor replay miss its effects.
-- This is an explicit fresh-environment cutover, with no old-reader fallback.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_deployment)
    OR EXISTS (SELECT 1 FROM cs_relationship_aggregates)
    OR EXISTS (SELECT 1 FROM cs_product_accounts)
    OR EXISTS (SELECT 1 FROM cs_received_commands) THEN
  RAISE EXCEPTION 'Paired request commits require a fresh development database; preserve the old database for inspection';
 END IF;
END $$;
ALTER TABLE cs_canonical_journal DROP CONSTRAINT cs_canonical_journal_pkey;
ALTER TABLE cs_canonical_journal ADD PRIMARY KEY(position, aggregate_key);
INSERT INTO cs_schema_migrations(version) VALUES(33);
