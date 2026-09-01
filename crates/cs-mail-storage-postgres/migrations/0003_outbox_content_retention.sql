ALTER TABLE cs_outbox ADD COLUMN IF NOT EXISTS content_ref TEXT;

CREATE INDEX IF NOT EXISTS cs_outbox_content_pending_idx
    ON cs_outbox (aggregate_key, content_ref)
    WHERE content_ref IS NOT NULL AND status IN ('pending', 'processing');

INSERT INTO cs_schema_migrations(version) VALUES (3)
ON CONFLICT (version) DO NOTHING;
