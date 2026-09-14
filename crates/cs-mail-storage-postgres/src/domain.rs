//! Independent domain owners assembled under the relationship/history transaction locks.
use super::*;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RelationshipRecord {
    relationship: cs_mail_protocol::Relationship,
    followup_policy: cs_mail_protocol::FollowupPolicy,
}
impl From<&ProtocolState> for RelationshipRecord {
    fn from(state: &ProtocolState) -> Self {
        Self {
            relationship: state.relationship.clone(),
            followup_policy: state.followup_policy,
        }
    }
}
impl RelationshipRecord {
    pub(super) fn into_state(self) -> ProtocolState {
        ProtocolState {
            relationship: self.relationship,
            followup_policy: self.followup_policy,
            quotes: BTreeMap::new(),
            used_quotes: std::collections::BTreeSet::default(),
            requests: BTreeMap::new(),
        }
    }
}
/// The normal transition snapshot contains active requests; explicit inspection may load history.
pub(super) fn load_request_records(
    tx: &mut Transaction<'_>,
    key: &str,
    state: &mut ProtocolState,
    all: bool,
    target: Option<RequestId>,
) -> Result<(), StorageError> {
    for row in tx.query("SELECT request FROM cs_requests WHERE aggregate_key=$1 AND ($2 OR closed_at IS NULL OR request_id=$3)",&[&key,&all,&target.map(|id|id.0.to_string())])? {
        let request=row.get::<_,Json<cs_mail_protocol::RelationshipRequest>>(0).0;state.requests.insert(request.id,request);
    }
    for row in tx.query(
        "SELECT terms,used FROM cs_contact_quotes WHERE aggregate_key=$1",
        &[&key],
    )? {
        let terms = row.get::<_, Json<cs_mail_protocol::RequestTerms>>(0).0;
        if row.get::<_, bool>(1) {
            state.used_quotes.insert(terms.quote_id);
        }
        state.quotes.insert(terms.quote_id, terms);
    }
    for row in tx.query(
        "SELECT quote_id FROM cs_quote_tombstones WHERE aggregate_key=$1",
        &[&key],
    )? {
        state.used_quotes.insert(cs_mail_primitives::QuoteId(
            row.get::<_, String>(0)
                .parse()
                .map_err(|_| StorageError::NumericRange)?,
        ));
    }
    Ok(())
}

pub(super) type PaymentRecords = BTreeMap<RequestId, cs_mail_finance::RequestFinancials>;
pub(super) type MessageRecords = BTreeMap<MessageId, Message>;
pub(super) fn load_domain_records(
    tx: &mut Transaction<'_>,
    key: &str,
    state: &ProtocolState,
    all: bool,
    target: Option<RequestId>,
) -> Result<(RequestHistory, PaymentRecords, MessageRecords), StorageError> {
    let reference = scoped_key(
        state.relationship.history.derivation_version(),
        state.relationship.history.as_bytes(),
    );
    let history = tx
        .query_one(
            "SELECT history_state FROM cs_request_histories WHERE request_history=$1",
            &[&reference],
        )?
        .get::<_, Json<RequestHistory>>(0)
        .0;
    let mut ids: Vec<_> = state.requests.keys().map(|id| id.0.to_string()).collect();
    if let Some(id) = target {
        ids.push(id.0.to_string());
    }
    let mut payments = BTreeMap::new();
    for row in tx.query(
        "SELECT request_id, financials FROM cs_request_financials WHERE aggregate_key=$1 AND ($2 OR request_id=ANY($3))",
        &[&key,&all,&ids],
    )? {
        let id: String = row.get(0);
        payments.insert(
            RequestId(id.parse().map_err(|_| StorageError::NumericRange)?),
            row.get::<_, Json<cs_mail_finance::RequestFinancials>>(1).0,
        );
    }
    let mut messages = BTreeMap::new();
    for row in tx.query(
        "SELECT message FROM cs_messages WHERE aggregate_key=$1 AND ($2 OR COALESCE(message#>>'{basis,InitialRequest,request}',message#>>'{basis,RequestFollowup,request}')=ANY($3))",
        &[&key,&all,&ids],
    )? {
        let message = row.get::<_, Json<Message>>(0).0;
        messages.insert(message.id, message);
    }
    Ok((history, payments, messages))
}
pub(super) fn persist_domain_records(
    tx: &mut Transaction<'_>,
    key: &str,
    manifest: &TransitionManifest,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let request_history = scoped_key(
        manifest
            .next_state
            .relationship
            .history
            .derivation_version(),
        manifest.next_state.relationship.history.as_bytes(),
    );
    if tx.execute(
        "UPDATE cs_request_histories SET history_state = $2, updated_at = $3 \
         WHERE request_history = $1",
        &[
            &request_history,
            &Json(&manifest.next_history),
            &to_i64(now.0)?,
        ],
    )? != 1
    {
        return Err(StorageError::VersionConflict);
    }
    for (id, request) in &manifest.next_state.requests {
        let closed = request.lifecycle.is_terminal().then_some(to_i64(now.0)?);
        tx.execute("INSERT INTO cs_requests(aggregate_key,request_id,request,closed_at) VALUES($1,$2,$3,$4) ON CONFLICT(aggregate_key,request_id) DO UPDATE SET request=EXCLUDED.request,closed_at=COALESCE(cs_requests.closed_at,EXCLUDED.closed_at) WHERE cs_requests.request IS DISTINCT FROM EXCLUDED.request",&[&key,&id.0.to_string(),&Json(request),&closed])?;
    }
    for id in &manifest.next_state.used_quotes {
        tx.execute("UPDATE cs_contact_quotes SET used=TRUE WHERE aggregate_key=$1 AND quote_id=$2 AND NOT used",&[&key,&id.0.to_string()])?;
    }
    for (id, financials) in &manifest.next_payments {
        tx.execute("INSERT INTO cs_request_financials (aggregate_key,request_id,financials) VALUES ($1,$2,$3) ON CONFLICT (aggregate_key,request_id) DO UPDATE SET financials=EXCLUDED.financials WHERE cs_request_financials.financials IS DISTINCT FROM EXCLUDED.financials", &[&key,&id.0.to_string(),&Json(financials)])?;
    }
    for message in manifest.next_messages.values() {
        persist_message(tx, key, message)?;
    }
    Ok(())
}
pub(super) fn persist_message(
    tx: &mut Transaction<'_>,
    key: &str,
    message: &Message,
) -> Result<(), StorageError> {
    if tx.query_opt("SELECT 1 FROM cs_retention_records WHERE aggregate_key=$1 AND record_domain='message' AND object_ref=$2 AND state='deleted'",&[&key,&message.id.0.to_string()])?.is_some(){return Err(StorageError::DuplicateConflict);}
    let count = tx.execute("INSERT INTO cs_messages (aggregate_key,message_id,message) VALUES ($1,$2,$3) ON CONFLICT (aggregate_key,message_id) DO UPDATE SET message=EXCLUDED.message WHERE cs_messages.message=EXCLUDED.message", &[&key,&message.id.0.to_string(),&Json(message)])?;
    if count != 1 {
        return Err(StorageError::DuplicateConflict);
    }
    Ok(())
}
