-- The old notice log was written after I/O and cannot establish whether a missing
-- row was never attempted or handed off before a crash. Do not invent that history.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_deployment)
    OR EXISTS (SELECT 1 FROM cs_guest_verifications)
    OR EXISTS (SELECT 1 FROM cs_guest_notices)
    OR EXISTS (SELECT 1 FROM cs_relationship_aggregates)
    OR EXISTS (SELECT 1 FROM cs_product_accounts) THEN
   RAISE EXCEPTION 'transactional email cutover requires a fresh development database';
 END IF;
END $$;
DROP TABLE cs_guest_notices;
ALTER TABLE cs_external_messages ADD COLUMN incoming_message_id TEXT;
CREATE TABLE cs_guest_email_bindings (
 aggregate_key TEXT PRIMARY KEY REFERENCES cs_relationship_aggregates(aggregate_key),
 binding JSONB NOT NULL
);
CREATE TABLE cs_guest_request_conversations (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL,
 conversation TEXT NOT NULL UNIQUE REFERENCES cs_external_conversations(id),
 PRIMARY KEY (aggregate_key, request_id)
);
CREATE TABLE cs_mail_notices (
 id TEXT PRIMARY KEY,
 record JSONB NOT NULL,
 state JSONB NOT NULL,
 CHECK (record->>'id' = id)
);
DROP INDEX cs_external_provider_reference;
CREATE TABLE cs_mail_provider_identities (
 kind TEXT NOT NULL CHECK(kind IN ('reference','event')),
 value TEXT NOT NULL,
 target JSONB NOT NULL,
 PRIMARY KEY(kind,value)
);
CREATE TABLE cs_mail_notice_events (
 id TEXT PRIMARY KEY,
 notice TEXT NOT NULL REFERENCES cs_mail_notices(id),
 record JSONB NOT NULL,
 received_at BIGINT NOT NULL CHECK(received_at >= 0)
);
CREATE TABLE cs_request_link_receipts (
 identity BYTEA PRIMARY KEY CHECK(octet_length(identity)=32),
 content_digest BYTEA NOT NULL CHECK(octet_length(content_digest)=32),
 notice TEXT REFERENCES cs_mail_notices(id)
);
REVOKE ALL ON cs_mail_provider_identities, cs_mail_notices, cs_mail_notice_events, cs_guest_email_bindings, cs_guest_request_conversations, cs_request_link_receipts FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(30);
