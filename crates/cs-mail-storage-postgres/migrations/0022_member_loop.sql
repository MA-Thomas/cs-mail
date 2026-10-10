-- The core request loop between members (Milestone 2): the operator's protocol policy
-- versions, each address's published endpoint content key, and the sender content-key
-- certificates recipients need to authenticate the content they decrypt.
CREATE TABLE cs_protocol_policies (
 version BIGINT PRIMARY KEY CHECK (version > 0),
 policy JSONB NOT NULL,
 published_at BIGINT NOT NULL CHECK (published_at >= 0)
);
CREATE TABLE cs_protocol_policy_current (
 singleton BOOLEAN PRIMARY KEY CHECK (singleton),
 version BIGINT NOT NULL REFERENCES cs_protocol_policies(version)
);
CREATE TABLE cs_endpoint_key_publications (
 persona TEXT NOT NULL,
 version BIGINT NOT NULL CHECK (version > 0),
 publication JSONB NOT NULL,
 received_at BIGINT NOT NULL CHECK (received_at >= 0),
 PRIMARY KEY (persona, version)
);
CREATE TABLE cs_endpoint_keys (
 persona TEXT PRIMARY KEY,
 version BIGINT NOT NULL,
 FOREIGN KEY (persona, version) REFERENCES cs_endpoint_key_publications(persona, version)
);
CREATE TABLE cs_content_certificates (
 digest BYTEA PRIMARY KEY CHECK (octet_length(digest) = 32),
 certificate JSONB NOT NULL
);
COMMENT ON TABLE cs_protocol_policies IS
 'Append-only operator protocol policy versions: request windows, cooldowns, backoff and member-program terms.';
COMMENT ON TABLE cs_endpoint_key_publications IS
 'Append-only signed endpoint content-key publications, one per address and version (evidence).';
COMMENT ON TABLE cs_endpoint_keys IS
 'Current endpoint content key per address; a projection of cs_endpoint_key_publications.';
COMMENT ON TABLE cs_content_certificates IS
 'Verified sender content-key certificates by digest, as bound into uploaded ciphertext.';
REVOKE ALL ON cs_protocol_policies, cs_protocol_policy_current, cs_endpoint_key_publications,
 cs_endpoint_keys, cs_content_certificates FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(22);
