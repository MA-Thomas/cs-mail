-- This draft format cannot infer user-authored classes or rewrite old signed terms.
-- Fail before changing existing records; deployments need a reviewed data migration.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_relationship_aggregates WHERE protocol_format_version < 9)
 OR EXISTS (SELECT 1 FROM cs_recipient_pricing WHERE preference IS NOT NULL) THEN
   RAISE EXCEPTION 'request classes require a fresh database or a reviewed format-9 data migration';
 END IF;
END $$;
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN (1,2,3,4,5,6,7,8,9));
DROP INDEX cs_directed_pair;
CREATE UNIQUE INDEX cs_directed_pair ON cs_relationship_aggregates
 ((relationship_state #>> '{relationship,key,sender}'),(relationship_state #>> '{relationship,key,recipient}')) WHERE protocol_format_version IN (8,9);
-- Coordinated format cutover: classes require recipient-authored descriptions.
-- Do not invent classes from the old scalar preference. Recipients must publish.
ALTER TABLE cs_recipient_pricing DROP COLUMN preference;
ALTER TABLE cs_recipient_pricing ADD COLUMN classes JSONB;
ALTER TABLE cs_recipient_pricing ADD CONSTRAINT cs_request_classes_limit CHECK (
    classes IS NULL OR (jsonb_typeof(classes->'classes') = 'array'
    AND jsonb_array_length(classes->'classes') <= 8)
);
INSERT INTO cs_schema_migrations(version) VALUES(18);
