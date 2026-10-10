-- Guest senders (Milestone 3b, docs/guest-sender-design.md): the registry that gives each
-- protocol identity exactly one sender subject, mailbox verifications and guest
-- sessions, request-history links by mailbox and payment-instrument fingerprint with
-- their merges, guest payment instruments as funding sources, and the notices owed to
-- guests.
CREATE TABLE cs_sender_subjects (
 identity TEXT PRIMARY KEY,
 subject_key TEXT NOT NULL UNIQUE,
 subject JSONB NOT NULL,
 registered_at BIGINT NOT NULL CHECK (registered_at >= 0)
);
-- Member addresses enrolled before the registry existed.
INSERT INTO cs_sender_subjects(identity, subject_key, subject, registered_at)
 SELECT identity, 'member:' || account || ':' || identity,
        jsonb_build_object('Member', account::numeric), 0
 FROM cs_persona_owners;
CREATE TABLE cs_guest_verifications (
 id TEXT PRIMARY KEY,
 verification JSONB NOT NULL,
 created_at BIGINT NOT NULL CHECK (created_at >= 0)
);
CREATE INDEX cs_guest_verifications_mailbox ON cs_guest_verifications ((verification->>'mailbox'), created_at);
CREATE TABLE cs_guest_sessions (
 token_digest BYTEA PRIMARY KEY CHECK (octet_length(token_digest) = 32),
 session JSONB NOT NULL
);
CREATE TABLE cs_guest_history_links (
 recipient TEXT NOT NULL,
 evidence TEXT NOT NULL,
 history TEXT NOT NULL,
 linked_at BIGINT NOT NULL CHECK (linked_at >= 0),
 PRIMARY KEY (recipient, evidence)
);
CREATE INDEX cs_guest_history_links_history ON cs_guest_history_links (history);
CREATE TABLE cs_guest_history_merges (
 merged TEXT PRIMARY KEY,
 sources JSONB NOT NULL,
 evidence JSONB NOT NULL,
 merged_at BIGINT NOT NULL CHECK (merged_at >= 0)
);
ALTER TABLE cs_funding_sources ALTER COLUMN account DROP NOT NULL;
ALTER TABLE cs_funding_sources ADD COLUMN guest TEXT REFERENCES cs_sender_subjects(identity);
ALTER TABLE cs_funding_sources ADD COLUMN fingerprint BYTEA CHECK (octet_length(fingerprint) = 32);
ALTER TABLE cs_funding_sources ADD CONSTRAINT cs_funding_sources_one_owner
 CHECK ((account IS NULL) <> (guest IS NULL) AND (guest IS NULL) = (fingerprint IS NULL));
CREATE TABLE cs_guest_notices (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL,
 kind TEXT NOT NULL,
 notice JSONB NOT NULL,
 recorded_at BIGINT NOT NULL CHECK (recorded_at >= 0),
 PRIMARY KEY (aggregate_key, request_id, kind)
);
COMMENT ON TABLE cs_sender_subjects IS
 'Exactly one sender subject per protocol identity, and one identity per subject key.';
COMMENT ON TABLE cs_guest_history_links IS
 'Guest request-history evidence (mailbox or instrument fingerprint) per recipient; retained indefinitely by default.';
COMMENT ON TABLE cs_guest_history_merges IS
 'Append-only record of guest histories merged because evidence linked them.';
COMMENT ON TABLE cs_guest_notices IS
 'Notices owed to guests about their requests, each recorded once.';
REVOKE ALL ON cs_sender_subjects, cs_guest_verifications, cs_guest_sessions,
 cs_guest_history_links, cs_guest_history_merges, cs_guest_notices FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(23);
