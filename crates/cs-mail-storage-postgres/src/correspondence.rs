//! A native mailbox commit is the delivery boundary. Receipts contain no ciphertext.
use super::{PostgresAccountRepository, StorageError, domain::RelationshipRecord, ingress};
use cs_mail_application::{
    accounts::{AccountClock, operations::AccountState},
    correspondence::{
        Command, Context, CorrespondenceStore, Decision, MAX_SOURCE_GRAPH, MessageRecord, Receipt,
        Response, SignedRequest,
    },
};
use cs_mail_correspondence::{Conversation, ConversationId, CorrespondenceError};
use cs_mail_primitives::{CanonicalTime, ProtocolIdentity};
use postgres::{Transaction, types::Json};
use std::collections::{BTreeMap, BTreeSet};

fn load_conversation(
    tx: &mut Transaction<'_>,
    id: ConversationId,
    records: &mut BTreeMap<ConversationId, Conversation>,
) -> Result<(), StorageError> {
    if records.contains_key(&id) {
        return Ok(());
    }
    if let Some(row) = tx.query_opt(
        "SELECT record FROM cs_conversations WHERE id=$1 FOR UPDATE",
        &[&id.value().to_string()],
    )? {
        let record = row.try_get::<_, Json<Conversation>>(0)?.0;
        if record.id() != id {
            return Err(CorrespondenceError::Invalid.into());
        }
        records.insert(id, record);
    }
    Ok(())
}

fn load_context(
    tx: &mut Transaction<'_>,
    request: &SignedRequest,
    clock: &dyn AccountClock,
) -> Result<Context, StorageError> {
    let row = tx
        .query_opt(
            "SELECT a.control,a.registry,a.product FROM cs_product_accounts a \
         JOIN cs_persona_owners p ON p.account=a.id \
         WHERE a.id=$1 AND p.identity=$2 FOR UPDATE OF a",
            &[
                &request.account.0.to_string(),
                &request.persona.0.to_string(),
            ],
        )?
        .ok_or(CorrespondenceError::Unauthorized)?;
    let authority = AccountState {
        control: row.try_get::<_, Json<_>>(0)?.0,
        registry: row.try_get::<_, Json<_>>(1)?.0,
        product: row.try_get(2)?,
    };
    cs_mail_application::correspondence::authenticate_request(request, &authority, clock.now())?;
    let prior = tx
        .query_opt(
            "SELECT receipt FROM cs_correspondence_receipts WHERE account=$1 AND operation=$2",
            &[
                &request.account.0.to_string(),
                &request.operation.0.to_string(),
            ],
        )?
        .map(|r| r.try_get::<_, Json<Receipt>>(0).map(|j| j.0))
        .transpose()?;
    let mut context = Context {
        mailbox: vec![],
        authority,
        custody_key: None,
        prior,
        contact: None,
        lane: None,
        conversations: BTreeMap::new(),
        messages: BTreeMap::new(),
        retained: BTreeMap::new(),
        fetched_copy: None,
    };
    // Exact replay requires live caller authority, but no vanished source content or new policy authority.
    if context.prior.is_some() {
        return Ok(context);
    }
    if let Command::Mailbox { after, limit } = request.command {
        load_mailbox(tx, request, &mut context, after, limit)?;
        return Ok(context);
    }
    if let Some(id) = request.command.conversation() {
        load_conversation(tx, id, &mut context.conversations)?;
    }
    load_sources(tx, request, &mut context)?;
    load_copy(tx, request, &mut context)?;
    load_contact(tx, request, &mut context)?;
    if matches!(request.command, Command::Send(_)) {
        context.custody_key = Some(super::consent::custody_key(tx)?);
    }
    Ok(context)
}

