CREATE TABLE cs_lifecycle_policies (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 version BIGINT NOT NULL, policy JSONB NOT NULL, PRIMARY KEY(aggregate_key,version)
);
CREATE TABLE cs_retention_changes (
 id BIGSERIAL PRIMARY KEY,record_ref BYTEA NOT NULL REFERENCES cs_retention_records(record_ref),
 at BIGINT NOT NULL, reason TEXT NOT NULL, hold_until BIGINT, delete_after BIGINT NOT NULL
);
ALTER TABLE cs_received_commands ALTER COLUMN operation DROP NOT NULL;
ALTER TABLE cs_received_commands ALTER COLUMN authority DROP NOT NULL;
ALTER TABLE cs_received_commands ALTER COLUMN policy DROP NOT NULL;
ALTER TABLE cs_work ADD COLUMN journal_position BIGINT;
CREATE INDEX cs_work_receipt ON cs_work(aggregate_key,journal_position);
ALTER TABLE cs_retention_records ADD COLUMN next_check_at BIGINT NOT NULL DEFAULT 0;
CREATE INDEX cs_retention_due ON cs_retention_records(aggregate_key,delete_after) WHERE state='active';
REVOKE ALL ON cs_lifecycle_policies,cs_retention_changes FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(12);
