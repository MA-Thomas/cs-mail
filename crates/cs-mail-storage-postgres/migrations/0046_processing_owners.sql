-- Owner scopes are conservative: both participants and their complete product accounts.
ALTER TABLE cs_received_commands ADD COLUMN ordering_scopes TEXT[];
UPDATE cs_received_commands c SET ordering_scopes=ARRAY(
 SELECT DISTINCT scope FROM cs_relationship_aggregates r
 CROSS JOIN LATERAL (VALUES(r.relationship_state#>>'{relationship,key,sender}'),(r.relationship_state#>>'{relationship,key,recipient}')) p(identity)
 CROSS JOIN LATERAL (SELECT 'persona:'||p.identity AS scope UNION ALL SELECT 'account:'||o.account FROM cs_persona_owners o WHERE o.identity=p.identity) s
 WHERE r.aggregate_key=c.aggregate_key ORDER BY scope
);
ALTER TABLE cs_received_commands ALTER COLUMN ordering_scopes SET NOT NULL;
ALTER TABLE cs_received_commands ADD CHECK(cardinality(ordering_scopes)>0);
ALTER TABLE cs_received_commands ADD CHECK(outcome IS NULL OR processing_failure IS NULL);
CREATE INDEX cs_pending_command_owners ON cs_received_commands USING gin(ordering_scopes) WHERE outcome IS NULL;
INSERT INTO cs_schema_migrations(version) VALUES(46);
