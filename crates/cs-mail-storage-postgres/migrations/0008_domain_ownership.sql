-- A new representation, without a legacy decoder or implicit financial conversion.
-- Older records stay intact and the application refuses their format.
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CONSTRAINT cs_relationship_aggregates_protocol_format_version_check CHECK (protocol_format_version IN (1,2,3,4));
CREATE UNIQUE INDEX cs_domain_directed_pair ON cs_relationship_aggregates
 ((protocol_state #>> '{relationship,key,sender}'), (protocol_state #>> '{relationship,key,recipient}')) WHERE protocol_format_version = 4;
CREATE TABLE cs_request_histories (
 request_history BYTEA PRIMARY KEY,
 history_state JSONB NOT NULL,
 updated_at BIGINT NOT NULL
);
CREATE TABLE cs_request_financials (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL,
 financials JSONB NOT NULL,
 PRIMARY KEY (aggregate_key,request_id)
);
CREATE TABLE cs_messages (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 message_id TEXT NOT NULL,
 message JSONB NOT NULL,
 PRIMARY KEY (aggregate_key,message_id)
);
REVOKE ALL ON cs_request_histories, cs_request_financials, cs_messages FROM PUBLIC;
ALTER TABLE cs_financial_programs ADD COLUMN program_format_version SMALLINT NOT NULL DEFAULT 1 CHECK (program_format_version IN (1,2));
INSERT INTO cs_schema_migrations(version) VALUES (8);
