DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_deployment) OR EXISTS(SELECT 1 FROM cs_relationship_aggregates)
 OR EXISTS(SELECT 1 FROM cs_product_accounts) OR EXISTS(SELECT 1 FROM cs_received_commands) THEN
  RAISE EXCEPTION 'Native request documents require a fresh environment; preserve the prior database';
 END IF;
END $$;
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN(1,2,3,4,5,6,7,8,9,10));
CREATE TABLE cs_request_documents (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 content_ref TEXT NOT NULL,request_id TEXT NOT NULL,message TEXT NOT NULL UNIQUE,
 conversation TEXT NOT NULL,initial BOOLEAN NOT NULL,document JSONB NOT NULL,
 expires_at BIGINT NOT NULL CHECK(expires_at>=0),PRIMARY KEY(aggregate_key,content_ref)
);
CREATE UNIQUE INDEX cs_request_initial_conversation ON cs_request_documents(conversation) WHERE initial;
CREATE TABLE cs_request_document_receipts (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 content_ref TEXT NOT NULL,digest BYTEA NOT NULL CHECK(octet_length(digest)=32),at BIGINT NOT NULL,
 PRIMARY KEY(aggregate_key,content_ref)
);
CREATE TABLE cs_request_conversations (
 conversation TEXT PRIMARY KEY REFERENCES cs_conversations(id),
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL,record JSONB NOT NULL,UNIQUE(aggregate_key,request_id)
);
REVOKE ALL ON cs_request_documents,cs_request_document_receipts,cs_request_conversations FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(34);
