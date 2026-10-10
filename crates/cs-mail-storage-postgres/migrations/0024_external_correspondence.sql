-- One route owns each conversation ID; no duplicate mutable conversation records.
CREATE TABLE cs_conversation_routes (
 id TEXT PRIMARY KEY,
 route TEXT NOT NULL CHECK (route IN ('native','external')),
 UNIQUE(id,route)
);
INSERT INTO cs_conversation_routes(id,route) SELECT id,'native' FROM cs_conversations;
ALTER TABLE cs_conversations ADD COLUMN route TEXT GENERATED ALWAYS AS ('native'::text) STORED;
ALTER TABLE cs_conversations ADD FOREIGN KEY(id,route) REFERENCES cs_conversation_routes(id,route);
CREATE TABLE cs_external_conversations (
 id TEXT PRIMARY KEY,
 route TEXT GENERATED ALWAYS AS ('external'::text) STORED,
 FOREIGN KEY(id,route) REFERENCES cs_conversation_routes(id,route),
 member TEXT NOT NULL REFERENCES cs_persona_owners(identity),
 account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 aggregate_key TEXT NOT NULL REFERENCES cs_relationship_aggregates(aggregate_key),
 record JSONB NOT NULL,
 CHECK(record#>>'{conversation,id}'=id),
 CHECK(record->>'member'=member)
);
CREATE TABLE cs_external_messages (
 id TEXT PRIMARY KEY,
 sequence BIGSERIAL UNIQUE NOT NULL,
 conversation TEXT NOT NULL REFERENCES cs_external_conversations(id),
 record JSONB NOT NULL,
 inbound_digest BYTEA CHECK (inbound_digest IS NULL OR octet_length(inbound_digest)=32),
 CHECK(record#>>'{manifest,message}'=id),
 CHECK(record#>>'{manifest,conversation}'=conversation)
);
CREATE INDEX cs_external_mailbox ON cs_external_messages(conversation, sequence);
CREATE TABLE cs_external_deliveries (
 message TEXT PRIMARY KEY REFERENCES cs_external_messages(id),
 account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 operational_key TEXT NOT NULL,
 relay_copy JSONB NOT NULL,
 state JSONB NOT NULL
);
CREATE TABLE cs_external_receipts (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 operation TEXT NOT NULL,
 conversation TEXT NOT NULL REFERENCES cs_external_conversations(id),
 receipt JSONB NOT NULL,
 PRIMARY KEY(account, operation)
);
REVOKE ALL ON cs_conversation_routes, cs_external_conversations, cs_external_messages, cs_external_deliveries, cs_external_receipts FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(24);
