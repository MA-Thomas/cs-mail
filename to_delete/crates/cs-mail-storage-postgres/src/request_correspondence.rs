//! A member request's messages are correspondence documents, and its acceptance grants
//! the reply lane its sender consented to. Both cross from the request's relationship
//! into another owner's records, so both commit inside the request's transition.
use super::{
    CapabilityEvent, PostgresEngine, StorageError, allocate_journal_position, consent, guests,
    ingress, insert_capability_event, load_balances, load_domain_records, load_locked_aggregate,
    persist_manifest, persist_schedules, to_i64, upsert_lane,
};
use cs_mail_application::correspondence::{
    MessageRecord, RequestConversation, RequestDocument, validate_prepared,
};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_correspondence::{
    Conversation, ConversationId, CorrespondenceError, CorrespondenceRef,
};
use cs_mail_ledger::LedgerView;
use cs_mail_primitives::{
    CanonicalTime, ContentRef, DeclarationAuthority, JournalPosition, ProtocolIdentity, RequestId,
};
use cs_mail_protocol::{
    AdmissionBasis, Message, PolicySnapshot, RelationshipRequest, RelationshipState,
    RequestLifecycle, ReverseDirection, SettlementSnapshot, TransitionContext,
};
use cs_mail_security::{SignedContentKeyCertificate, SigningScope};
use postgres::{Transaction, types::Json};

impl PostgresEngine {
    /// Stages a member request's initial message or follow-up for later admission. The
    /// sender's certified content key must have produced every copy, and the document
    /// must belong to this relationship's request and to that request's conversation.
    /// # Errors
    /// Rejects foreign or uncertified copies, an unknown request, a conversation already
    /// taken, conflicting ciphertext, a guest sender, or storage failures.
    #[allow(clippy::too_many_lines)] // Every check of one staged document, in one transaction.
    pub fn store_request_document(
        &self,
        document: &RequestDocument,
        certificate: &SignedContentKeyCertificate,
        clock: impl FnOnce() -> CanonicalTime,
        policy: &PolicySnapshot,
        deployment: [u8; 32],
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        ingress::validate_ingress_scope(&mut tx, &self.aggregate_key, deployment, policy)?;
        let aggregate = load_locked_aggregate(&mut tx, &self.aggregate_key)?;
        let relationship = aggregate.state.relationship.key;
        if guests::is_guest(&mut tx, relationship.sender)? {
            return Err(StorageError::ContentScopeMismatch);
        }
        let now = clock();
        let digest = ingress::registry_locked(&mut tx, &self.aggregate_key)?
            .verify_content_key_certificate(
                certificate,
                now,
                policy.protocol_version,
                SigningScope {
                    deployment_domain: deployment,
                    intended_provider: policy.recipient_provider,
                    relationship: relationship.reference,
                },
            )?;
        let c = &certificate.certificate;
        let package = &document.package;
        // `validate_prepared` ties both recovery copies to these bindings and sender key.
        if c.owner != relationship.sender
            || package.sender_recovery.sender != c.key
            || [&package.sender_copy, &package.recipient_copy]
                .iter()
                .any(|copy| {
                    copy.envelope.sender_key != c.key.reference
                        || copy.binding.sender_certificate != digest
                        || copy.binding.content_ref != document.content_ref
                })
        {
            return Err(StorageError::ContentScopeMismatch);
        }
        validate_prepared(
            package,
            relationship.sender,
            c.operational_key,
            relationship.recipient,
            relationship.reference,
            Some(consent::custody_key(&mut tx)?),
            now,
        )?;
        if package.recipient_copy.binding.declarations.origin.authority
            != DeclarationAuthority::NativeSender(c.operational_key)
        {
            return Err(StorageError::ContentScopeMismatch);
        }
        ingress::admission_policy(&mut tx, &self.aggregate_key)?
            .evaluate(&package.recipient_copy.binding.declarations)
            .map_err(StorageError::AdmissionRefused)?;
        let request = tx
            .query_opt(
                "SELECT request FROM cs_requests WHERE aggregate_key=$1 AND request_id=$2",
                &[&self.aggregate_key, &document.request.0.to_string()],
            )?
            .map(|row| row.try_get::<_, Json<RelationshipRequest>>(0).map(|j| j.0))
            .transpose()?
            .ok_or(CorrespondenceError::Unauthorized)?;
        let message = package.manifest.message();
        let conversation = package.manifest.conversation();
        let initial = message == request.initial_message;
        if initial {
            if !request.lifecycle.is_preparing_submission()
                || tx
                    .query_opt(
                        "SELECT 1 FROM cs_conversation_routes WHERE id=$1",
                        &[&conversation.value().to_string()],
                    )?
                    .is_some()
            {
                return Err(CorrespondenceError::Conflict.into());
            }
        } else if !request.lifecycle.is_awaiting_recipient_decision()
            || request_conversation(&mut tx, &self.aggregate_key, request.id)? != Some(conversation)
        {
            return Err(CorrespondenceError::Unauthorized.into());
        }
        if let Some(row) = tx.query_opt(
            "SELECT document FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2",
            &[&self.aggregate_key, &document.content_ref.0.to_string()],
        )? {
            return if row.try_get::<_, Json<RequestDocument>>(0)?.0 == *document {
                tx.commit()?;
                Ok(())
            } else {
                Err(StorageError::DuplicateConflict)
            };
        }
        let expires_at = package.recipient_copy.expires_at;
        let inserted = tx.execute(
            "INSERT INTO cs_request_documents \
             (aggregate_key,content_ref,request_id,message,conversation,initial,document,expires_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            &[
                &self.aggregate_key,
                &document.content_ref.0.to_string(),
                &document.request.0.to_string(),
                &message.0.to_string(),
                &conversation.value().to_string(),
                &initial,
                &Json(document),
                &to_i64(expires_at.0)?,
            ],
        )?;
        if inserted != 1 {
            // Another staged document already claims this message or conversation.
            return Err(StorageError::DuplicateConflict);
        }
        let record_ref = super::retention_record_key(
            relationship.reference.as_bytes(),
            "request-document",
            &document.content_ref.0.to_string(),
        );
        tx.execute(
            "INSERT INTO cs_retention_records \
             (record_ref, aggregate_key, record_domain, object_ref, policy_version, \
              delete_after, state, created_at) \
             VALUES ($1, $2, 'request-document', $3, $4, $5, 'active', $6)",
            &[
                &record_ref,
                &self.aggregate_key,
                &document.content_ref.0.to_string(),
                &i64::from(policy.retention_policy_version.0),
                &to_i64(expires_at.0)?,
                &to_i64(now.0)?,
            ],
        )?;
        super::members::record_certificate(&mut tx, &digest.0, certificate)?;
        tx.commit()?;
        Ok(())
    }
}

