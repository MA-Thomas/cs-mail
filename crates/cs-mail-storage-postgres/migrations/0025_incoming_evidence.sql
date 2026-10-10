ALTER TABLE cs_external_messages ADD COLUMN inbound_evidence JSONB;
INSERT INTO cs_schema_migrations(version) VALUES(25);
