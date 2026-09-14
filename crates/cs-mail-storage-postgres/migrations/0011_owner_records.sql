ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN (1,2,3,4,5,6));
DROP INDEX cs_current_directed_pair, cs_domain_directed_pair, cs_authenticated_directed_pair;
ALTER TABLE cs_relationship_aggregates RENAME COLUMN protocol_state TO relationship_state;
CREATE UNIQUE INDEX cs_directed_pair ON cs_relationship_aggregates
 ((relationship_state #>> '{relationship,key,sender}'),(relationship_state #>> '{relationship,key,recipient}')) WHERE protocol_format_version=6;
CREATE TABLE cs_requests (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL, request JSONB NOT NULL, closed_at BIGINT,
 PRIMARY KEY(aggregate_key,request_id)
);
ALTER TABLE cs_contact_quotes ADD COLUMN used BOOLEAN NOT NULL DEFAULT FALSE;
CREATE TABLE cs_quote_tombstones (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key), quote_id TEXT NOT NULL,
 PRIMARY KEY(aggregate_key,quote_id)
);
ALTER TABLE cs_financial_programs DROP COLUMN program;
ALTER TABLE cs_financial_programs DROP CONSTRAINT cs_financial_programs_program_format_version_check;
ALTER TABLE cs_financial_programs ADD CHECK(program_format_version IN(1,2,3,4));
ALTER TABLE cs_financial_programs ADD COLUMN metadata JSONB NOT NULL;
ALTER TABLE cs_financial_programs ADD COLUMN ledger_revision BIGINT NOT NULL DEFAULT 0;
CREATE TABLE cs_program_members (settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit), id TEXT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(settlement_unit,id));
CREATE UNIQUE INDEX cs_unique_member_identity ON cs_program_members(settlement_unit,(record->'identity_digest'));
CREATE TABLE cs_program_lots (settlement_unit BIGINT NOT NULL, id TEXT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(settlement_unit,id));
CREATE TABLE cs_program_schedules (settlement_unit BIGINT NOT NULL, id TEXT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(settlement_unit,id));
CREATE TABLE cs_program_quarters (settlement_unit BIGINT NOT NULL, id TEXT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(settlement_unit,id));
CREATE TABLE cs_program_payables (settlement_unit BIGINT NOT NULL, id TEXT NOT NULL, record JSONB NOT NULL, PRIMARY KEY(settlement_unit,id));
-- LIKE does not copy foreign keys. Each owner remains bound to its program.
ALTER TABLE cs_program_lots ADD FOREIGN KEY(settlement_unit) REFERENCES cs_financial_programs(settlement_unit);
ALTER TABLE cs_program_schedules ADD FOREIGN KEY(settlement_unit) REFERENCES cs_financial_programs(settlement_unit);
ALTER TABLE cs_program_quarters ADD FOREIGN KEY(settlement_unit) REFERENCES cs_financial_programs(settlement_unit);
ALTER TABLE cs_program_payables ADD FOREIGN KEY(settlement_unit) REFERENCES cs_financial_programs(settlement_unit);
-- Drop copied identity indexes: uniqueness of identity applies only to membership.

CREATE TABLE cs_program_accounts (
 settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit), account_key TEXT NOT NULL,
 account JSONB NOT NULL,balance NUMERIC(39,0) NOT NULL, PRIMARY KEY(settlement_unit,account_key)
);
CREATE TABLE cs_program_journal (
 settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit), revision BIGINT NOT NULL,
 ordinal INTEGER NOT NULL, entry JSONB NOT NULL, PRIMARY KEY(settlement_unit,revision,ordinal)
);
DROP TABLE cs_idempotency_records, cs_capability_idempotency, cs_attempt_aggregates;
REVOKE ALL ON cs_requests,cs_quote_tombstones,cs_program_members,cs_program_lots,cs_program_schedules,cs_program_quarters,cs_program_payables,cs_program_accounts,cs_program_journal FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(11);
