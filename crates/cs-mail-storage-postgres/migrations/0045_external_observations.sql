-- A commit-ordered clock for external message observations, including later feedback.
CREATE TABLE cs_external_observation_clock (
 singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton),
 revision BIGINT NOT NULL CHECK(revision>=0)
);
INSERT INTO cs_external_observation_clock(singleton,revision)
 SELECT TRUE,COALESCE(max(sequence),0) FROM cs_external_messages;
CREATE TABLE cs_external_observations (
 message TEXT PRIMARY KEY REFERENCES cs_external_messages(id),
 conversation TEXT NOT NULL REFERENCES cs_external_conversations(id),
 revision BIGINT NOT NULL UNIQUE CHECK(revision>0)
);
INSERT INTO cs_external_observations(message,conversation,revision)
 SELECT id,conversation,sequence FROM cs_external_messages;
CREATE INDEX cs_external_observation_page ON cs_external_observations(conversation,revision);
REVOKE ALL ON cs_external_observation_clock,cs_external_observations FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(45);
