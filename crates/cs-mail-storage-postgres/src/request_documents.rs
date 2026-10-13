//! Load protected document facts and persist the application's complete effects.
use super::{PostgresEngine, StorageError, ingress, load_locked_aggregate, to_i64};
use cs_mail_application::requests::documents::{
    MailboxDelivery, RequestConversation, SignedRequestDocument, StageContext, StageDecision,
    StageReceipt, SubmissionDocument,
};
use cs_mail_primitives::{CanonicalTime, ContentRef, RequestId};
use cs_mail_protocol::ProtocolCommand;
use postgres::{Transaction, types::Json};
pub(super) fn conversation(
    tx: &mut Transaction<'_>,
    key: &str,
    request: RequestId,
) -> Result<Option<RequestConversation>, StorageError> {
    tx.query_opt(
        "SELECT record FROM cs_request_conversations WHERE aggregate_key=$1 AND request_id=$2",
        &[&key, &request.0.to_string()],
    )?
    .map(|row| {
        row.try_get::<_, Json<RequestConversation>>(0)
            .map(|value| value.0)
    })
    .transpose()
    .map_err(Into::into)
}
pub(super) fn staged(
    tx: &mut Transaction<'_>,
    key: &str,
    content: ContentRef,
) -> Result<Option<SignedRequestDocument>, StorageError> {
    tx.query_opt("SELECT document FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2 FOR SHARE",&[&key,&content.0.to_string()])?
        .map(|row|row.try_get::<_,Json<SignedRequestDocument>>(0).map(|value|value.0)).transpose().map_err(Into::into)
}
pub(super) fn submission(
    tx: &mut Transaction<'_>,
    key: &str,
    command: &ProtocolCommand,
) -> Result<Option<SubmissionDocument>, StorageError> {
    let (ProtocolCommand::SubmitRequestToRecipient {
        content_ref,
        request_id,
        ..
    }
    | ProtocolCommand::AdmitFollowup {
        content_ref,
        request_id,
        ..
    }) = command
    else {
        return Ok(None);
    };
    let Some(document) = staged(tx, key, *content_ref)? else {
        return Ok(None);
    };
    let link = conversation(tx, key, *request_id)?;
    let policy_conversation = if let Some(link) = &link {
        tx.query_opt(
            "SELECT record FROM cs_conversations WHERE id=$1 FOR UPDATE",
            &[&link.conversation().value().to_string()],
        )?
        .map(|row| {
            row.try_get::<_, Json<cs_mail_correspondence::Conversation>>(0)
                .map(|value| value.0)
        })
        .transpose()?
    } else {
        None
    };
    Ok(Some(SubmissionDocument {
        document,
        conversation: link,
        policy_conversation,
    }))
}
impl PostgresEngine {
    /// # Errors
    /// Refuses altered signed staging and rolls back every effect on persistence failure.
    pub fn stage_request_document(
        &self,
        signed: &SignedRequestDocument,
        scope: cs_mail_security::SigningScope,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<StageReceipt, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let aggregate = load_locked_aggregate(&mut tx, &self.aggregate_key)?;
        let request = tx
            .query_opt(
                "SELECT request FROM cs_requests WHERE aggregate_key=$1 AND request_id=$2",
                &[
                    &self.aggregate_key,
                    &signed.document.request().0.to_string(),
                ],
            )?
            .map(|row| {
                row.try_get::<_, Json<cs_mail_protocol::RelationshipRequest>>(0)
                    .map(|record| record.0)
            })
            .transpose()?;
        let retention = request
            .as_ref()
            .map(|request| request.terms.retention_policy_version);
        let prior=tx.query_opt("SELECT digest,at FROM cs_request_document_receipts WHERE aggregate_key=$1 AND content_ref=$2",&[&self.aggregate_key,&signed.document.content().0.to_string()])?
            .map(|row|->Result<StageReceipt,StorageError>{Ok(StageReceipt{digest:row.try_get::<_,Vec<u8>>(0)?.try_into().map_err(|_|StorageError::NumericRange)?,at:CanonicalTime(super::to_u64(row.try_get(1)?)?)})}).transpose()?;
        let context = StageContext {
            authority: ingress::registry_locked(&mut tx, &self.aggregate_key)?,
            relationship: aggregate.state.relationship.key,
            scope,
            custody: super::consent::custody_key(&mut tx)?,
            policy: ingress::admission_policy(&mut tx, &self.aggregate_key)?,
            request,
            conversation: conversation(&mut tx, &self.aggregate_key, signed.document.request())?,
            conversation_available: tx
                .query_opt(
                    "SELECT 1 FROM cs_conversation_routes WHERE id=$1",
                    &[&signed
                        .document
                        .package()
                        .manifest
                        .conversation()
                        .value()
                        .to_string()],
                )?
                .is_none(),
            prior,
        };
        let now = clock();
        let (receipt, append) = StageDecision::decide(signed, context, now)?.into_parts();
        if let Some((signed, initial)) = append {
            let document = &signed.document;
            tx.execute("INSERT INTO cs_request_documents(aggregate_key,content_ref,request_id,message,conversation,initial,document,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[
                &self.aggregate_key,&document.content().0.to_string(),&document.request().0.to_string(),&document.package().manifest.message().0.to_string(),
                &document.package().manifest.conversation().value().to_string(),&initial,&Json(&signed),&to_i64(document.package().recipient_copy.expires_at.0)?])?;
            tx.execute("INSERT INTO cs_request_document_receipts(aggregate_key,content_ref,digest,at) VALUES($1,$2,$3,$4)",&[&self.aggregate_key,&document.content().0.to_string(),&receipt.digest.as_slice(),&to_i64(receipt.at.0)?])?;
            let reference = super::retention_record_key(
                aggregate.state.relationship.key.reference.as_bytes(),
                "request-document",
                &document.content().0.to_string(),
            );
            tx.execute("INSERT INTO cs_retention_records(record_ref,aggregate_key,record_domain,object_ref,policy_version,delete_after,state,created_at) VALUES($1,$2,'request-document',$3,$4,$5,'active',$6)",&[
                &reference,&self.aggregate_key,&document.content().0.to_string(),&i64::from(retention.ok_or(StorageError::VersionConflict)?.0),&to_i64(document.package().recipient_copy.expires_at.0)?,&to_i64(now.0)?])?;
        }
        tx.commit()?;
        Ok(receipt)
    }
}
pub(super) fn persist_delivery(
    tx: &mut Transaction<'_>,
    key: &str,
    delivery: MailboxDelivery,
) -> Result<(), StorageError> {
    let (conversation, link, record, package, content) = delivery.into_parts();
    let id = link.conversation().value().to_string();
    match conversation {
        cs_mail_application::requests::documents::ConversationWrite::Create(conversation) => {
            tx.execute(
                "INSERT INTO cs_conversation_routes(id,route) VALUES($1,'native')",
                &[&id],
            )?;
            tx.execute(
                "INSERT INTO cs_conversations(id,record) VALUES($1,$2)",
                &[&id, &Json(conversation)],
            )?;
            tx.execute("INSERT INTO cs_request_conversations(conversation,aggregate_key,request_id,record) VALUES($1,$2,$3,$4)", &[&id,&key,&link.request().request.0.to_string(),&Json(link)])?;
        }
        cs_mail_application::requests::documents::ConversationWrite::Update(conversation) => {
            if tx.execute(
                "UPDATE cs_conversations SET record=$2 WHERE id=$1",
                &[&id, &Json(conversation)],
            )? != 1
            {
                return Err(StorageError::VersionConflict);
            }
        }
    }
    super::correspondence::persist_delivery(tx, &record, &package)?;
    if tx.execute(
        "DELETE FROM cs_request_documents WHERE aggregate_key=$1 AND content_ref=$2",
        &[&key, &content.0.to_string()],
    )? != 1
    {
        return Err(StorageError::VersionConflict);
    }
    Ok(())
}
