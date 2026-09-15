ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN (1,2,3,4,5,6,7,8));
DROP INDEX cs_directed_pair;
CREATE UNIQUE INDEX cs_directed_pair ON cs_relationship_aggregates
 ((relationship_state #>> '{relationship,key,sender}'),(relationship_state #>> '{relationship,key,recipient}')) WHERE protocol_format_version=8;
ALTER TABLE cs_work DROP CONSTRAINT cs_work_kind_check;
ALTER TABLE cs_work ADD CHECK(kind IN ('delivery','request-payment','member-payment','utility-payment','annual-allocation','prepare-distribution','artifacts'));
CREATE TABLE cs_payment_arrangement (
 singleton BOOLEAN PRIMARY KEY CHECK(singleton), scope JSONB NOT NULL,
 settlement_unit BIGINT NOT NULL, processor_key BYTEA NOT NULL CHECK(octet_length(processor_key)=32),
 bank_authority BYTEA NOT NULL CHECK(octet_length(bank_authority)=32)
);
CREATE TABLE cs_billing_accounts (
 id TEXT PRIMARY KEY, person BYTEA NOT NULL UNIQUE CHECK(octet_length(person)=32), member TEXT NOT NULL UNIQUE,
 record JSONB NOT NULL, ledger JSONB NOT NULL
);
CREATE TABLE cs_billing_identities (identity TEXT PRIMARY KEY, account TEXT NOT NULL REFERENCES cs_billing_accounts(id));
CREATE TABLE cs_account_authorities (account TEXT NOT NULL REFERENCES cs_billing_accounts(id), actor JSONB NOT NULL, PRIMARY KEY(account,actor));
CREATE TABLE cs_remote_identities (identity TEXT PRIMARY KEY, provider TEXT NOT NULL);
CREATE TABLE cs_service_offers (version TEXT PRIMARY KEY, record JSONB NOT NULL);
CREATE TABLE cs_billing_commands (
 account TEXT NOT NULL REFERENCES cs_billing_accounts(id), id TEXT NOT NULL,
 command JSONB NOT NULL, outcome JSONB NOT NULL, received_at BIGINT NOT NULL, PRIMARY KEY(account,id)
);
CREATE TABLE cs_billing_journal (account TEXT NOT NULL REFERENCES cs_billing_accounts(id), event TEXT NOT NULL, entry JSONB NOT NULL, PRIMARY KEY(account,event));
CREATE TABLE cs_funding_sources (source_key TEXT PRIMARY KEY, account TEXT NOT NULL REFERENCES cs_billing_accounts(id), record JSONB NOT NULL);
CREATE TABLE cs_recipient_pricing (recipient TEXT PRIMARY KEY, policy JSONB NOT NULL, preference JSONB);
REVOKE ALL ON cs_payment_arrangement,cs_billing_accounts,cs_billing_identities,cs_account_authorities,cs_remote_identities,cs_service_offers,cs_billing_commands,cs_billing_journal,cs_funding_sources,cs_recipient_pricing FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(14);
