-- Alias management introduces no identity, billing or signed-history rewrite.
-- Retired routes retain their unique address reservation permanently.
CREATE TABLE cs_alias_routes (
 id TEXT PRIMARY KEY,
 owner TEXT NOT NULL REFERENCES cs_persona_owners(identity),
 address TEXT NOT NULL UNIQUE,
 record JSONB NOT NULL
);
CREATE INDEX cs_alias_owner ON cs_alias_routes(owner,id);
CREATE TABLE cs_alias_commands (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 operation TEXT NOT NULL,
 request JSONB NOT NULL,
 receipt JSONB NOT NULL,
 PRIMARY KEY(account,operation)
);
CREATE TABLE cs_alias_sender_scopes (
 recipient TEXT NOT NULL REFERENCES cs_persona_owners(identity),
 sender TEXT NOT NULL,
 record JSONB NOT NULL,
 PRIMARY KEY(recipient,sender)
);
CREATE TABLE cs_alias_deliveries (
 message TEXT PRIMARY KEY,
 alias TEXT NOT NULL REFERENCES cs_alias_routes(id),
 conversation TEXT NOT NULL,
 sender TEXT NOT NULL,
 admitted_at BIGINT NOT NULL,
 evidence JSONB NOT NULL
);
REVOKE ALL ON cs_alias_routes,cs_alias_commands,cs_alias_sender_scopes,cs_alias_deliveries FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(38);
