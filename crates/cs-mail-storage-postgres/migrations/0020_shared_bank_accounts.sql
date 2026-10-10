-- Whether one person may hold a second account is the identity service's decision under
-- cs-mail's evidence policy (docs/shared-identity-contract.md); cs-mail keeps no person
-- evidence. Accounts whose enrollment review approved it may share a bank account, so the
-- bank token is no longer unique among enrollments, and each billing account has its own
-- funding source for it. A payment operation names the funding source it was reserved on.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_enrollment_operations) OR EXISTS (SELECT 1 FROM cs_funding_sources) THEN
   RAISE EXCEPTION 'shared bank accounts require a fresh database or a reviewed data migration';
 END IF;
END $$;
ALTER TABLE cs_enrollment_operations DROP CONSTRAINT cs_enrollment_operations_bank_token_key;
ALTER TABLE cs_billing_accounts DROP COLUMN person;
CREATE TABLE cs_funding_source_operations (
 operation TEXT PRIMARY KEY,
 source_key TEXT NOT NULL REFERENCES cs_funding_sources(source_key)
);
REVOKE ALL ON cs_funding_source_operations FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(20);
