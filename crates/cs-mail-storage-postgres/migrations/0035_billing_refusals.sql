DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_deployment) THEN
  RAISE EXCEPTION 'Billing refusal cutover requires a fresh environment; preserve populated development accounts through the explicit continuity upgrade';
 END IF;
END $$;
-- Existing successes retain their recorded operations. A refusal is a separate terminal fact.
ALTER TABLE cs_billing_commands ADD COLUMN refusal JSONB;
ALTER TABLE cs_billing_commands ADD CHECK (refusal IS NULL OR outcome = '[]'::jsonb);
INSERT INTO cs_schema_migrations(version) VALUES(35);
