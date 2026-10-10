//! Atomic notice-attempt reservation and provider evidence. No plaintext code or
//! email payload enters these tables and no network operation occurs under locks.
use super::{
    Json, PostgresAccountRepository, PostgresDeployment, StorageError, Transaction, ingress, to_i64,
};
use cs_mail_application::notices::{NoticeRecord, reserve};
use cs_mail_correspondence::{
    CorrespondenceError,
    external::delivery::{DeliveryState, ProviderReference},
};
use cs_mail_guests::NoticeId;
use cs_mail_primitives::CanonicalTime;

/// One namespace across authored mail and transactional notices. The distinct
/// variants prevent a numerical notice ID from aliasing an authored message.
#[derive(serde::Serialize)]
pub(super) enum MailTarget {
    Message(cs_mail_primitives::MessageId),
    Notice(NoticeId),
}
pub(super) fn claim_provider_identity(
    tx: &mut Transaction<'_>,
    kind: &str,
    value: &str,
    target: MailTarget,
) -> Result<(), StorageError> {
    let target = serde_json::to_value(target).map_err(|_| CorrespondenceError::Invalid)?;
    tx.execute("INSERT INTO cs_mail_provider_identities(kind,value,target) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&kind, &value, &Json(&target)])?;
    let row = tx.query_one(
        "SELECT target FROM cs_mail_provider_identities WHERE kind=$1 AND value=$2",
        &[&kind, &value],
    )?;
    if row.try_get::<_, Json<serde_json::Value>>(0)?.0 != target {
        return Err(CorrespondenceError::Conflict.into());
    }
    Ok(())
}

pub(super) fn reserve_in(
    tx: &mut Transaction<'_>,
    record: &NoticeRecord,
    now: CanonicalTime,
) -> Result<bool, StorageError> {
    let prior = tx
        .query_opt(
            "SELECT record FROM cs_mail_notices WHERE id=$1 FOR UPDATE",
            &[&record.id.value().to_string()],
        )?
        .map(|r| r.try_get::<_, Json<NoticeRecord>>(0).map(|j| j.0))
        .transpose()?;
    let Some(state) = reserve(record, prior.as_ref(), now)? else {
        return Ok(false);
    };
    tx.execute(
        "INSERT INTO cs_mail_notices(id,record,state) VALUES($1,$2,$3)",
        &[&record.id.value().to_string(), &Json(record), &Json(state)],
    )?;
    Ok(true)
}

impl PostgresAccountRepository {
    /// # Errors
    /// Refuses altered replays, destination mismatch and evidence about unattempted mail.
    pub fn notice_feedback(
        &self,
        event: &cs_mail_correspondence::external::delivery::NoticeProviderEvent,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        if let Some(row) = tx.query_opt(
            "SELECT record FROM cs_mail_notice_events WHERE id=$1",
            &[&event.id.as_str()],
        )? {
            return if row.try_get::<_, Json<cs_mail_correspondence::external::delivery::NoticeProviderEvent>>(0)?.0 == *event {
                Ok(())
            } else { Err(CorrespondenceError::Conflict.into()) };
        }
        let row = tx
            .query_opt(
                "SELECT record,state FROM cs_mail_notices WHERE id=$1 FOR UPDATE",
                &[&event.notice.value().to_string()],
            )?
            .ok_or(CorrespondenceError::Unavailable)?;
        let record = row.try_get::<_, Json<NoticeRecord>>(0)?.0;
        if record.recipient != event.recipient || record.id != event.notice {
            return Err(CorrespondenceError::Unauthorized.into());
        }
        claim_provider_identity(
            &mut tx,
            "event",
            event.id.as_str(),
            MailTarget::Notice(event.notice),
        )?;
        claim_provider_identity(
            &mut tx,
            "reference",
            event.reference.as_str(),
            MailTarget::Notice(event.notice),
        )?;
        let now = self.clock.now();
        let state = row
            .try_get::<_, Json<DeliveryState>>(1)?
            .0
            .finish(Some(event.reference.clone()), now)?;
        tx.execute(
            "UPDATE cs_mail_notices SET state=$2 WHERE id=$1",
            &[&event.notice.value().to_string(), &Json(state)],
        )?;
        tx.execute(
            "INSERT INTO cs_mail_notice_events(id,notice,record,received_at) VALUES($1,$2,$3,$4)",
            &[
                &event.id.as_str(),
                &event.notice.value().to_string(),
                &Json(event),
                &to_i64(now.0)?,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}
impl PostgresDeployment {
    /// # Errors
    /// Invalid bound or storage failure. Dispatch rechecks the candidate's state.
    pub fn waiting_notices(&self, limit: i64) -> Result<Vec<NoticeRecord>, StorageError> {
        if !(1..=1000).contains(&limit) {
            return Err(CorrespondenceError::Invalid.into());
        }
        self.client.lock().map_err(|_| StorageError::LockPoisoned)?
            .query("SELECT record FROM cs_mail_notices WHERE state=to_jsonb('Waiting'::text) ORDER BY id LIMIT $1", &[&limit])?
            .into_iter().map(|r| r.try_get::<_, Json<NoticeRecord>>(0).map(|j| j.0).map_err(Into::into)).collect()
    }
    /// # Errors
    /// Storage failure. A competing worker or old candidate obtains no attempt.
    pub fn begin_queued_notice(
        &self,
        id: NoticeId,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<Option<NoticeRecord>, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        let row = tx.query_one(
            "SELECT record,state FROM cs_mail_notices WHERE id=$1 FOR UPDATE",
            &[&id.value().to_string()],
        )?;
        let record = row.try_get::<_, Json<NoticeRecord>>(0)?.0;
        let state = row.try_get::<_, Json<DeliveryState>>(1)?.0;
        if state != DeliveryState::Waiting {
            return Ok(None);
        }
        let (allowed, now) = super::external::request_link_still_allowed(&mut tx, &record, clock)?;
        let next = if allowed {
            state.begin(now)?
        } else {
            state.cancel(now)
        };
        tx.execute(
            "UPDATE cs_mail_notices SET state=$2 WHERE id=$1",
            &[&id.value().to_string(), &Json(next)],
        )?;
        tx.commit()?;
        Ok(allowed.then_some(record))
    }
    /// Reserve one complete attempt before releasing mail to a carrier. Interrupted
    /// attempts remain uncertain; a worker never obtains a second dispatch authority.
    /// # Errors
    /// Conflicting identity or persistence failure.
    pub fn reserve_notice(
        &self,
        record: &NoticeRecord,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<bool, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        let reserved = reserve_in(&mut tx, record, clock())?;
        if reserved {
            super::external::paid_conversation_in(&mut tx, record)?;
        }
        tx.commit()?;
        Ok(reserved)
    }
    /// # Errors
    /// Conflicting terminal outcomes or unavailable notice.
    pub fn finish_notice(
        &self,
        id: NoticeId,
        reference: Option<ProviderReference>,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let row = tx
            .query_opt(
                "SELECT state FROM cs_mail_notices WHERE id=$1 FOR UPDATE",
                &[&id.value().to_string()],
            )?
            .ok_or(CorrespondenceError::Unavailable)?;
        if let Some(reference) = &reference {
            claim_provider_identity(
                &mut tx,
                "reference",
                reference.as_str(),
                MailTarget::Notice(id),
            )?;
        }
        let next = row
            .try_get::<_, Json<DeliveryState>>(0)?
            .0
            .finish(reference, now)?;
        tx.execute(
            "UPDATE cs_mail_notices SET state=$2 WHERE id=$1",
            &[&id.value().to_string(), &Json(next)],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// # Errors
    /// Storage failure or malformed durable state.
    pub fn notice_state(&self, id: NoticeId) -> Result<Option<DeliveryState>, StorageError> {
        self.client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?
            .query_opt(
                "SELECT state FROM cs_mail_notices WHERE id=$1",
                &[&id.value().to_string()],
            )?
            .map(|r| r.try_get::<_, Json<DeliveryState>>(0).map(|j| j.0))
            .transpose()
            .map_err(Into::into)
    }
}
