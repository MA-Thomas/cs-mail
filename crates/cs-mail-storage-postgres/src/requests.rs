//! Protected facts and persistence for application-owned paired request decisions.
use super::{
    StorageError, domain, ingress, load_balances, load_domain_records, load_locked_aggregate,
    persist_manifest, record_journal_position,
};
use cs_mail_application::requests::{CounterpartyContext, DirectionEffect, DirectionSnapshot};
use cs_mail_ledger::LedgerView;
use cs_mail_primitives::{CanonicalTime, ProtocolIdentity};
use cs_mail_protocol::SettlementSnapshot;
use postgres::Transaction;

pub(super) fn load_counterparty(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
) -> Result<(CounterpartyContext, Option<String>), StorageError> {
    if super::guests::is_guest(tx, sender)? {
        return Ok((CounterpartyContext::Guest, None));
    }
    // The receipt-order lock is held by the caller and protects absent pair rows too.
    let key = tx
        .query_opt(
            "SELECT aggregate_key FROM cs_relationship_aggregates
        WHERE relationship_state#>>'{relationship,key,sender}'=$1
          AND relationship_state#>>'{relationship,key,recipient}'=$2
          AND protocol_format_version=$3 FOR UPDATE",
            &[
                &recipient.0.to_string(),
                &sender.0.to_string(),
                &super::CURRENT_PROTOCOL_FORMAT_VERSION,
            ],
        )?
        .map(|row| row.get::<_, String>(0));
    let opposite = if let Some(key) = &key {
        let mut aggregate = load_locked_aggregate(tx, key)?;
        domain::load_request_records(tx, key, &mut aggregate.state, true, None)?;
        let (history, payments, messages) =
            load_domain_records(tx, key, &aggregate.state, true, None)?;
        let ledger = LedgerView::from_balances(
            aggregate.ledger_revision,
            aggregate.unit,
            load_balances(tx, key)?,
        );
        let mut snapshot = SettlementSnapshot::complete(
            aggregate.revision,
            aggregate.state,
            history,
            payments,
            messages,
            ledger,
        );
        snapshot.lane = ingress::load_current_lane(tx, key)?;
        let mut reply_offers = std::collections::BTreeMap::new();
        for row in tx.query("SELECT c.request_id,c.record FROM cs_request_conversations c JOIN cs_relationship_aggregates a ON a.aggregate_key=c.aggregate_key WHERE a.relationship_state#>>'{relationship,key,sender}'=$1 AND a.relationship_state#>>'{relationship,key,recipient}'=$2", &[&sender.0.to_string(),&recipient.0.to_string()])? {
            let link = row.try_get::<_,postgres::types::Json<cs_mail_application::requests::documents::RequestConversation>>(1)?.0;
            reply_offers.insert(link.request().request, link.reverse_offer_epoch());
        }
        Some(DirectionSnapshot {
            reply_offers,
            snapshot: Box::new(snapshot),
        })
    } else {
        None
    };
    Ok((CounterpartyContext::Member { opposite }, key))
}

pub(super) fn persist_opposite(
    tx: &mut Transaction<'_>,
    key: &str,
    declared: &DirectionEffect,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let effect = declared.manifest();
    let position = declared.journal_position();
    let revision = effect
        .expected_snapshot_revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    // The decision already contains the complete batch; storage only restores the
    // protected ledger needed to check its declared expected revision.
    let aggregate = load_locked_aggregate(tx, key)?;
    if aggregate.ledger_revision != effect.expected_ledger_revision
        || aggregate.state.relationship.key.reference != declared.relationship()
    {
        return Err(StorageError::VersionConflict);
    }
    record_journal_position(tx, key, revision, "protocol", now, position)?;
    let ledger = LedgerView::from_balances(
        aggregate.ledger_revision,
        aggregate.unit,
        load_balances(tx, key)?,
    );
    let next_ledger = ledger.apply(&effect.ledger_batch)?;
    persist_manifest(tx, key, effect, &next_ledger, now, position)?;
    if let Some(lane) = declared.lane() {
        super::upsert_lane(tx, key, lane, now)?;
        super::insert_capability_event(
            tx,
            key,
            position,
            &super::CapabilityEvent::LaneGranted(lane.grant().id),
        )?;
    }
    super::persist_schedules(tx, key, declared.schedules())?;
    Ok(())
}
