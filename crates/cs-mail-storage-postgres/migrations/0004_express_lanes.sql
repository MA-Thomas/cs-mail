CREATE TABLE IF NOT EXISTS cs_capability_lanes (
    aggregate_key TEXT PRIMARY KEY REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    lane_id TEXT NOT NULL UNIQUE,
    lane JSONB NOT NULL,
    updated_at BIGINT NOT NULL CHECK (updated_at >= 0)
);

CREATE TABLE IF NOT EXISTS cs_capability_events (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    event JSONB NOT NULL,
    PRIMARY KEY (aggregate_key, journal_position, ordinal)
);

CREATE TABLE IF NOT EXISTS cs_capability_idempotency (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request JSONB NOT NULL,
    outcome JSONB NOT NULL,
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (aggregate_key, idempotency_key)
);

COMMENT ON TABLE cs_capability_lanes IS
    'Relationship-privacy-domain data: deployments should grant access only through the capability service role.';

REVOKE ALL ON cs_capability_lanes FROM PUBLIC;
REVOKE ALL ON cs_capability_events FROM PUBLIC;
REVOKE ALL ON cs_capability_idempotency FROM PUBLIC;

INSERT INTO cs_schema_migrations(version) VALUES (4)
ON CONFLICT (version) DO NOTHING;
