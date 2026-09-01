CREATE TABLE IF NOT EXISTS cs_schema_migrations (
    version BIGINT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE IF NOT EXISTS cs_relationship_aggregates (
    aggregate_key TEXT PRIMARY KEY,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    ledger_revision BIGINT NOT NULL CHECK (ledger_revision >= 0),
    settlement_unit BIGINT NOT NULL CHECK (settlement_unit >= 0),
    protocol_state JSONB NOT NULL,
    updated_at BIGINT NOT NULL CHECK (updated_at >= 0)
);

CREATE TABLE IF NOT EXISTS cs_ledger_accounts (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    account_key TEXT NOT NULL,
    account JSONB NOT NULL,
    balance NUMERIC(20, 0) NOT NULL CHECK (balance >= 0),
    PRIMARY KEY (aggregate_key, account_key)
);

CREATE TABLE IF NOT EXISTS cs_ledger_batches (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    settlement_unit BIGINT NOT NULL CHECK (settlement_unit >= 0),
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (aggregate_key, journal_position)
);

CREATE TABLE IF NOT EXISTS cs_ledger_transfers (
    aggregate_key TEXT NOT NULL,
    journal_position BIGINT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_account JSONB NOT NULL,
    destination_account JSONB NOT NULL,
    amount NUMERIC(20, 0) NOT NULL CHECK (amount > 0),
    PRIMARY KEY (aggregate_key, journal_position, ordinal),
    FOREIGN KEY (aggregate_key, journal_position)
        REFERENCES cs_ledger_batches(aggregate_key, journal_position) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cs_protocol_events (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    event JSONB NOT NULL,
    PRIMARY KEY (aggregate_key, journal_position, ordinal)
);

CREATE TABLE IF NOT EXISTS cs_idempotency_records (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request JSONB NOT NULL,
    manifest JSONB NOT NULL,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (aggregate_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS cs_outbox (
    id BIGSERIAL PRIMARY KEY,
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    journal_position BIGINT NOT NULL CHECK (journal_position > 0),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    payload JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'published')),
    created_at BIGINT NOT NULL CHECK (created_at >= 0),
    published_at BIGINT,
    claim_until BIGINT,
    UNIQUE (aggregate_key, journal_position, ordinal)
);

CREATE INDEX IF NOT EXISTS cs_outbox_pending_idx
    ON cs_outbox (status, id) WHERE status IN ('pending', 'processing');

CREATE TABLE IF NOT EXISTS cs_schedules (
    aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key) ON DELETE CASCADE,
    task_key TEXT NOT NULL,
    task JSONB NOT NULL,
    due_at BIGINT NOT NULL CHECK (due_at >= 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing')),
    claim_until BIGINT,
    PRIMARY KEY (aggregate_key, task_key)
);

CREATE INDEX IF NOT EXISTS cs_schedules_due_idx
    ON cs_schedules (status, due_at) WHERE status = 'pending';

ALTER TABLE cs_outbox ADD COLUMN IF NOT EXISTS claim_until BIGINT;
ALTER TABLE cs_schedules ADD COLUMN IF NOT EXISTS claim_until BIGINT;

INSERT INTO cs_schema_migrations(version) VALUES (1)
ON CONFLICT (version) DO NOTHING;
