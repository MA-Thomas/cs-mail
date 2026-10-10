-- Development support only. The simulated provider's own records, kept apart from every
-- canonical cs-mail table: operations and their latest evidence, queued directives, and
-- the audit of operator scenario commands.
CREATE TABLE cs_sim_operations (
 operation_id TEXT PRIMARY KEY,
 refunds_capture TEXT,
 record JSONB NOT NULL,
 recorded_seq BIGSERIAL NOT NULL
);
CREATE INDEX cs_sim_operations_refunds ON cs_sim_operations(refunds_capture);
CREATE TABLE cs_sim_directives (
 seq BIGSERIAL PRIMARY KEY,
 directive JSONB NOT NULL,
 queued_by TEXT NOT NULL,
 consumed_by TEXT
);
CREATE TABLE cs_sim_commands (
 idempotency_key TEXT PRIMARY KEY,
 operator TEXT NOT NULL,
 digest BYTEA NOT NULL CHECK (octet_length(digest) = 32),
 command JSONB NOT NULL,
 outcome JSONB NOT NULL,
 authorized_at BIGINT NOT NULL CHECK (authorized_at >= 0)
);
REVOKE ALL ON cs_sim_operations, cs_sim_directives, cs_sim_commands FROM PUBLIC;
INSERT INTO cs_sim_migrations(version) VALUES(1);
