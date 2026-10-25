-- Request terms carry reply-lane terms and requests the sender's consenting key.
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN (1,2,3,4,5,6,7,8,9,10));
-- A member request's messages are correspondence documents. Before admission they are
-- staged with their relationship; the admitting transaction publishes them to both
-- mailboxes in the request's conversation and removes the staged copy.
CREATE TABLE cs_request_documents (
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 content_ref TEXT NOT NULL,
 request_id TEXT NOT NULL,
 message TEXT NOT NULL,
 conversation TEXT NOT NULL,
 initial BOOLEAN NOT NULL,
 document JSONB NOT NULL,
 expires_at BIGINT NOT NULL CHECK (expires_at >= 0),
 PRIMARY KEY (aggregate_key, content_ref),
 UNIQUE (message),
 CHECK (document->>'content_ref' = content_ref),
 CHECK (document->>'request' = request_id),
 CHECK (document#>>'{package,manifest,message}' = message),
 CHECK (document#>>'{package,manifest,conversation}' = conversation)
);
-- One staged initial message may claim a not-yet-published conversation.
CREATE UNIQUE INDEX cs_request_documents_initial_conversation
 ON cs_request_documents (conversation) WHERE initial;
-- The conversation a submitted request began, for follow-ups and the recipient's answers.
CREATE TABLE cs_request_conversations (
 conversation TEXT PRIMARY KEY REFERENCES cs_conversations(id),
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 request_id TEXT NOT NULL,
 sender TEXT NOT NULL,
 recipient TEXT NOT NULL,
 UNIQUE (aggregate_key, request_id)
);
REVOKE ALL ON cs_request_documents, cs_request_conversations FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(33);