impl CorrespondenceStore for PostgresAccountRepository {
    type Error = StorageError;
    fn transact(
        &self,
        request: &SignedRequest,
        decide: impl FnOnce(Context, &dyn AccountClock) -> Result<Decision, StorageError>,
    ) -> Result<Response, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        // Existing receipt ordering also serializes account/key and contact changes.
        // A single centralized gate protects absent IDs and the bounded source graph.
        ingress::lock_receipt_order(&mut tx)?;
        let context = load_context(&mut tx, request, &*self.clock)?;
        let (response, effects) = decide(context, &*self.clock)?.into_parts();
        if let Some(effects) = effects {
            if let Some(lane) = &effects.lane {
                let key: String = tx
                    .query_one(
                        "SELECT aggregate_key FROM cs_capability_lanes WHERE lane_id=$1",
                        &[&lane.grant.id.0.to_string()],
                    )?
                    .get(0);
                super::upsert_lane(&mut tx, &key, lane, effects.receipt.authorized_at)?;
                super::persist_schedules(&mut tx, &key, &[lane.schedule_change()])?;
            }
            if let Some(conversation) = effects.conversation {
                tx.execute("INSERT INTO cs_conversations(id,record) VALUES($1,$2) ON CONFLICT(id) DO UPDATE SET record=EXCLUDED.record", &[&conversation.id().value().to_string(), &Json(conversation)])?;
            }
            if let Some((record, package)) = effects.delivery {
                let id = record.manifest.message().0.to_string();
                tx.execute("INSERT INTO cs_correspondence_messages(id,conversation,record) VALUES($1,$2,$3)", &[&id, &record.manifest.conversation().value().to_string(), &Json(&record)])?;
                for copy in [package.sender_copy, package.recipient_copy] {
                    tx.execute("INSERT INTO cs_correspondence_copies(message,owner,ciphertext) VALUES($1,$2,$3)", &[&id, &copy.binding.recipient.0.to_string(), &Json(copy)])?;
                }
                for recovery in [package.sender_recovery, package.recipient_recovery] {
                    tx.execute("INSERT INTO cs_correspondence_recovery(message,owner,record) VALUES($1,$2,$3)", &[&id,&recovery.ciphertext.binding.recipient.0.to_string(),&Json(recovery)])?;
                }
            }
            if let Some(id) = effects.delete_copies {
                tx.execute("DELETE FROM cs_correspondence_copies c USING cs_correspondence_messages m WHERE c.message=m.id AND m.conversation=$1 AND c.owner=$2", &[&id.value().to_string(), &request.persona.0.to_string()])?;
            }
            tx.execute("INSERT INTO cs_correspondence_receipts(account,operation,receipt) VALUES($1,$2,$3)", &[&request.account.0.to_string(), &request.operation.0.to_string(), &Json(effects.receipt)])?;
        }
        tx.commit()?;
        Ok(response)
    }
}

fn load_mailbox(
    tx: &mut Transaction<'_>,
    request: &SignedRequest,
    context: &mut Context,
    after: u64,
    limit: u16,
) -> Result<(), StorageError> {
    if limit == 0 || limit > 100 {
        return Err(CorrespondenceError::Invalid.into());
    }
    let after = i64::try_from(after).map_err(|_| CorrespondenceError::Invalid)?;
    for row in tx.query(
        "SELECT m.sequence,m.record,c.ciphertext->>'expires_at' FROM cs_correspondence_messages m \
             JOIN cs_correspondence_copies c ON c.message=m.id WHERE c.owner=$1 AND m.sequence>$2 \
             ORDER BY m.sequence LIMIT $3",
        &[&request.persona.0.to_string(), &after, &i64::from(limit)],
    )? {
        let sequence =
            u64::try_from(row.try_get::<_, i64>(0)?).map_err(|_| CorrespondenceError::Invalid)?;
        let record = row.try_get::<_, Json<MessageRecord>>(1)?.0;
        load_conversation(
            tx,
            record.manifest.conversation(),
            &mut context.conversations,
        )?;
        if !context
            .conversations
            .get(&record.manifest.conversation())
            .is_some_and(|c| {
                c.scope().contains(request.persona) && c.scope().contains(record.author)
            })
        {
            return Err(CorrespondenceError::Invalid.into());
        }
        let expiry = CanonicalTime(
            row.try_get::<_, String>(2)?
                .parse()
                .map_err(|_| CorrespondenceError::Invalid)?,
        );
        context.mailbox.push((sequence, record, expiry));
    }
    Ok(())
}

