ALTER TABLE cs_relationship_aggregates
    ADD COLUMN IF NOT EXISTS protocol_format_version SMALLINT NOT NULL DEFAULT 1;

ALTER TABLE cs_relationship_aggregates
    DROP CONSTRAINT IF EXISTS cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates
    ADD CONSTRAINT cs_relationship_aggregates_protocol_format_version_check
    CHECK (protocol_format_version IN (1, 2));

COMMENT ON COLUMN cs_relationship_aggregates.protocol_format_version IS
    'Version 2 binds message declarations, validity, and capability references. Version 1 aggregates are deliberately rejected until explicitly migrated.';

INSERT INTO cs_schema_migrations(version) VALUES (6)
ON CONFLICT (version) DO NOTHING;
