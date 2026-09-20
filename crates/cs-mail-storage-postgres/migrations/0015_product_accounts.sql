CREATE TABLE cs_product_accounts (
 id TEXT PRIMARY KEY,
 principal TEXT NOT NULL UNIQUE,
 billing TEXT NOT NULL UNIQUE REFERENCES cs_billing_accounts(id),
 member TEXT NOT NULL UNIQUE,
 membership_identity BYTEA NOT NULL CHECK(octet_length(membership_identity)=32),
 issuer TEXT NOT NULL, product TEXT NOT NULL, subject_ref TEXT NOT NULL,
 control JSONB NOT NULL, security_version BIGINT NOT NULL DEFAULT 1,
 binding_version BIGINT NOT NULL CHECK(binding_version=1), registry JSONB NOT NULL,
 UNIQUE(issuer,product,subject_ref)
);
CREATE TABLE cs_persona_owners (
 identity TEXT PRIMARY KEY,
 account TEXT NOT NULL REFERENCES cs_product_accounts(id)
);
CREATE TABLE cs_key_claims (
 reference TEXT PRIMARY KEY,
 claim JSONB NOT NULL
);
CREATE TABLE cs_identity_configuration (
 singleton BOOLEAN PRIMARY KEY CHECK(singleton), issuer TEXT NOT NULL, product TEXT NOT NULL,
 revision BIGINT NOT NULL DEFAULT 0, decision_keys JSONB NOT NULL
);
CREATE TABLE cs_enrollment_operations (
 operation TEXT PRIMARY KEY, pending JSONB NOT NULL,
 billing TEXT NOT NULL UNIQUE, member TEXT NOT NULL UNIQUE,
 persona TEXT NOT NULL UNIQUE, key_reference TEXT NOT NULL,
 bank_token BYTEA NOT NULL UNIQUE CHECK(octet_length(bank_token)=32),
 decision JSONB, authorized_at BIGINT,
 account TEXT UNIQUE REFERENCES cs_product_accounts(id),
 CHECK ((account IS NULL AND decision IS NULL AND authorized_at IS NULL) OR (account IS NOT NULL AND decision IS NOT NULL AND authorized_at IS NOT NULL))
);
CREATE TABLE cs_identity_outbox (
 operation TEXT PRIMARY KEY REFERENCES cs_enrollment_operations(operation),
 confirmed BOOLEAN NOT NULL DEFAULT FALSE,
 generation BIGINT NOT NULL DEFAULT 0 CHECK(generation>=0), lease_until BIGINT,
 attempts INTEGER NOT NULL DEFAULT 0, next_attempt BIGINT NOT NULL DEFAULT 0, intervention JSONB
);
REVOKE ALL ON cs_product_accounts,cs_persona_owners,cs_key_claims,cs_identity_configuration,cs_enrollment_operations,cs_identity_outbox FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(15);

CREATE TABLE cs_product_account_commands (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id), id TEXT NOT NULL,
 command JSONB NOT NULL, outcome JSONB NOT NULL, authorized_at BIGINT NOT NULL,
 PRIMARY KEY(account,id)
);
REVOKE ALL ON cs_product_account_commands FROM PUBLIC;

CREATE TABLE cs_identity_security_events (
 account TEXT NOT NULL REFERENCES cs_product_accounts(id), version BIGINT NOT NULL,
 event JSONB NOT NULL, applied_at BIGINT NOT NULL,
 PRIMARY KEY(account,version)
);
REVOKE ALL ON cs_identity_security_events FROM PUBLIC;
