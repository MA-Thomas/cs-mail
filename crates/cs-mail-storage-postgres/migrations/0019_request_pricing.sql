-- Request pricing redesign (docs/request-pricing-design.md).
-- One cs-mail-wide operator policy replaces the per-recipient menu, and each address's
-- signed class publication is kept as evidence with a current projection.
-- Existing per-recipient menus cannot be converted; fail before changing any records.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_recipient_pricing) THEN
   RAISE EXCEPTION 'request pricing redesign requires a fresh database or a reviewed data migration';
 END IF;
END $$;
DROP TABLE cs_recipient_pricing;

CREATE TABLE cs_request_pricing_policies (
 version BIGINT PRIMARY KEY CHECK (version > 0),
 policy JSONB NOT NULL,
 published_at BIGINT NOT NULL CHECK (published_at >= 0)
);
CREATE TABLE cs_request_pricing_current (
 singleton BOOLEAN PRIMARY KEY CHECK (singleton),
 version BIGINT NOT NULL REFERENCES cs_request_pricing_policies(version)
);
CREATE TABLE cs_request_class_publications (
 recipient TEXT NOT NULL,
 version BIGINT NOT NULL CHECK (version > 0),
 publication JSONB NOT NULL,
 received_at BIGINT NOT NULL CHECK (received_at >= 0),
 PRIMARY KEY (recipient, version)
);
CREATE TABLE cs_recipient_request_classes (
 recipient TEXT PRIMARY KEY,
 version BIGINT NOT NULL,
 classes JSONB NOT NULL CHECK (
  jsonb_typeof(classes->'classes') = 'array' AND jsonb_array_length(classes->'classes') <= 8
 ),
 FOREIGN KEY (recipient, version) REFERENCES cs_request_class_publications(recipient, version)
);
COMMENT ON TABLE cs_request_pricing_policies IS
 'Append-only operator pricing policy versions: processing component C and collateral bounds.';
COMMENT ON TABLE cs_request_class_publications IS
 'Append-only signed request-class publications, one per address and version (evidence).';
COMMENT ON TABLE cs_recipient_request_classes IS
 'Current publication per address; a projection of cs_request_class_publications.';
REVOKE ALL ON cs_request_pricing_policies, cs_request_pricing_current,
 cs_request_class_publications, cs_recipient_request_classes FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(19);
