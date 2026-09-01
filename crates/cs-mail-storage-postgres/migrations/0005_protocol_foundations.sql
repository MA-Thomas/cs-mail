CREATE TABLE IF NOT EXISTS cs_attempt_aggregates (
    attempt_subject BYTEA PRIMARY KEY,
    attempt_state JSONB NOT NULL,
    updated_at BIGINT NOT NULL CHECK (updated_at >= 0)
);

CREATE TABLE IF NOT EXISTS cs_canonical_journal (
    position BIGSERIAL PRIMARY KEY,
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    aggregate_revision BIGINT NOT NULL CHECK (aggregate_revision > 0),
    entry_kind TEXT NOT NULL,
    received_at BIGINT NOT NULL CHECK (received_at >= 0),
    UNIQUE (aggregate_key, aggregate_revision)
);

CREATE INDEX IF NOT EXISTS cs_canonical_journal_aggregate_idx
    ON cs_canonical_journal (aggregate_key, position);

CREATE TABLE IF NOT EXISTS cs_key_registries (
    aggregate_key TEXT PRIMARY KEY REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    registry_version BIGINT NOT NULL CHECK (registry_version >= 0),
    registry JSONB NOT NULL,
    updated_at BIGINT NOT NULL CHECK (updated_at >= 0)
);

CREATE TABLE IF NOT EXISTS cs_contact_quotes (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    quote_id TEXT NOT NULL,
    terms JSONB NOT NULL,
    provider_operational_key TEXT,
    signature BYTEA,
    issued_at BIGINT NOT NULL CHECK (issued_at >= 0),
    expires_at BIGINT NOT NULL CHECK (expires_at >= issued_at),
    PRIMARY KEY (aggregate_key, quote_id),
    CHECK ((provider_operational_key IS NULL) = (signature IS NULL))
);

CREATE INDEX IF NOT EXISTS cs_contact_quotes_expiry_idx
    ON cs_contact_quotes (expires_at);

CREATE TABLE IF NOT EXISTS cs_provider_receipts (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    receipt_id TEXT NOT NULL,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    payload JSONB NOT NULL,
    provider_operational_key TEXT NOT NULL,
    signature BYTEA NOT NULL,
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (aggregate_key, receipt_id),
    UNIQUE (aggregate_key, journal_position)
);

CREATE TABLE IF NOT EXISTS cs_retention_records (
    record_ref BYTEA PRIMARY KEY,
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    record_domain TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version >= 0),
    delete_after BIGINT NOT NULL CHECK (delete_after >= 0),
    hold_until BIGINT,
    record_key_ref TEXT,
    state TEXT NOT NULL CHECK (state IN ('active', 'deleted')),
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    UNIQUE (aggregate_key, record_domain, object_ref)
);

CREATE INDEX IF NOT EXISTS cs_retention_due_idx
    ON cs_retention_records (delete_after)
    WHERE state = 'active' AND hold_until IS NULL;

CREATE TABLE IF NOT EXISTS cs_deletion_manifests (
    id BIGSERIAL PRIMARY KEY,
    record_ref BYTEA NOT NULL,
    aggregate_key TEXT NOT NULL,
    record_domain TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL,
    deleted_at BIGINT NOT NULL CHECK (deleted_at >= 0),
    reason TEXT NOT NULL
);

ALTER TABLE cs_encrypted_content
    ADD COLUMN IF NOT EXISTS retention_policy_version BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cs_encrypted_content
    ADD COLUMN IF NOT EXISTS record_ref BYTEA;
ALTER TABLE cs_outbox
    ADD COLUMN IF NOT EXISTS delete_after BIGINT;
ALTER TABLE cs_idempotency_records
    ADD COLUMN IF NOT EXISTS delete_after BIGINT;
ALTER TABLE cs_capability_idempotency
    ADD COLUMN IF NOT EXISTS delete_after BIGINT;

UPDATE cs_encrypted_content
SET record_ref = convert_to(
    json_build_array('legacy-content', aggregate_key, content_ref)::text,
    'UTF8'
)
WHERE record_ref IS NULL;

ALTER TABLE cs_encrypted_content ALTER COLUMN record_ref SET NOT NULL;

INSERT INTO cs_retention_records (
    record_ref, aggregate_key, record_domain, object_ref, policy_version,
    delete_after, state, created_at
)
SELECT
    record_ref, aggregate_key, 'content', content_ref, retention_policy_version,
    expires_at, 'active', created_at
FROM cs_encrypted_content
ON CONFLICT (record_ref) DO NOTHING;

COMMENT ON TABLE cs_attempt_aggregates IS
    'Privacy-scoped repeated-attempt state shared across public identities for one principal-recipient subject.';
COMMENT ON TABLE cs_key_registries IS
    'Durable actor-key authority and append-only transparency projection.';
COMMENT ON TABLE cs_retention_records IS
    'Policy-versioned deletion schedule; record keys permit cryptographic erasure implementations.';

REVOKE ALL ON cs_relationship_aggregates FROM PUBLIC;
REVOKE ALL ON cs_attempt_aggregates FROM PUBLIC;
REVOKE ALL ON cs_canonical_journal FROM PUBLIC;
REVOKE ALL ON cs_ledger_accounts FROM PUBLIC;
REVOKE ALL ON cs_ledger_batches FROM PUBLIC;
REVOKE ALL ON cs_ledger_transfers FROM PUBLIC;
REVOKE ALL ON cs_protocol_events FROM PUBLIC;
REVOKE ALL ON cs_idempotency_records FROM PUBLIC;
REVOKE ALL ON cs_outbox FROM PUBLIC;
REVOKE ALL ON cs_schedules FROM PUBLIC;
REVOKE ALL ON cs_encrypted_content FROM PUBLIC;
REVOKE ALL ON cs_contact_quotes FROM PUBLIC;
REVOKE ALL ON cs_provider_receipts FROM PUBLIC;
REVOKE ALL ON cs_retention_records FROM PUBLIC;
REVOKE ALL ON cs_deletion_manifests FROM PUBLIC;

INSERT INTO cs_schema_migrations(version) VALUES (5)
ON CONFLICT (version) DO NOTHING;
