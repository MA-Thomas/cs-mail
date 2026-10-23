-- New contracts have one alias model. Populated predecessors require an explicit,
-- separately verified preservation upgrade; never infer policy or rewrite signatures.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_alias_routes) OR EXISTS (SELECT 1 FROM cs_address_directory)
 OR EXISTS (SELECT 1 FROM cs_request_class_publications) THEN
  RAISE EXCEPTION 'Unified aliases require a fresh database or the preservation-checked explicit upgrade';
 END IF;
END $$;
DROP INDEX cs_alias_one_current_public;
ALTER TABLE cs_alias_routes DROP CONSTRAINT cs_alias_checked_record;
ALTER TABLE cs_alias_routes ADD CONSTRAINT cs_alias_checked_record CHECK (
 record->>'id'=id AND record->>'owner'=owner AND record->>'address'=address
 AND record ? 'configuration' AND NOT record ? 'usage'
);
ALTER TABLE cs_recipient_request_classes DROP CONSTRAINT cs_recipient_request_classes_recipient_version_fkey;
ALTER TABLE cs_recipient_request_classes DROP CONSTRAINT cs_recipient_request_classes_pkey;
ALTER TABLE cs_request_class_publications DROP CONSTRAINT cs_request_class_publications_pkey;
ALTER TABLE cs_request_class_publications ADD COLUMN alias TEXT NOT NULL;
ALTER TABLE cs_request_class_publications ADD PRIMARY KEY(alias,version);
ALTER TABLE cs_recipient_request_classes ADD COLUMN alias TEXT NOT NULL;
ALTER TABLE cs_recipient_request_classes ADD PRIMARY KEY(alias);
ALTER TABLE cs_recipient_request_classes ADD FOREIGN KEY(alias,version) REFERENCES cs_request_class_publications(alias,version);
ALTER TABLE cs_address_directory DROP CONSTRAINT cs_address_directory_pkey;
ALTER TABLE cs_address_directory ADD COLUMN alias TEXT NOT NULL;
ALTER TABLE cs_address_directory ADD PRIMARY KEY(alias);
ALTER TABLE cs_directory_commands DROP CONSTRAINT cs_directory_commands_pkey;
ALTER TABLE cs_directory_commands DROP CONSTRAINT cs_directory_commands_address_fkey;
ALTER TABLE cs_directory_commands RENAME COLUMN address TO alias;
ALTER TABLE cs_directory_commands ADD FOREIGN KEY(alias) REFERENCES cs_alias_routes(id);
ALTER TABLE cs_directory_commands ADD PRIMARY KEY(alias,operation);
CREATE TABLE cs_correspondence_routes (
 owner TEXT PRIMARY KEY REFERENCES cs_persona_owners(identity),
 alias TEXT NOT NULL REFERENCES cs_alias_routes(id),
 revision BIGINT NOT NULL CHECK(revision > 0)
);
REVOKE ALL ON cs_correspondence_routes FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(44);
