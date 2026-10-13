-- Claims remain owned by a protocol identity, independently of discovery visibility.
ALTER TABLE cs_address_directory ADD COLUMN mailbox TEXT UNIQUE;
ALTER TABLE cs_address_directory ADD CONSTRAINT cs_mailbox_normalized CHECK(mailbox = lower(mailbox));
INSERT INTO cs_schema_migrations(version) VALUES(32);
