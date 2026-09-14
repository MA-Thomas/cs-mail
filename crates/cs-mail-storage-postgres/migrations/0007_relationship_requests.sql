-- Existing format-1/2 financial records retain their meaning and remain unreadable
-- by the request engine until an explicit migration is supplied. No balances are reset.
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT IF EXISTS cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CONSTRAINT cs_relationship_aggregates_protocol_format_version_check CHECK (protocol_format_version IN (1,2,3));
ALTER TABLE cs_ledger_accounts DROP CONSTRAINT IF EXISTS cs_ledger_accounts_balance_check;
ALTER TABLE cs_ledger_accounts ALTER COLUMN balance TYPE NUMERIC(39,0);
CREATE UNIQUE INDEX cs_current_directed_pair ON cs_relationship_aggregates
 ((protocol_state #>> '{relationship,key,sender}'), (protocol_state #>> '{relationship,key,recipient}'))
 WHERE protocol_format_version = 3;
CREATE TABLE cs_request_relationship_keys (
    relationship_ref BYTEA PRIMARY KEY,
    aggregate_key TEXT NOT NULL UNIQUE REFERENCES cs_relationship_aggregates(aggregate_key)
);
CREATE TABLE cs_financial_programs (
    settlement_unit BIGINT PRIMARY KEY,
    program JSONB NOT NULL,
    administration_key BYTEA,
    payment_provider_key BYTEA,
    CHECK (administration_key IS NULL OR octet_length(administration_key)=32),
    CHECK (payment_provider_key IS NULL OR octet_length(payment_provider_key)=32)
);
CREATE TABLE cs_financial_commands (
    settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit),
    operation_key TEXT NOT NULL,
    command JSONB NOT NULL,
    outcome JSONB NOT NULL,
    received_at BIGINT NOT NULL,
    PRIMARY KEY(settlement_unit,operation_key)
);
CREATE TABLE cs_financial_events (
    settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit),
    revision BIGINT NOT NULL,
    event JSONB NOT NULL,
    received_at BIGINT NOT NULL,
    PRIMARY KEY(settlement_unit,revision)
);
CREATE TABLE cs_member_payment_work (
    settlement_unit BIGINT NOT NULL REFERENCES cs_financial_programs(settlement_unit),
    allocation_id TEXT NOT NULL,
    PRIMARY KEY(settlement_unit,allocation_id)
);
REVOKE ALL ON cs_request_relationship_keys, cs_financial_programs, cs_financial_commands, cs_financial_events, cs_member_payment_work FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES (7);
