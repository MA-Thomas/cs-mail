CREATE TABLE cs_conversations (
 id TEXT PRIMARY KEY, record JSONB NOT NULL,
 CHECK(record->>'id'=id)
);
CREATE TABLE cs_correspondence_messages (
 sequence BIGSERIAL NOT NULL UNIQUE,
 id TEXT PRIMARY KEY, conversation TEXT NOT NULL REFERENCES cs_conversations(id),
 record JSONB NOT NULL,
 CHECK(record#>>'{manifest,message}'=id),
 CHECK(record#>>'{manifest,conversation}'=conversation)
);
CREATE INDEX cs_correspondence_by_conversation ON cs_correspondence_messages(conversation);
CREATE TABLE cs_correspondence_copies (
 message TEXT NOT NULL REFERENCES cs_correspondence_messages(id),
 owner TEXT NOT NULL REFERENCES cs_persona_owners(identity),
 ciphertext JSONB NOT NULL,
 PRIMARY KEY(message,owner),
 CHECK(ciphertext#>>'{binding,message_id}'=message),
 CHECK(ciphertext#>>'{binding,recipient}'=owner)
);
CREATE TABLE cs_correspondence_receipts (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 operation TEXT NOT NULL, receipt JSONB NOT NULL,
 PRIMARY KEY(account,operation)
);
REVOKE ALL ON cs_conversations,cs_correspondence_messages,cs_correspondence_copies,cs_correspondence_receipts FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(16);
