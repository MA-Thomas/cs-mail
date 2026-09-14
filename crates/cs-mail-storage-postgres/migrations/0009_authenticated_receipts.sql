-- The inbox is authoritative for receipt, ordering, replay and outcome evidence.
CREATE TABLE cs_received_commands (
 position BIGINT PRIMARY KEY DEFAULT nextval('cs_canonical_journal_position_seq'),
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 idempotency_key TEXT NOT NULL,
 replay_fingerprint BYTEA NOT NULL CHECK (octet_length(replay_fingerprint)=32),
 authority JSONB NOT NULL,
 content_available BOOLEAN NOT NULL,
 digest BYTEA NOT NULL CHECK (octet_length(digest)=32),
 deployment BYTEA NOT NULL CHECK (octet_length(deployment)=32),
 received_at BIGINT NOT NULL,
 operation JSONB NOT NULL,
 policy JSONB NOT NULL,
 outcome JSONB,
 receipt JSONB,
 UNIQUE (aggregate_key, idempotency_key)
);
CREATE INDEX cs_pending_received_commands ON cs_received_commands(position) WHERE outcome IS NULL;
CREATE TABLE cs_admission_policies (
 aggregate_key TEXT PRIMARY KEY REFERENCES cs_relationship_aggregates(aggregate_key),
 policy JSONB NOT NULL
);
REVOKE ALL ON cs_received_commands, cs_admission_policies FROM PUBLIC;

ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CONSTRAINT cs_relationship_aggregates_protocol_format_version_check CHECK(protocol_format_version IN (1,2,3,4,5));
CREATE UNIQUE INDEX cs_authenticated_directed_pair ON cs_relationship_aggregates
 ((protocol_state #>> '{relationship,key,sender}'),(protocol_state #>> '{relationship,key,recipient}')) WHERE protocol_format_version=5;
ALTER TABLE cs_financial_programs DROP CONSTRAINT cs_financial_programs_program_format_version_check;
ALTER TABLE cs_financial_programs ADD CONSTRAINT cs_financial_programs_program_format_version_check CHECK(program_format_version IN (1,2,3));
CREATE TABLE cs_ingress_scopes (
 aggregate_key TEXT PRIMARY KEY REFERENCES cs_relationship_aggregates(aggregate_key),
 deployment BYTEA NOT NULL CHECK(octet_length(deployment)=32), provider TEXT NOT NULL, protocol INTEGER NOT NULL, financial_scope JSONB NOT NULL
);
REVOKE ALL ON cs_ingress_scopes FROM PUBLIC;

ALTER TABLE cs_contact_quotes ADD COLUMN provider_verifying_key BYTEA CHECK(provider_verifying_key IS NULL OR octet_length(provider_verifying_key)=32);
INSERT INTO cs_schema_migrations(version) VALUES (9);