impl PostgresEngine {
    /// Whether a request document is staged under `content_ref` in this relationship.
    /// # Errors
    /// Returns database errors.
    pub fn has_request_document(&self, content_ref: ContentRef) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let staged = staged(&mut tx, &self.aggregate_key, content_ref)?;
        tx.commit()?;
        Ok(staged)
    }
}

fn request_conversation(
    tx: &mut Transaction<'_>,
    key: &str,
    request: RequestId,
) -> Result<Option<ConversationId>, StorageError> {
    tx.query_opt(
        "SELECT conversation FROM cs_request_conversations WHERE aggregate_key=$1 AND request_id=$2",
        &[&key, &request.0.to_string()],
    )?
    .map(|row| parse_conversation(&row.try_get::<_, String>(0)?))
    .transpose()
}
fn parse_conversation(value: &str) -> Result<ConversationId, StorageError> {
    Ok(ConversationId::new(
        value.parse().map_err(|_| CorrespondenceError::Invalid)?,
    )?)
}
fn parse_identity(value: &str) -> Result<ProtocolIdentity, StorageError> {
    Ok(ProtocolIdentity(
        value.parse().map_err(|_| StorageError::NumericRange)?,
    ))
}

/// Whether a staged document can supply this content reference.
pub(super) fn staged(
    tx: &mut Transaction<'_>,
    key: &str,
    content: ContentRef,
) -> Result<bool, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT 1 FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2",
            &[&key, &content.0.to_string()],
        )?
        .is_some())
}

/// The recipient's copy of a staged document, checked by admission like any content.
pub(super) fn staged_recipient_copy(
    tx: &mut Transaction<'_>,
    key: &str,
    content: ContentRef,
) -> Result<Option<EncryptedContentRecord>, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT document FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2 FOR SHARE",
            &[&key, &content.0.to_string()],
        )?
        .map(|row| row.try_get::<_, Json<RequestDocument>>(0).map(|j| j.0))
        .transpose()?
        .map(|document| document.package.recipient_copy))
}

