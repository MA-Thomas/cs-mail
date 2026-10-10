-- One carrier email cannot be attached to two local messages.
CREATE UNIQUE INDEX cs_external_provider_reference
 ON cs_external_deliveries ((state#>>'{Accepted,provider_reference}'))
 WHERE state ? 'Accepted';
CREATE TABLE cs_external_provider_events (
 id TEXT PRIMARY KEY,
 message TEXT NOT NULL REFERENCES cs_external_deliveries(message),
 record JSONB NOT NULL,
 received_at BIGINT NOT NULL CHECK (received_at >= 0),
 CHECK(record->>'message'=message),
 CHECK(record->>'id'=id)
);
CREATE INDEX cs_external_provider_event_message ON cs_external_provider_events(message);
REVOKE ALL ON cs_external_provider_events FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(26);
