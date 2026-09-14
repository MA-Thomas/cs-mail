//! Policy-pinned lifecycle rules. Obligations and replay identity outlive formation data.
use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LifecyclePolicy {
    pub version: RetentionPolicyVersion,
    /// Retention after request closure, message validity, or quote expiry.
    pub formation_lifetime: Duration,
    /// Retention of authenticated submissions and detailed outcomes after receipt.
    pub replay_lifetime: Duration,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetentionReport {
    pub examined: usize,
    pub deleted: usize,
    pub deferred: usize,
}

/// Register newly eligible records exactly once. A later policy never rewrites their deadlines.
pub(super) fn register_records(tx: &mut Transaction<'_>, key: &str) -> Result<(), StorageError> {
    let Some(row)=tx.query_opt("SELECT policy FROM cs_lifecycle_policies WHERE aggregate_key=$1 ORDER BY version DESC LIMIT 1",&[&key])? else {return Ok(());};
    let policy = row.get::<_, Json<LifecyclePolicy>>(0).0;
    let rows=tx.query("SELECT 'request' AS domain,request_id AS object_ref,closed_at AS origin FROM cs_requests WHERE aggregate_key=$1 AND closed_at IS NOT NULL UNION ALL SELECT 'message',message_id,(message#>>'{valid_until}')::bigint FROM cs_messages WHERE aggregate_key=$1 UNION ALL SELECT 'quote',quote_id,(terms->>'expires_at')::bigint FROM cs_contact_quotes WHERE aggregate_key=$1 UNION ALL SELECT 'command',position::text,received_at FROM cs_received_commands WHERE aggregate_key=$1 AND outcome IS NOT NULL AND NOT outcome ? 'Retired'",&[&key])?;
    for row in rows {
        let domain: String = row.get(0);
        let object: String = row.get(1);
        let origin = CanonicalTime(to_u64(row.get(2))?);
        let lifetime = if domain == "command" {
            policy.replay_lifetime
        } else {
            policy.formation_lifetime
        };
        let deadline = origin
            .checked_add(lifetime)
            .ok_or(StorageError::NumericRange)?;
        let reference = record_ref(key, &domain, &object);
        tx.execute("INSERT INTO cs_retention_records(record_ref,aggregate_key,record_domain,object_ref,policy_version,delete_after,state,created_at) VALUES($1,$2,$3,$4,$5,$6,'active',$7) ON CONFLICT DO NOTHING",&[&reference,&key,&domain,&object,&i64::from(policy.version.0),&to_i64(deadline.0)?,&to_i64(origin.0)?])?;
    }
    Ok(())
}
fn record_ref(key: &str, domain: &str, object: &str) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"cs-mail/retained-record/v1");
    for part in [key, domain, object] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().to_vec()
}
impl PostgresEngine {
    /// Host configuration: explicit lifetimes, versioned and immutable once installed.
    /// # Errors
    /// Rejects conflicting policy versions, pending commands and persistence failures.
    pub fn configure_lifecycle(&self, policy: &LifecyclePolicy) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let inserted=tx.execute("INSERT INTO cs_lifecycle_policies(aggregate_key,version,policy) VALUES($1,$2,$3) ON CONFLICT(aggregate_key,version) DO UPDATE SET policy=EXCLUDED.policy WHERE cs_lifecycle_policies.policy=EXCLUDED.policy",&[&self.aggregate_key,&i64::from(policy.version.0),&Json(policy)])?;
        if inserted != 1 {
            return Err(StorageError::DuplicateConflict);
        }
        register_records(&mut tx, &self.aggregate_key)?;
        tx.commit()?;
        Ok(())
    }
    /// Host retention authority: records a scoped hold/release and optional deadline extension.
    /// The reason is audit evidence; this API is not an unauthenticated network endpoint.
    /// # Errors
    /// Rejects missing/deleted records, empty reasons, shortening retention and invalid times.
    pub fn update_retention(
        &self,
        domain: &str,
        object: &str,
        hold_until: Option<CanonicalTime>,
        delete_after: Option<CanonicalTime>,
        now: CanonicalTime,
        reason: &str,
    ) -> Result<(), StorageError> {
        if reason.trim().is_empty()
            || reason.len() > 2000
            || hold_until.is_some_and(|until| until <= now)
        {
            return Err(StorageError::NumericRange);
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let row=tx.query_opt("SELECT record_ref,delete_after FROM cs_retention_records WHERE aggregate_key=$1 AND record_domain=$2 AND object_ref=$3 AND state='active' FOR UPDATE",&[&self.aggregate_key,&domain,&object])?.ok_or(StorageError::Protocol(ProtocolError::MissingRecord))?;
        let reference: Vec<u8> = row.get(0);
        let old: i64 = row.get(1);
        let deadline = delete_after
            .map(|at| to_i64(at.0))
            .transpose()?
            .unwrap_or(old);
        if deadline < old {
            return Err(StorageError::VersionConflict);
        }
        let hold = hold_until.map(|at| to_i64(at.0)).transpose()?;
        tx.execute(
            "UPDATE cs_retention_records SET hold_until=$2,delete_after=$3,next_check_at=0 WHERE record_ref=$1",
            &[&reference, &hold, &deadline],
        )?;
        tx.execute("INSERT INTO cs_retention_changes(record_ref,at,reason,hold_until,delete_after) VALUES($1,$2,$3,$4,$5)",&[&reference,&to_i64(now.0)?,&reason,&hold,&deadline])?;
        tx.commit()?;
        Ok(())
    }
    /// Deletes a bounded batch under the receipt-order gate and the recorded holds/deadlines.
    /// Retired commands retain fingerprints and signed receipts, never execute again.
    /// # Errors
    /// Returns pending-command, database or numeric errors; the entire batch is atomic.
    pub fn run_retention(
        &self,
        now: CanonicalTime,
        limit: i64,
    ) -> Result<RetentionReport, StorageError> {
        if limit < 0 {
            return Err(StorageError::NumericRange);
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let rows=tx.query("SELECT record_ref,record_domain,object_ref,policy_version FROM cs_retention_records WHERE aggregate_key=$1 AND state='active' AND delete_after<=$2 AND next_check_at<=$2 AND (hold_until IS NULL OR hold_until<=$2) ORDER BY delete_after,record_ref LIMIT $3 FOR UPDATE",&[&self.aggregate_key,&to_i64(now.0)?,&limit])?;
        let mut report = RetentionReport {
            examined: rows.len(),
            ..RetentionReport::default()
        };
        for row in rows {
            let domain: String = row.get(1);
            let object: String = row.get(2);
            if !delete_record(&mut tx, &self.aggregate_key, &domain, &object)? {
                tx.execute(
                    "UPDATE cs_retention_records SET next_check_at=$2 WHERE record_ref=$1",
                    &[
                        &row.get::<_, Vec<u8>>(0),
                        &to_i64(
                            now.checked_add(Duration(1000))
                                .ok_or(StorageError::NumericRange)?
                                .0,
                        )?,
                    ],
                )?;
                report.deferred += 1;
                continue;
            }
            let reference: Vec<u8> = row.get(0);
            let version: i64 = row.get(3);
            tx.execute("INSERT INTO cs_deletion_manifests(record_ref,aggregate_key,record_domain,object_ref,policy_version,deleted_at,reason) VALUES($1,$2,$3,$4,$5,$6,'retention-expired')",&[&reference,&self.aggregate_key,&domain,&object,&version,&to_i64(now.0)?])?;
            tx.execute(
                "UPDATE cs_retention_records SET state='deleted' WHERE record_ref=$1",
                &[&reference],
            )?;
            report.deleted += 1;
        }
        tx.commit()?;
        Ok(report)
    }
    /// Reapplies deletion manifests after restoring a backup, before serving traffic.
    /// Import all manifests newer than the backup first. Outstanding obligations are never removed.
    /// # Errors
    /// Refuses restoration with pending work that would resurrect a deleted record.
    pub fn reconcile_restored_deletions(&self) -> Result<usize, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let rows=tx.query("SELECT record_ref,record_domain,object_ref FROM cs_deletion_manifests WHERE aggregate_key=$1 ORDER BY id",&[&self.aggregate_key])?;
        for row in &rows {
            if !delete_record(
                &mut tx,
                &self.aggregate_key,
                &row.get::<_, String>(1),
                &row.get::<_, String>(2),
            )? {
                return Err(StorageError::PendingCommands);
            }
            tx.execute(
                "UPDATE cs_retention_records SET state='deleted' WHERE record_ref=$1",
                &[&row.get::<_, Vec<u8>>(0)],
            )?;
        }
        tx.commit()?;
        Ok(rows.len())
    }
}
fn delete_record(
    tx: &mut Transaction<'_>,
    key: &str,
    domain: &str,
    object: &str,
) -> Result<bool, StorageError> {
    match domain {
        "content" => {
            if tx.query_opt("SELECT 1 FROM cs_work WHERE aggregate_key=$1 AND content_ref=$2 AND status<>'complete' LIMIT 1",&[&key,&object])?.is_some(){return Ok(false);}
            tx.execute(
                "DELETE FROM cs_encrypted_content WHERE aggregate_key=$1 AND content_ref=$2",
                &[&key, &object],
            )?;
        }
        "request" => {
            if tx.query_opt("SELECT 1 FROM cs_requests WHERE aggregate_key=$1 AND request_id=$2 AND closed_at IS NULL",&[&key,&object])?.is_some(){return Ok(false);}
            tx.execute(
                "DELETE FROM cs_requests WHERE aggregate_key=$1 AND request_id=$2",
                &[&key, &object],
            )?;
        }
        "message" => {
            // Active follow-up accounting and incomplete delivery still need this record.
            if tx.query_opt("SELECT 1 FROM cs_messages m WHERE m.aggregate_key=$1 AND m.message_id=$2 AND (EXISTS(SELECT 1 FROM cs_work w WHERE w.aggregate_key=m.aggregate_key AND w.content_ref=m.message->>'content' AND w.status<>'complete') OR EXISTS(SELECT 1 FROM cs_requests r WHERE r.aggregate_key=m.aggregate_key AND r.closed_at IS NULL AND r.request_id=COALESCE(m.message#>>'{basis,InitialRequest,request}',m.message#>>'{basis,RequestFollowup,request})))",&[&key,&object])?.is_some(){return Ok(false);}
            tx.execute(
                "DELETE FROM cs_messages WHERE aggregate_key=$1 AND message_id=$2",
                &[&key, &object],
            )?;
        }
        "quote" => {
            // Detailed replay still promises the original signed terms.
            if tx.query_opt("SELECT 1 FROM cs_received_commands WHERE aggregate_key=$1 AND outcome#>>'{Protocol,transition,terms_outcome,ChargeRequired,quote_id}'=$2 LIMIT 1",&[&key,&object])?.is_some(){return Ok(false);}
            if tx.query_opt("SELECT 1 FROM cs_contact_quotes WHERE aggregate_key=$1 AND quote_id=$2 AND signature IS NULL",&[&key,&object])?.is_some(){return Ok(false);}
            tx.execute("INSERT INTO cs_quote_tombstones(aggregate_key,quote_id) VALUES($1,$2) ON CONFLICT DO NOTHING",&[&key,&object])?;
            tx.execute(
                "DELETE FROM cs_contact_quotes WHERE aggregate_key=$1 AND quote_id=$2",
                &[&key, &object],
            )?;
        }
        "command" => {
            let position = object
                .parse::<i64>()
                .map_err(|_| StorageError::NumericRange)?;
            if tx.query_opt("SELECT 1 FROM cs_work WHERE aggregate_key=$1 AND journal_position=$2 AND status<>'complete' LIMIT 1",&[&key,&position])?.is_some(){return Ok(false);}
            let Some(row)=tx.query_opt("SELECT payload FROM cs_provider_receipts WHERE aggregate_key=$1 AND journal_position=$2",&[&key,&position])? else{return Ok(false);};
            let receipt = row.get::<_, Json<cs_mail_security::ReceiptPayload>>(0).0;
            let retired = ReceivedOutcome::Retired {
                outcome_digest: receipt.outcome_digest,
            };
            tx.execute("UPDATE cs_received_commands SET operation=NULL,authority=NULL,policy=NULL,outcome=$3,receipt=NULL WHERE aggregate_key=$1 AND position=$2",&[&key,&position,&Json(retired)])?;
            tx.execute("DELETE FROM cs_work WHERE aggregate_key=$1 AND journal_position=$2 AND status='complete'",&[&key,&position])?;
            tx.execute(
                "DELETE FROM cs_protocol_events WHERE aggregate_key=$1 AND journal_position=$2",
                &[&key, &position],
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