fn load_sources(
    tx: &mut Transaction<'_>,
    request: &SignedRequest,
    context: &mut Context,
) -> Result<(), StorageError> {
    let follow_ancestry = matches!(request.command, Command::Send(_) | Command::Assess { .. });
    let mut pending = request.command.source_messages();
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        if visited.len() > MAX_SOURCE_GRAPH {
            return Err(CorrespondenceError::Invalid.into());
        }
        let Some(row) = tx.query_opt(
            "SELECT record FROM cs_correspondence_messages WHERE id=$1",
            &[&id.0.to_string()],
        )?
        else {
            continue;
        };
        let record = row.try_get::<_, Json<MessageRecord>>(0)?.0;
        if record.manifest.message() != id {
            return Err(CorrespondenceError::Invalid.into());
        }
        load_conversation(
            tx,
            record.manifest.conversation(),
            &mut context.conversations,
        )?;
        let conversation = context
            .conversations
            .get(&record.manifest.conversation())
            .ok_or(CorrespondenceError::PolicyUnresolved)?;
        if !conversation.scope().contains(record.author) {
            return Err(CorrespondenceError::Invalid.into());
        }
        for row in tx.query(
            "SELECT owner,ciphertext->>'expires_at' FROM cs_correspondence_copies WHERE message=$1",
            &[&id.0.to_string()],
        )? {
            let owner = ProtocolIdentity(
                row.try_get::<_, String>(0)?
                    .parse()
                    .map_err(|_| CorrespondenceError::Invalid)?,
            );
            if !conversation.scope().contains(owner) {
                return Err(CorrespondenceError::Invalid.into());
            }
            let expires = CanonicalTime(
                row.try_get::<_, String>(1)?
                    .parse()
                    .map_err(|_| CorrespondenceError::Invalid)?,
            );
            context.retained.insert((id, owner), expires);
        }
        if follow_ancestry {
            pending.extend(
                record
                    .manifest
                    .sources()
                    .iter()
                    .map(|s| s.selection().message()),
            );
        }
        context.messages.insert(id, record);
    }
    Ok(())
}

fn load_copy(
    tx: &mut Transaction<'_>,
    request: &SignedRequest,
    context: &mut Context,
) -> Result<(), StorageError> {
    let fetch = match &request.command {
        Command::Fetch { message } => Some(*message),
        Command::Resolve { source } => Some(source.message()),
        _ => None,
    };
    if let Some(id) = fetch {
        context.fetched_copy = tx
            .query_opt(
                "SELECT ciphertext FROM cs_correspondence_copies WHERE message=$1 AND owner=$2",
                &[&id.0.to_string(), &request.persona.0.to_string()],
            )?
            .map(|r| r.try_get::<_, Json<_>>(0).map(|j| j.0))
            .transpose()?;
        if context
            .fetched_copy
            .as_ref()
            .is_some_and(|c| c.binding.message_id != id || c.binding.recipient != request.persona)
        {
            return Err(CorrespondenceError::Invalid.into());
        }
    }
    Ok(())
}

fn load_contact(
    tx: &mut Transaction<'_>,
    request: &SignedRequest,
    context: &mut Context,
) -> Result<(), StorageError> {
    let other = match &request.command {
        Command::Create { other, .. } => Some(*other),
        Command::Send(p) => context
            .conversations
            .get(&p.manifest.conversation())
            .and_then(|c| c.scope().other(request.persona).ok()),
        _ => None,
    };
    if let Some(other) = other {
        // A received but unprocessed block/revocation must not be overtaken by
        // this separate mailbox commit. The host drains the existing inbox then retries.
        ingress::require_drained(tx)?;
        let recipient = tx.query_opt("SELECT a.control FROM cs_persona_owners p JOIN cs_product_accounts a ON a.id=p.account WHERE p.identity=$1 FOR SHARE OF a", &[&other.0.to_string()])?;
        let recipient_active = recipient
            .map(|r| {
                r.try_get::<_, Json<cs_mail_accounts::control::AccountControl>>(0)
                    .map(|j| j.0.allows_service())
            })
            .transpose()?
            .unwrap_or(false);
        if !recipient_active {
            return Ok(());
        }
        if let Some(row) = tx.query_opt(
            "SELECT relationship_state,aggregate_key FROM cs_relationship_aggregates \
                 WHERE relationship_state#>>'{relationship,key,sender}'=$1 \
                 AND relationship_state#>>'{relationship,key,recipient}'=$2 \
                 AND protocol_format_version=$3 FOR SHARE",
            &[
                &request.persona.0.to_string(),
                &other.0.to_string(),
                &super::CURRENT_PROTOCOL_FORMAT_VERSION,
            ],
        )? {
            let key: String = row.try_get(1)?;
            context.lane = tx
                .query_opt(
                    "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1 FOR UPDATE",
                    &[&key],
                )?
                .map(|r| {
                    r.try_get::<_, Json<cs_mail_capabilities::Lane>>(0)
                        .map(|j| j.0)
                })
                .transpose()?;
            context.contact = Some(
                row.try_get::<_, Json<RelationshipRecord>>(0)?
                    .0
                    .into_state()
                    .relationship,
            );
        }
    }
    Ok(())
}
