DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM cs_deployment) THEN
  RAISE EXCEPTION 'Received-conversation reply cutover requires a fresh environment or the explicit preservation-checked continuity upgrade';
 END IF;
END $$;
ALTER TABLE cs_relationship_aggregates DROP CONSTRAINT cs_relationship_aggregates_protocol_format_version_check;
ALTER TABLE cs_relationship_aggregates ADD CHECK(protocol_format_version IN(1,2,3,4,5,6,7,8,9,10,11));
UPDATE cs_relationship_aggregates SET relationship_state=jsonb_set(relationship_state,'{relationship}',(relationship_state->'relationship')-'answers'||jsonb_build_object('reply_epoch',0)),protocol_format_version=11 WHERE protocol_format_version=10;
UPDATE cs_conversations SET record=record||jsonb_build_object('received','{}'::jsonb);
UPDATE cs_request_conversations SET record=record||jsonb_build_object('reverse_offer_epoch',NULL);
UPDATE cs_correspondence_receipts SET receipt=jsonb_set(receipt,'{outcome,Conversation}',receipt#>'{outcome,Conversation}'||jsonb_build_object('received','{}'::jsonb)) WHERE receipt#>'{outcome,Conversation}' IS NOT NULL;
INSERT INTO cs_schema_migrations(version) VALUES(36);