/// Publishes an admitted request message to both mailboxes, inside the transition that
/// admitted it. The initial message begins the request's conversation, restricted to
/// the originating relationship.
/// # Errors
/// Rejects a missing or mismatched staged document; storage failures.
pub(super) fn publish(
    tx: &mut Transaction<'_>,
    key: &str,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
    message: &Message,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let (AdmissionBasis::InitialRequest { request }
    | AdmissionBasis::RequestFollowup { request, .. }) = message.basis
    else {
        return Err(StorageError::ContentScopeMismatch);
    };
    let document = tx
        .query_opt(
            "DELETE FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2 \
             RETURNING document, initial",
            &[&key, &message.content.0.to_string()],
        )?
        .ok_or(StorageError::AdmissionRefused(
            cs_mail_protocol::admission::AdmissionFailure::ContentMissing,
        ))?;
    let initial: bool = document.try_get(1)?;
    let document = document.try_get::<_, Json<RequestDocument>>(0)?.0;
    let package = document.package;
    let conversation = package.manifest.conversation();
    if document.request != request
        || package.manifest.message() != message.id
        || initial != matches!(message.basis, AdmissionBasis::InitialRequest { .. })
    {
        return Err(StorageError::ContentScopeMismatch);
    }
    let id = conversation.value().to_string();
    if initial {
        let record = Conversation::begun_by_request(
            conversation,
            CorrespondenceRef::new(sender, recipient)?,
        );
        tx.execute(
            "INSERT INTO cs_conversation_routes(id,route) VALUES($1,'native')",
            &[&id],
        )?;
        tx.execute(
            "INSERT INTO cs_conversations(id,record) VALUES($1,$2)",
            &[&id, &Json(record)],
        )?;
        tx.execute(
            "INSERT INTO cs_request_conversations(conversation,aggregate_key,request_id,sender,recipient) \
             VALUES($1,$2,$3,$4,$5)",
            &[
                &id,
                &key,
                &request.0.to_string(),
                &sender.0.to_string(),
                &recipient.0.to_string(),
            ],
        )?;
    } else if request_conversation(tx, key, request)? != Some(conversation) {
        return Err(StorageError::ContentScopeMismatch);
    }
    let message_id = message.id.0.to_string();
    let record = MessageRecord {
        manifest: package.manifest,
        author: sender,
        admission: message.basis,
        committed_at: now,
    };
    tx.execute(
        "INSERT INTO cs_correspondence_messages(id,conversation,record) VALUES($1,$2,$3)",
        &[&message_id, &id, &Json(&record)],
    )?;
    for copy in [package.sender_copy, package.recipient_copy] {
        tx.execute(
            "INSERT INTO cs_correspondence_copies(message,owner,ciphertext) VALUES($1,$2,$3)",
            &[
                &message_id,
                &copy.binding.recipient.0.to_string(),
                &Json(copy),
            ],
        )?;
    }
    for recovery in [package.sender_recovery, package.recipient_recovery] {
        tx.execute(
            "INSERT INTO cs_correspondence_recovery(message,owner,record) VALUES($1,$2,$3)",
            &[
                &message_id,
                &recovery.ciphertext.binding.recipient.0.to_string(),
                &Json(recovery),
            ],
        )?;
    }
    Ok(())
}

/// The submitted request a conversation began, for a correspondence decision.
pub(super) fn conversation_request(
    tx: &mut Transaction<'_>,
    conversation: ConversationId,
) -> Result<Option<RequestConversation>, StorageError> {
    tx.query_opt(
        "SELECT request_id,sender,recipient FROM cs_request_conversations WHERE conversation=$1",
        &[&conversation.value().to_string()],
    )?
    .map(|row| {
        Ok(RequestConversation {
            conversation,
            request: RequestId(
                row.try_get::<_, String>(0)?
                    .parse()
                    .map_err(|_| StorageError::NumericRange)?,
            ),
            sender: parse_identity(&row.try_get::<_, String>(1)?)?,
            recipient: parse_identity(&row.try_get::<_, String>(2)?)?,
        })
    })
    .transpose()
}

/// The directed relationship from `sender` to `recipient`, locked, if it exists.
fn directed_relationship(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
) -> Result<Option<String>, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT aggregate_key FROM cs_relationship_aggregates \
             WHERE relationship_state#>>'{relationship,key,sender}'=$1 \
             AND relationship_state#>>'{relationship,key,recipient}'=$2 \
             AND protocol_format_version=$3 FOR UPDATE",
            &[
                &sender.0.to_string(),
                &recipient.0.to_string(),
                &super::CURRENT_PROTOCOL_FORMAT_VERSION,
            ],
        )?
        .map(|row| row.get(0)))
}

