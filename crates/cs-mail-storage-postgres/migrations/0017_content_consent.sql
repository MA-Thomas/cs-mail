-- Only public custody material is stored here. The host protects the private root separately.
CREATE TABLE cs_custody_configuration (
 singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton), public_key JSONB NOT NULL
);
CREATE TABLE cs_content_processors (
 key TEXT PRIMARY KEY, record JSONB NOT NULL, CHECK(record->>'key'=key)
);
CREATE TABLE cs_correspondence_recovery (
 message TEXT NOT NULL, owner TEXT NOT NULL, record JSONB NOT NULL,
 PRIMARY KEY(message,owner),
 FOREIGN KEY(message,owner) REFERENCES cs_correspondence_copies(message,owner) ON DELETE CASCADE,
 CHECK(record#>>'{ciphertext,binding,message_id}'=message),
 CHECK(record#>>'{ciphertext,binding,recipient}'=owner)
);
CREATE TABLE cs_decryption_grants (
 id TEXT PRIMARY KEY, account TEXT NOT NULL REFERENCES cs_product_accounts(id),
 persona TEXT NOT NULL REFERENCES cs_persona_owners(identity), record JSONB NOT NULL,
 CHECK(record#>>'{terms,id}'=id), CHECK(record#>>'{terms,account}'=account),
 CHECK(record#>>'{terms,persona}'=persona)
);
CREATE TABLE cs_consent_receipts (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id), operation TEXT NOT NULL,
 record JSONB NOT NULL, PRIMARY KEY(account,operation),
 CHECK(record->>'account'=account), CHECK(record->>'operation'=operation)
);
REVOKE ALL ON cs_custody_configuration,cs_content_processors,cs_correspondence_recovery,cs_decryption_grants,cs_consent_receipts FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(17);
