-- The CSQD deployment record, created once by bootstrap together with its payment and
-- identity trust, and the audit log of authenticated operator commands.
CREATE TABLE cs_deployment (
 singleton BOOLEAN PRIMARY KEY CHECK (singleton),
 record JSONB NOT NULL,
 bootstrapped_at BIGINT NOT NULL CHECK (bootstrapped_at >= 0)
);
CREATE TABLE cs_operator_commands (
 family TEXT NOT NULL,
 idempotency_key TEXT NOT NULL,
 operator TEXT NOT NULL,
 digest BYTEA NOT NULL CHECK (octet_length(digest) = 32),
 command JSONB NOT NULL,
 outcome JSONB NOT NULL,
 authorized_at BIGINT NOT NULL CHECK (authorized_at >= 0),
 PRIMARY KEY (family, idempotency_key)
);
COMMENT ON TABLE cs_deployment IS
 'The one deployment: provider, domain, scope, provider key, operator keys and trust roots.';
COMMENT ON TABLE cs_operator_commands IS
 'Append-only audit of verified operator commands; replays return the recorded outcome.';
REVOKE ALL ON cs_deployment, cs_operator_commands FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(21);
