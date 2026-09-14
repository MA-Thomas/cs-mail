-- Breaking refactor. Never reinterpret or discard an existing financial database.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM cs_relationship_aggregates) OR EXISTS (SELECT 1 FROM cs_financial_programs) THEN
  RAISE EXCEPTION 'Stage 3/4 requires an explicit offline migration of existing data';
 END IF;
END $$;
DROP TABLE cs_outbox, cs_member_payment_work;
CREATE SEQUENCE cs_work_claim_token;
CREATE TABLE cs_work (
 id BIGSERIAL PRIMARY KEY,
 owner TEXT NOT NULL,
 aggregate_key TEXT REFERENCES cs_relationship_aggregates(aggregate_key),
 work_key TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('delivery','request-payment','member-payment','artifacts')),
 payload JSONB NOT NULL,
 content_ref TEXT,
 status TEXT NOT NULL CHECK(status IN ('ready','running','complete','blocked')),
 available_at BIGINT NOT NULL,
 claim_until BIGINT,
 claim_token BIGINT,
 attempts INTEGER NOT NULL DEFAULT 0,
 failure TEXT,
 completed_at BIGINT,
 created_at BIGINT NOT NULL,
 UNIQUE(owner,work_key)
);
CREATE INDEX cs_work_ready ON cs_work(owner,kind,available_at,id) WHERE status IN ('ready','running');
ALTER TABLE cs_schedules ADD COLUMN claim_token BIGINT;
REVOKE ALL ON cs_work, cs_work_claim_token FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(10);
