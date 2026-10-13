-- Public, member-authored address configuration. No enrollment evidence is copied.
CREATE TABLE cs_address_directory (
 address TEXT PRIMARY KEY REFERENCES cs_persona_owners(identity),
 entry JSONB NOT NULL,
 listed BOOLEAN NOT NULL,
 search_name TEXT NOT NULL,
 search_description TEXT NOT NULL
);
CREATE TABLE cs_directory_commands (
 address TEXT NOT NULL REFERENCES cs_persona_owners(identity),
 operation TEXT NOT NULL,
 request JSONB NOT NULL,
 receipt JSONB NOT NULL,
 PRIMARY KEY(address,operation)
);
CREATE INDEX cs_directory_search_order ON cs_address_directory(search_name,address) WHERE listed;
REVOKE ALL ON cs_address_directory,cs_directory_commands FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(31);
