//! Explicit receipt-history cutover. Conversation owns reply evidence; relationships own withdrawal.
use super::*;
pub(super) fn repair(
    tx: &mut postgres::Transaction<'_>,
    owner: u128,
    root: &Path,
) -> Result<usize> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext('cs-mail-receipt-order'))",
        &[],
    )?;
    require(
        tx.query_one(
            "SELECT count(*) FROM cs_received_commands WHERE outcome IS NULL",
            &[],
        )?
        .get::<_, i64>(0)
            == 0,
    )?;
    let schema: i64 = tx
        .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
        .get(0);
    if schema < 36 {
        let old = tx
            .query(
                "SELECT relationship_state FROM cs_relationship_aggregates",
                &[],
            )?
            .into_iter()
            .map(|row| row.get::<_, Json<Value>>(0).0)
            .collect::<Vec<_>>();
        let migration = std::fs::read_to_string(root.parent().ok_or("missing workspace")?.parent().ok_or("missing workspace")?.join("crates/cs-mail-storage-postgres/migrations/0036_received_conversation_replies.sql"))?;
        tx.batch_execute(&format!(
            "ALTER TABLE{}",
            migration
                .split_once("ALTER TABLE")
                .ok_or("missing cutover body")?
                .1
        ))?;
        for row in tx.query("SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates ORDER BY aggregate_key FOR UPDATE", &[])? {
            let aggregate: String = row.get(0); let Json(mut state): Json<Value> = row.get(1);
            let count: i64 = tx.query_one("SELECT count(*) FROM cs_protocol_events WHERE aggregate_key=$1 AND event->>'kind' IN('RelationshipBlocked','RelationshipRevoked')", &[&aggregate])?.get(0);
            state["relationship"]["reply_epoch"] = json!(count);
            tx.execute("UPDATE cs_relationship_aggregates SET relationship_state=$2 WHERE aggregate_key=$1", &[&aggregate,&Json(state)])?;
        }
        // Historical broad-lane agreement remains withdrawn when its original offer was ended.
        for row in tx.query("SELECT c.conversation,c.record,r.request FROM cs_request_conversations c JOIN cs_requests r USING(aggregate_key,request_id)", &[])? {
            let id: String = row.get(0); let Json(mut link):Json<Value> = row.get(1); let Json(request):Json<Value> = row.get(2);
            let reverse = old.iter().find(|state|state["relationship"]["key"]["sender"]==request["terms"]["recipient"] && state["relationship"]["key"]["recipient"]==request["terms"]["sender"]);
            if request["terms"]["reply_lane"].is_null() || reverse.is_none_or(|state| state["relationship"]["answers"][request["id"].to_string()].is_null() || !state["relationship"]["answers"][request["id"].to_string()]["ended"].is_null()) { continue; }
            let epoch:Value=tx.query_one("SELECT relationship_state#>'{relationship,reply_epoch}' FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2", &[&number(&request["terms"]["recipient"])?.to_string(),&number(&request["terms"]["sender"])?.to_string()])?.get::<_,Json<Value>>(0).0;
            link["reverse_offer_epoch"] = epoch;
            tx.execute("UPDATE cs_request_conversations SET record=$2 WHERE conversation=$1", &[&id,&Json(link)])?;
        }
    }
    let mut changed = 0;
    for row in tx.query(
        "SELECT id,record FROM cs_conversations WHERE record->'scope' @> $1 ORDER BY id FOR UPDATE",
        &[&Json(json!([owner]))],
    )? {
        let id: String = row.get(0);
        let original = row.get::<_, Json<Conversation>>(1).0;
        let mut next = original.clone();
        for message in tx.query(
            "SELECT record FROM cs_correspondence_messages WHERE conversation=$1 ORDER BY sequence",
            &[&id],
        )? {
            let message = message.get::<_, Json<MessageRecord>>(0).0;
            let recipient = next.scope().other(message.author)?;
            let direction=tx.query_opt("SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2 FOR UPDATE", &[&recipient.0.to_string(),&message.author.0.to_string()])?;
            let (relationship, withdrawal) = if let Some(direction) = direction {
                let key: String = direction.get(0);
                let Json(state): Json<Value> = direction.get(1);
                let at=tx.query_one("SELECT max((event->>'at')::bigint) FROM cs_protocol_events WHERE aggregate_key=$1 AND event->>'kind' IN('RelationshipBlocked','RelationshipRevoked')", &[&key])?.get::<_,Option<i64>>(0).map(|at|CanonicalTime(u64::try_from(at).unwrap_or(u64::MAX)));
                (
                    Some(serde_json::from_value::<cs_mail_protocol::Relationship>(
                        state["relationship"].clone(),
                    )?),
                    at,
                )
            } else {
                (None, None)
            };
            next = cs_mail_application::correspondence::restore_received_conversation(
                &next,
                &message,
                relationship.as_ref(),
                withdrawal,
            )?;
        }
        if next != original {
            tx.execute(
                "UPDATE cs_conversations SET record=$2 WHERE id=$1",
                &[&id, &Json(next)],
            )?;
            tx.execute("INSERT INTO cs_development_continuity_archive(kind,record) VALUES('received-conversation-correction',$1)", &[&Json(json!({"conversation":id,"before":original,"account_persona":owner}))])?;
            changed += 1;
        }
    }
    Ok(changed)
}
