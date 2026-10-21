-- Runtime has one representation. Populated predecessor correspondence must be
-- copied and explicitly assigned before this schema is adopted.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_correspondence_messages)
 OR EXISTS(SELECT 1 FROM cs_external_messages)
 OR EXISTS(SELECT 1 FROM cs_alias_deliveries) THEN
  RAISE EXCEPTION 'Receiving inboxes require an explicit preservation-checked cutover';
 END IF;
END $$;
ALTER TABLE cs_alias_routes ADD CONSTRAINT cs_alias_identity_owner UNIQUE(id,owner);
ALTER TABLE cs_conversations
 ADD COLUMN participant_first TEXT GENERATED ALWAYS AS(record#>>'{scope,0}') STORED NOT NULL,
 ADD COLUMN participant_second TEXT GENERATED ALWAYS AS(record#>>'{scope,1}') STORED NOT NULL,
 ADD CONSTRAINT cs_conversation_participants UNIQUE(id,participant_first,participant_second);
ALTER TABLE cs_correspondence_messages
 ADD COLUMN participant_first TEXT GENERATED ALWAYS AS(least((record->>'author')::numeric,(record#>>'{receiving,recipient}')::numeric)::text) STORED NOT NULL,
 ADD COLUMN participant_second TEXT GENERATED ALWAYS AS(greatest((record->>'author')::numeric,(record#>>'{receiving,recipient}')::numeric)::text) STORED NOT NULL,
 ADD CONSTRAINT cs_native_participants FOREIGN KEY(conversation,participant_first,participant_second) REFERENCES cs_conversations(id,participant_first,participant_second),
 ADD COLUMN receiving_alias TEXT GENERATED ALWAYS AS(record#>>'{receiving,alias}') STORED NOT NULL,
 ADD COLUMN receiving_owner TEXT GENERATED ALWAYS AS(record#>>'{receiving,recipient}') STORED NOT NULL,
 ADD CONSTRAINT cs_native_receiving_owner FOREIGN KEY(receiving_alias,receiving_owner) REFERENCES cs_alias_routes(id,owner),
 ADD CONSTRAINT cs_native_receiving_binding CHECK(
   record#>>'{receiving,transport}' IS NOT DISTINCT FROM 'Native'
   AND record#>>'{receiving,message}' IS NOT DISTINCT FROM id
   AND record#>>'{receiving,conversation}' IS NOT DISTINCT FROM conversation
   AND record#>>'{receiving,sender}' IS NOT DISTINCT FROM record->>'author'
   AND record#>>'{receiving,at}' IS NOT DISTINCT FROM record->>'committed_at'
   AND receiving_owner <> record->>'author');
ALTER TABLE cs_external_conversations
 ADD CONSTRAINT cs_external_member UNIQUE(id,member),
 ADD COLUMN receiving_alias TEXT GENERATED ALWAYS AS(record->>'receiving_alias') STORED NOT NULL,
 ADD CONSTRAINT cs_external_conversation_inbox FOREIGN KEY(receiving_alias,member) REFERENCES cs_alias_routes(id,owner);
ALTER TABLE cs_external_messages
 ADD COLUMN copy_owner TEXT GENERATED ALWAYS AS(record#>>'{member_copy,binding,recipient}') STORED NOT NULL,
 ADD CONSTRAINT cs_external_copy_owner FOREIGN KEY(conversation,copy_owner) REFERENCES cs_external_conversations(id,member),
 ADD COLUMN receiving_alias TEXT GENERATED ALWAYS AS(record#>>'{direction,GuestToMember,alias}') STORED,
 ADD COLUMN receiving_owner TEXT GENERATED ALWAYS AS(record#>>'{direction,GuestToMember,recipient}') STORED,
 ADD CONSTRAINT cs_external_receiving_owner FOREIGN KEY(receiving_alias,receiving_owner) REFERENCES cs_alias_routes(id,owner),
 ADD CONSTRAINT cs_external_receiving_binding CHECK(
   (record->'direction' IS NOT DISTINCT FROM '"MemberToGuest"'::jsonb AND receiving_alias IS NULL AND receiving_owner IS NULL
       AND record#>>'{member_copy,binding,sender}' IS NOT DISTINCT FROM copy_owner)
   OR (record#>>'{direction,GuestToMember,transport}' IS NOT DISTINCT FROM 'External'
       AND receiving_alias IS NOT NULL AND receiving_owner IS NOT NULL
       AND record#>>'{direction,GuestToMember,message}' IS NOT DISTINCT FROM id
       AND record#>>'{direction,GuestToMember,conversation}' IS NOT DISTINCT FROM conversation
       AND record#>>'{direction,GuestToMember,sender}' IS NOT DISTINCT FROM record#>>'{member_copy,binding,sender}'
       AND receiving_owner IS NOT DISTINCT FROM record#>>'{member_copy,binding,recipient}'
       AND record#>>'{direction,GuestToMember,at}' IS NOT DISTINCT FROM record->>'committed_at'));
DROP TABLE cs_alias_deliveries;
CREATE VIEW cs_alias_deliveries AS
 SELECT id AS message,'native'::text AS transport,receiving_alias AS alias,conversation,
        record->>'author' AS sender,(record->>'committed_at')::bigint AS admitted_at,
        record->'receiving' AS evidence FROM cs_correspondence_messages
 UNION ALL
 SELECT id,'external',receiving_alias,conversation,record#>>'{member_copy,binding,sender}',
        (record->>'committed_at')::bigint,record#>'{direction,GuestToMember}'
 FROM cs_external_messages WHERE receiving_alias IS NOT NULL;
REVOKE ALL ON cs_alias_deliveries FROM PUBLIC;
INSERT INTO cs_schema_migrations(version) VALUES(43);
