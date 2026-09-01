CREATE TABLE IF NOT EXISTS cs_encrypted_content (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    content_ref TEXT NOT NULL,
    record JSONB NOT NULL,
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    expires_at BIGINT NOT NULL CHECK (expires_at > created_at),
    PRIMARY KEY (aggregate_key, content_ref)
);

CREATE INDEX IF NOT EXISTS cs_encrypted_content_expiry_idx
    ON cs_encrypted_content (expires_at);

INSERT INTO cs_schema_migrations(version) VALUES (2)
ON CONFLICT (version) DO NOTHING;
