-- Current native documents bind an encrypted immutable shared subject.
-- Populated predecessor data needs an explicit preservation-checked offline copy.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_deployment) OR EXISTS(SELECT 1 FROM cs_conversations)
 OR EXISTS(SELECT 1 FROM cs_external_conversations) THEN
  RAISE EXCEPTION 'Shared subjects require a fresh environment; preserve the prior database';
 END IF;
END $$;
INSERT INTO cs_schema_migrations(version) VALUES(42);