/// The facts of the direction from a member request's recipient back to its sender:
/// the sender's standing toward the recipient and any request of the recipient's still
/// awaiting the sender. A direction never opened is `Unknown` with no request.
/// # Errors
/// Storage failures.
pub(super) fn reverse_direction(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
) -> Result<ReverseDirection, StorageError> {
    if guests::is_guest(tx, sender)? {
        return Ok(ReverseDirection::NotApplicable);
    }
    let Some(reverse) = directed_relationship(tx, recipient, sender)? else {
        return Ok(ReverseDirection::Member {
            state: RelationshipState::Unknown,
            unresolved_request: None,
        });
    };
    let state = load_locked_aggregate(tx, &reverse)?.state;
    Ok(ReverseDirection::Member {
        state: state.relationship.state,
        unresolved_request: state
            .active_request()
            .filter(|request| request.lifecycle.is_awaiting_recipient_decision())
            .map(|request| request.id),
    })
}

/// The requests this transition accepted, as they now stand.
pub(super) fn newly_accepted<'a>(
    before: &SettlementSnapshot,
    after: &'a cs_mail_protocol::TransitionManifest,
) -> Vec<&'a RelationshipRequest> {
    after
        .next_state
        .requests
        .values()
        .filter(|request| {
            matches!(request.lifecycle, RequestLifecycle::Accepted { .. })
                && !before
                    .state
                    .requests
                    .get(&request.id)
                    .is_some_and(|r| matches!(r.lifecycle, RequestLifecycle::Accepted { .. }))
        })
        .collect()
}

/// Grants the reply lane an accepted member request consented to, on the reverse
/// relationship, in the transaction committing the acceptance.
/// # Errors
/// A missing reverse relationship, an invalid grant, or storage failures.
pub(super) fn grant_reply_lane(
    tx: &mut Transaction<'_>,
    request: &RelationshipRequest,
    now: CanonicalTime,
    policy: &PolicySnapshot,
) -> Result<(), StorageError> {
    if guests::is_guest(tx, request.terms.sender)? {
        return Ok(());
    }
    let key = directed_relationship(tx, request.terms.recipient, request.terms.sender)?
        .ok_or(StorageError::ReplyRelationshipMissing)?;
    let aggregate = load_locked_aggregate(tx, &key)?;
    let ledger = LedgerView::from_balances(
        aggregate.ledger_revision,
        aggregate.unit,
        load_balances(tx, &key)?,
    );
    let (history, payments, messages) =
        load_domain_records(tx, &key, &aggregate.state, false, None)?;
    let mut snapshot = SettlementSnapshot::complete(
        aggregate.revision,
        aggregate.state,
        history,
        payments,
        messages,
        ledger,
    );
    snapshot.lane = ingress::load_current_lane(tx, &key)?;
    let context = |journal_position| TransitionContext {
        admission: Ok(()),
        now,
        journal_position,
        protocol_version: policy.protocol_version,
        policy: policy.clone(),
        pricing: cs_mail_protocol::pricing::QuotePricing::NotRequested,
        reverse: ReverseDirection::NotApplicable,
    };
    let decide = |position| {
        cs_mail_application::relationships::ReplyLane::decide(
            &snapshot,
            request,
            &context(position),
        )
        .map_err(|error| match error {
            cs_mail_application::relationships::LaneAcceptanceError::Protocol(e) => {
                StorageError::Protocol(e)
            }
            cs_mail_application::relationships::LaneAcceptanceError::Capability(e) => {
                StorageError::Capability(e)
            }
        })
    };
    // Decide before allocating the reverse relationship's journal entry; nothing is
    // recorded there when the reply direction already has its owner's choice.
    if decide(JournalPosition(0))?.is_none() {
        return Ok(());
    }
    let next_revision = snapshot
        .revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    let position = allocate_journal_position(tx, &key, next_revision, "capability", now)?;
    let (lane, manifest) = decide(position)?
        .ok_or(StorageError::VersionConflict)?
        .into_effects();
    let next_ledger = snapshot.ledger.apply(&manifest.ledger_batch)?;
    persist_manifest(tx, &key, &manifest, &next_ledger, now, position)?;
    upsert_lane(tx, &key, &lane, now)?;
    persist_schedules(tx, &key, &[lane.schedule_change()])?;
    insert_capability_event(
        tx,
        &key,
        position,
        &CapabilityEvent::LaneGranted(lane.grant().id),
    )?;
    Ok(())
}
