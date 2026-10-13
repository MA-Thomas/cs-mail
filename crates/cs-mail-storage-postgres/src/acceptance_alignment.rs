//! Explicit maintenance execution through the ordinary authenticated lane owner.
use super::*;
use cs_mail_application::relationships::alignment::{
    AcceptanceAlignmentCommand, AcceptanceAlignmentContext, AlignmentDisposition,
};
use cs_mail_capabilities::VerifiedLaneGrant;
use cs_mail_protocol::{ProtocolEvent, RelationshipRequest, RequestLifecycle};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AcceptanceAlignmentOutcome {
    pub disposition: AlignmentDisposition,
    pub position: Option<JournalPosition>,
    pub replayed: bool,
}
impl PostgresDeployment {
    /// # Errors
    /// Refuses altered retries, substituted evidence, invalid authority or storage failure.
    /// Consent, ordinary lane effects, receipt and correction provenance commit together.
    #[allow(clippy::too_many_lines)] // Keep consent, protected evidence, ordinary effects and provenance in one atomic operation.
    pub fn align_development_acceptance(
        &self,
        command: &AcceptanceAlignmentCommand,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<AcceptanceAlignmentOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        // This maintenance operation requires an existing explicit continuity archive.
        let operation = command.operation.0.to_string();
        if let Some(row) = tx.query_opt("SELECT record FROM cs_development_continuity_archive WHERE kind='acceptance-alignment' AND record->>'operation'=$1", &[&operation])? {
            let Json(record): Json<serde_json::Value> = row.get(0);
            if record["command"] != serde_json::to_value(command)? { return Err(StorageError::DuplicateConflict); }
            let mut outcome: AcceptanceAlignmentOutcome = serde_json::from_value(record["outcome"].clone())?;
            outcome.replayed = true;
            tx.commit()?;
            return Ok(outcome);
        }
        let row = tx.query_one("SELECT aggregate_key,request FROM cs_requests WHERE request_id=$1 AND request#>'{terms,relationship}'=$2 FOR UPDATE", &[&command.source.request.0.to_string(),&Json(command.source.relationship)])?;
        let original_key: String = row.get(0);
        let Json(request): Json<RelationshipRequest> = row.get(1);
        let original = load_locked_aggregate(&mut tx, &original_key)?;
        let reverse_key: String = tx.query_one("SELECT aggregate_key FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2 FOR UPDATE", &[&request.terms.recipient.0.to_string(),&request.terms.sender.0.to_string()])?.get(0);
        let reverse = load_locked_aggregate(&mut tx, &reverse_key)?;
        let lane = ingress::load_current_lane(&mut tx, &reverse_key)?;
        let acceptance = match request.lifecycle {
            RequestLifecycle::Accepted { event, .. } => tx.query_opt("SELECT event FROM cs_protocol_events WHERE aggregate_key=$1 AND event->'reference'=$2", &[&original_key,&Json(event)])?.map(|row| row.get::<_,Json<ProtocolEvent>>(0).0),
            _ => None,
        };
        let withdrawal = tx.query_opt("SELECT event FROM cs_protocol_events WHERE aggregate_key=ANY($1) AND event->>'kind' IN('RelationshipBlocked','RelationshipRevoked') ORDER BY journal_position DESC LIMIT 1", &[&vec![original_key.clone(), reverse_key.clone()]])?.map(|row| row.get::<_,Json<ProtocolEvent>>(0).0);
        // The prepared, signed correction freezes one immutable policy version.
        // Publishing a later policy must not reinterpret the same operation.
        let policy = tx
            .query_opt(
                "SELECT policy FROM cs_protocol_policies WHERE version=$1 FOR SHARE",
                &[&to_i64(command.policy.policy_version.0)?],
            )?
            .ok_or(StorageError::VersionConflict)?
            .get::<_, Json<cs_mail_protocol::policy::ProtocolPolicy>>(0)
            .0
            .snapshot(cs_mail_protocol::policy::DeploymentTerms {
                protocol_version: request.terms.protocol_version,
                provider: request.terms.recipient_provider,
                unit: request.terms.unit,
                financial_scope: request.terms.financial.scope,
                payment_provider_key: request.terms.payment_provider_key,
            })?;
        if policy != command.policy {
            return Err(StorageError::VersionConflict);
        }
        let authority = ingress::registry_locked(&mut tx, &reverse_key)?;
        let now = clock();
        let grant = &command.consent.grant;
        ingress::check_scope(
            grant.deployment_domain,
            grant.intended_provider,
            grant.protocol_version,
            policy.financial.scope.deployment_domain,
            &policy,
        )?;
        let proof = VerifiedLaneGrant::verify(
            &command.consent,
            &authority.active_verifying_key(
                grant.recipient_operational_key,
                ActorRef::Recipient(grant.recipient),
                now,
            )?,
        )?;
        let disposition = command.decide(
            AcceptanceAlignmentContext {
                request: &request,
                acceptance: acceptance.as_ref(),
                original: &original.state.relationship,
                reverse: &reverse.state.relationship,
                lane: lane.as_ref(),
                last_withdrawal: withdrawal.as_ref(),
                reverse_has_unresolved_request: reverse
                    .state
                    .requests
                    .values()
                    .any(|request| !request.lifecycle.is_terminal()),
                now,
            },
            proof,
        )?;
        let position = if disposition == AlignmentDisposition::Apply {
            let bytes = grant.signing_bytes()?;
            let position = ingress::record_received(
                &mut tx,
                &reverse_key,
                ingress::ReceivedFacts {
                    id: command.operation,
                    digest: Sha256::digest(&bytes).into(),
                    deployment: grant.deployment_domain,
                    now,
                    operation: ingress::Operation::Grant(command.consent.clone()),
                    policy,
                    replay: ingress::ReplayIdentity::signed(
                        command.operation,
                        &bytes,
                        &command.consent.signature,
                    ),
                    authority,
                    content_available: true,
                },
            )?;
            let row = tx.query_one(
                "SELECT * FROM cs_received_commands WHERE position=$1",
                &[&position],
            )?;
            ingress::process_received_row(&mut tx, &row, position)?;
            let Json(outcome): Json<ReceivedOutcome> = tx
                .query_one(
                    "SELECT outcome FROM cs_received_commands WHERE position=$1",
                    &[&position],
                )?
                .get(0);
            if !matches!(outcome, ReceivedOutcome::Lane(ref outcome) if outcome.changed) {
                return Err(StorageError::VersionConflict);
            }
            Some(JournalPosition(to_u64(position)?))
        } else {
            None
        };
        let outcome = AcceptanceAlignmentOutcome {
            disposition,
            position,
            replayed: false,
        };
        tx.execute("INSERT INTO cs_development_continuity_archive(kind,record) VALUES('acceptance-alignment',$1)", &[&Json(serde_json::json!({"operation":operation,"command":command,"outcome":outcome,"recorded_at":now}))])?;
        tx.commit()?;
        Ok(outcome)
    }
}
