//! Authenticated durable receipt, ordered application, and recoverable outcome evidence.
use super::*;
use cs_mail_capabilities::{AdmissionAuthentication, SignedBondFreeAdmission, SignedLaneControl};
use cs_mail_security::{CommandDigest, OutcomeDigest, ReceiptKind, ReceiptPayload};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum Operation {
    Protocol(KernelCommand<ProtocolCommand>),
    Message(Box<BondFreeAdmission>),
    Grant(cs_mail_capabilities::ValidatedLaneGrant),
    Control(LaneControl),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReceivedOutcome {
    Protocol(Box<DurableExecutionOutcome>),
    Message(BondFreeAdmissionOutcome),
    Lane(Box<LaneOperationOutcome>),
    Refused(Refusal),
    /// Detailed evidence expired; the original signed receipt and replay identity remain.
    Retired {
        outcome_digest: OutcomeDigest,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Refusal {
    Billing(cs_mail_billing::BillingError),
    Protocol(ProtocolError),
    Admission(cs_mail_protocol::admission::AdmissionFailure),
    Capability(CapabilityError),
    DuplicateConflict,
    VersionConflict,
    NotAuthorized,
    QuoteMissing,
    InvalidQuote,
}
impl Refusal {
    pub fn into_error(self) -> StorageError {
        match self {
            Self::Billing(e) => StorageError::Billing(e),
            Self::Protocol(e) => StorageError::Protocol(e),
            Self::Admission(e) => StorageError::AdmissionRefused(e),
            Self::Capability(e) => StorageError::Capability(e),
            Self::DuplicateConflict => StorageError::DuplicateConflict,
            Self::VersionConflict => StorageError::VersionConflict,
            Self::NotAuthorized => StorageError::BondFreeNotAuthorized,
            Self::QuoteMissing => StorageError::QuoteMissing,
            Self::InvalidQuote => StorageError::Security(SecurityError::InvalidSignature),
        }
    }
}
/// A durable record handle. Construction is confined to authenticated ingress.
#[derive(Clone, Debug)]
pub struct ReceivedCommand {
    position: JournalPosition,
    aggregate: String,
    replayed: bool,
}
impl ReceivedCommand {
    pub const fn position(&self) -> JournalPosition {
        self.position
    }
}

/// All operations sharing this database use one ordered log. Locks are never held across network I/O.
pub(super) fn lock_receipt_order(tx: &mut Transaction<'_>) -> Result<(), StorageError> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext('cs-mail-receipt-order'))",
        &[],
    )?;
    Ok(())
}
pub(super) fn require_drained(tx: &mut Transaction<'_>) -> Result<(), StorageError> {
    lock_receipt_order(tx)?;
    if tx
        .query_opt(
            "SELECT position FROM cs_received_commands WHERE outcome IS NULL LIMIT 1",
            &[],
        )?
        .is_some()
    {
        return Err(StorageError::PendingCommands);
    }
    Ok(())
}
pub(super) fn registry_locked(
    tx: &mut Transaction<'_>,
    key: &str,
) -> Result<cs_mail_security::AuthoritySnapshot, StorageError> {
    let registry = tx
        .query_opt(
            "SELECT registry FROM cs_key_registries WHERE aggregate_key=$1 FOR UPDATE",
            &[&key],
        )?
        .ok_or(StorageError::RegistryMissing)?
        .get::<_, Json<KeyRegistry>>(0)
        .0;
    let mut snapshot = cs_mail_security::AuthoritySnapshot::new(registry)?;
    // User authority comes only from product accounts.
    // Scope to the relationship's participants; provider/scheduler keys remain local.
    let relationship = tx
        .query_one(
            "SELECT relationship_state FROM cs_relationship_aggregates WHERE aggregate_key=$1",
            &[&key],
        )?
        .get::<_, Json<domain::RelationshipRecord>>(0)
        .0
        .into_state();
    let participants = [
        relationship.relationship.key.sender,
        relationship.relationship.key.recipient,
    ];
    let mut included = std::collections::BTreeSet::new();
    for persona in participants {
        if let Some(id) = persona_authority(tx, persona, &mut snapshot, &included)? {
            included.insert(id);
        }
    }
    Ok(snapshot)
}
/// Adds the authority of the account owning `persona`, if that account may use the service.
/// This is the only source of user-key authority.
pub(super) fn add_persona_authority(
    tx: &mut Transaction<'_>,
    persona: cs_mail_primitives::ProtocolIdentity,
    snapshot: &mut cs_mail_security::AuthoritySnapshot,
) -> Result<(), StorageError> {
    persona_authority(tx, persona, snapshot, &std::collections::BTreeSet::new())?;
    Ok(())
}
fn persona_authority(
    tx: &mut Transaction<'_>,
    persona: cs_mail_primitives::ProtocolIdentity,
    snapshot: &mut cs_mail_security::AuthoritySnapshot,
    included: &std::collections::BTreeSet<String>,
) -> Result<Option<String>, StorageError> {
    let Some(row) = tx.query_opt("SELECT a.id,a.registry,a.control FROM cs_persona_owners p JOIN cs_product_accounts a ON a.id=p.account WHERE p.identity=$1 FOR SHARE OF a", &[&persona.0.to_string()])? else {
        return Ok(None);
    };
    if !row
        .get::<_, Json<cs_mail_accounts::control::AccountControl>>(2)
        .0
        .allows_service()
    {
        return Ok(None);
    }
    let id: String = row.get(0);
    if included.contains(&id) {
        return Ok(None);
    }
    let personas = tx
        .query(
            "SELECT identity FROM cs_persona_owners WHERE account=$1",
            &[&id],
        )?
        .into_iter()
        .map(|r| {
            r.get::<_, String>(0)
                .parse::<u128>()
                .map(cs_mail_primitives::ProtocolIdentity)
                .map_err(|_| StorageError::NumericRange)
        })
        .collect::<Result<_, _>>()?;
    snapshot.add_account(
        cs_mail_primitives::AccountId(id.parse().map_err(|_| StorageError::NumericRange)?),
        personas,
        row.get::<_, Json<KeyRegistry>>(1).0,
    )?;
    Ok(Some(id))
}

impl PostgresEngine {
    /// Records a signature-verified command before attempting its transition.
    /// # Errors
    /// Rejects invalid signatures, scope, conflicting replay, or persistence failures.
    pub fn receive_signed(
        &self,
        signed: &SignedCommandBytes,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedCommand, StorageError> {
        let envelope =
            cs_mail_wire::decode_command_envelope(&signed.payload).map_err(SecurityError::Wire)?;
        let replay =
            ReplayIdentity::signed(envelope.idempotency_key, &signed.payload, &signed.signature);
        self.receive(deployment, clock, policy, replay, |tx, key, now, policy| {
            let relationship = load_locked_aggregate(tx, key)?
                .state
                .relationship
                .key
                .reference;
            let verified = registry_locked(tx, key)?.verify(
                signed,
                now,
                policy.protocol_version,
                SigningScope {
                    deployment_domain: deployment,
                    intended_provider: policy.recipient_provider,
                    relationship,
                },
            )?;
            let digest = verified.digest().0;
            let command = verified.into_command();
            Ok((
                command.idempotency_key(),
                digest,
                Operation::Protocol(command),
            ))
        })
    }
    /// Receives native free admission against durable key authority.
    /// # Errors
    /// Rejects invalid signature, authority, scope, or conflicting replay.
    pub fn receive_message(
        &self,
        signed: &SignedBondFreeAdmission,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedCommand, StorageError> {
        let replay = ReplayIdentity::signed(
            signed.admission.idempotency_key,
            &signed.admission.signing_bytes()?,
            &signed.signature,
        );
        self.receive(deployment, clock, policy, replay, |tx, key, now, policy| {
            let a = &signed.admission;
            check_scope(
                a.deployment_domain,
                a.intended_provider,
                a.protocol_version,
                deployment,
                policy,
            )?;
            let AdmissionAuthentication::NativeKey(reference) = a.authentication else {
                return Err(StorageError::BondFreeNotAuthorized);
            };
            if matches!(
                a.evidence,
                Some(cs_mail_capabilities::LaneEvidence::Legacy(_))
            ) {
                return Err(StorageError::BondFreeNotAuthorized);
            }
            let registry = registry_locked(tx, key)?;
            signed.verify(&registry.active_verifying_key(
                reference,
                ActorRef::Sender(a.sender),
                now,
            )?)?;
            Ok((
                a.idempotency_key,
                Sha256::digest(a.signing_bytes()?).into(),
                Operation::Message(Box::new(a.clone())),
            ))
        })
    }
    /// Receives a recipient grant using the stored recipient key.
    /// # Errors
    /// Rejects invalid scope, signature or authority.
    pub fn receive_grant(
        &self,
        signed: &SignedLaneGrant,
        idempotency: IdempotencyKey,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedCommand, StorageError> {
        let replay = ReplayIdentity::signed(
            idempotency,
            &signed.grant.signing_bytes()?,
            &signed.signature,
        );
        self.receive(deployment, clock, policy, replay, |tx, key, now, policy| {
            let g = &signed.grant;
            check_scope(
                g.deployment_domain,
                g.intended_provider,
                g.protocol_version,
                deployment,
                policy,
            )?;
            let registry = registry_locked(tx, key)?;
            let lane = cs_mail_capabilities::ValidatedLaneGrant::verify(
                signed,
                &registry.active_verifying_key(
                    g.recipient_operational_key,
                    ActorRef::Recipient(g.recipient),
                    now,
                )?,
            )?;
            let bytes = signed.grant.signing_bytes()?;
            Ok((
                idempotency,
                Sha256::digest(bytes).into(),
                Operation::Grant(lane),
            ))
        })
    }
    /// Receives a signed recipient lane control against durable authority.
    /// # Errors
    /// Rejects invalid scope, signature or authority.
    pub fn receive_control(
        &self,
        signed: &SignedLaneControl,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedCommand, StorageError> {
        let replay = ReplayIdentity::signed(
            signed.control.idempotency_key,
            &signed.control.signing_bytes(),
            &signed.signature,
        );
        self.receive(deployment, clock, policy, replay, |tx, key, now, policy| {
            let c = &signed.control;
            check_scope(
                c.deployment_domain,
                c.intended_provider,
                c.protocol_version,
                deployment,
                policy,
            )?;
            signed.verify(&registry_locked(tx, key)?.active_verifying_key(
                c.recipient_operational_key,
                ActorRef::Recipient(c.recipient),
                now,
            )?)?;
            Ok((
                c.idempotency_key,
                Sha256::digest(c.signing_bytes()).into(),
                Operation::Control(c.clone()),
            ))
        })
    }
    fn receive(
        &self,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
        replay: ReplayIdentity,
        verify: impl FnOnce(
            &mut Transaction<'_>,
            &str,
            CanonicalTime,
            &PolicySnapshot,
        ) -> Result<(IdempotencyKey, [u8; 32], Operation), StorageError>,
    ) -> Result<ReceivedCommand, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        lock_receipt_order(&mut tx)?;
        validate_ingress_scope(&mut tx, &self.aggregate_key, deployment, &policy)?;
        if let Some(row)=tx.query_opt("SELECT position,replay_fingerprint FROM cs_received_commands WHERE aggregate_key=$1 AND idempotency_key=$2",&[&self.aggregate_key,&replay.id.0.to_string()])? {
            if row.get::<_,Vec<u8>>(1)!=replay.fingerprint {return Err(StorageError::DuplicateConflict);}
            let position=row.get(0); tx.commit()?;
            return Ok(ReceivedCommand {position:JournalPosition(to_u64(position)?),aggregate:self.aggregate_key.clone(),replayed:true});
        }
        let now = clock();
        let authority = registry_locked(&mut tx, &self.aggregate_key)?;
        let (id, digest, operation) = verify(&mut tx, &self.aggregate_key, now, &policy)?;
        let content_ref = match &operation {
            Operation::Message(a) => Some(a.content_ref),
            Operation::Protocol(c) => match c.command() {
                ProtocolCommand::SubmitRequestToRecipient { content_ref, .. }
                | ProtocolCommand::AdmitFollowup { content_ref, .. } => Some(*content_ref),
                _ => None,
            },
            _ => None,
        };
        let content_available = if let Some(reference) = content_ref {
            tx.query_opt(
                "SELECT 1 FROM cs_encrypted_content WHERE aggregate_key=$1 AND content_ref=$2",
                &[&self.aggregate_key, &reference.0.to_string()],
            )?
            .is_some()
        } else {
            true
        };
        let last: Option<i64> = tx
            .query_one("SELECT max(received_at) FROM cs_received_commands", &[])?
            .get(0);
        let receipt_time = to_i64(now.0)?;
        if last.is_some_and(|last| last > receipt_time) {
            return Err(StorageError::VersionConflict);
        }
        let position=tx.query_one("INSERT INTO cs_received_commands (aggregate_key,idempotency_key,digest,deployment,received_at,operation,policy,replay_fingerprint,authority,content_available) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING position",&[&self.aggregate_key,&id.0.to_string(),&digest.as_slice(),&deployment.as_slice(),&receipt_time,&Json(operation),&Json(policy),&replay.fingerprint.as_slice(),&Json(authority),&content_available])?.get(0);
        tx.commit()?;
        Ok(ReceivedCommand {
            position: JournalPosition(to_u64(position)?),
            aggregate: self.aggregate_key.clone(),
            replayed: false,
        })
    }
    /// Processes the oldest durable command, including commands for another relationship.
    /// # Errors
    /// Database, signing-dependency and internal failures leave the command pending for retry.
    pub fn process_next_received(&self) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        lock_receipt_order(&mut tx)?;
        let Some(row)=tx.query_opt("SELECT * FROM cs_received_commands WHERE outcome IS NULL ORDER BY position LIMIT 1 FOR UPDATE",&[])? else { return Ok(false); };
        let position: i64 = row.get("position");
        let received_position = JournalPosition(to_u64(position)?);
        let content_available: bool = row.get("content_available");
        let key: String = row.get("aggregate_key");
        let now = CanonicalTime(to_u64(row.get("received_at"))?);
        let operation = row.get::<_, Json<Operation>>("operation").0;
        let policy = row.get::<_, Json<PolicySnapshot>>("policy").0;
        tx.batch_execute("SAVEPOINT apply_command")?;
        let result = match &operation {
            Operation::Protocol(c) => apply_protocol(
                &mut tx,
                &key,
                c,
                now,
                policy.clone(),
                received_position,
                content_available,
            )
            .map(Box::new)
            .map(ReceivedOutcome::Protocol),
            Operation::Message(a) => {
                apply_free(&mut tx, &key, a, now, received_position, content_available)
                    .map(ReceivedOutcome::Message)
            }
            Operation::Grant(l) => apply_grant(
                &mut tx,
                &key,
                l.clone(),
                now,
                received_position,
                policy.clone(),
            )
            .map(Box::new)
            .map(ReceivedOutcome::Lane),
            Operation::Control(c) => apply_control(&mut tx, &key, c, now, received_position)
                .map(Box::new)
                .map(ReceivedOutcome::Lane),
        };
        let outcome = match result {
            Ok(o) => o,
            Err(error) => {
                let refusal = classify_refusal(error)?;
                tx.batch_execute("ROLLBACK TO SAVEPOINT apply_command")?;
                ReceivedOutcome::Refused(refusal)
            }
        };
        let aggregate = load_locked_aggregate(&mut tx, &key)?;
        let digest: Vec<u8> = row.get("digest");
        let command_digest =
            CommandDigest(digest.try_into().map_err(|_| StorageError::NumericRange)?);
        let outcome_digest = OutcomeDigest(Sha256::digest(serde_json::to_vec(&outcome)?).into());
        let receipt = ReceiptPayload {
            deployment_domain: row
                .get::<_, Vec<u8>>("deployment")
                .try_into()
                .map_err(|_| StorageError::NumericRange)?,
            receipt_id: cs_mail_primitives::ReceiptRef(u128::from(to_u64(position)?)),
            kind: receipt_kind(&operation, &outcome),
            relationship: aggregate.state.relationship.key.reference,
            command_digest,
            journal_position: JournalPosition(to_u64(position)?),
            received_at: now,
            outcome_digest,
            provider: policy.recipient_provider,
            protocol_version: policy.protocol_version,
        };
        tx.execute(
            "UPDATE cs_received_commands SET outcome=$2,receipt=$3 WHERE position=$1",
            &[&position, &Json(outcome), &Json(receipt)],
        )?;
        work::enqueue(
            &mut tx,
            &key,
            work::WorkSource::Receipt {
                position: received_position,
                ordinal: 0,
            },
            &WorkPayload::Artifacts {
                position: received_position,
            },
            now,
        )?;
        retention::register_records(&mut tx, &key)?;
        tx.commit()?;
        Ok(true)
    }
    /// Resolves a durable receipt in canonical order. Exact replay returns the stored outcome.
    /// # Errors
    /// Returns storage errors. Domain refusals are explicit `ReceivedOutcome::Refused` values.
    pub fn process_received(
        &self,
        handle: &ReceivedCommand,
    ) -> Result<ReceivedOutcome, StorageError> {
        loop {
            {
                let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
                let row=client.query_one("SELECT outcome FROM cs_received_commands WHERE position=$1 AND aggregate_key=$2",&[&to_i64(handle.position.0)?,&handle.aggregate])?;
                if let Some(Json(mut outcome)) = row.get::<_, Option<Json<ReceivedOutcome>>>(0) {
                    match &mut outcome {
                        ReceivedOutcome::Protocol(o) => o.replayed = handle.replayed,
                        ReceivedOutcome::Message(o) => o.replayed = handle.replayed,
                        ReceivedOutcome::Lane(o) => o.replayed = handle.replayed,
                        ReceivedOutcome::Refused(_) | ReceivedOutcome::Retired { .. } => {}
                    }
                    return Ok(outcome);
                }
            }
            self.process_next_received()?;
        }
    }
    pub(super) fn execute_system(
        &self,
        command: KernelCommand<ProtocolCommand>,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedOutcome, StorageError> {
        let replay =
            ReplayIdentity::trusted(command.idempotency_key(), &serde_json::to_vec(&command)?);
        let handle = self.receive(
            policy.financial.scope.deployment_domain,
            || now,
            policy,
            replay,
            |tx, key, now, _| {
                match command.command() {
                    ProtocolCommand::RecordPayment {
                        request_id,
                        receipt,
                    } => {
                        let aggregate = load_locked_aggregate(tx, key)?;
                        let (_, payments, _) = load_domain_records(
                            tx,
                            key,
                            &aggregate.state,
                            false,
                            Some(*request_id),
                        )?;
                        let financials = payments
                            .get(request_id)
                            .ok_or(StorageError::Protocol(ProtocolError::IncompleteSnapshot))?;
                        let operation = if receipt.evidence.operation_id == financials.capture().id
                        {
                            financials.capture()
                        } else {
                            financials
                                .refund()
                                .operation()
                                .ok_or(StorageError::Protocol(ProtocolError::PaymentInvalid))?
                        };
                        receipt
                            .verify(financials.provider_key(), operation)
                            .map_err(|_| StorageError::Protocol(ProtocolError::PaymentInvalid))?;
                    }
                    _ => {
                        registry_locked(tx, key)?.active_verifying_key(
                            command.operational_key(),
                            command.actor(),
                            now,
                        )?;
                    }
                }

                Ok((
                    command.idempotency_key(),
                    Sha256::digest(serde_json::to_vec(&command)?).into(),
                    Operation::Protocol(command),
                ))
            },
        )?;
        match self.process_received(&handle)? {
            outcome @ (ReceivedOutcome::Protocol(_) | ReceivedOutcome::Retired { .. }) => {
                Ok(outcome)
            }
            ReceivedOutcome::Refused(e) => Err(e.into_error()),
            _ => unreachable!(),
        }
    }
}
fn check_scope(
    domain: [u8; 32],
    provider: ProviderRef,
    version: ProtocolVersion,
    expected: [u8; 32],
    policy: &PolicySnapshot,
) -> Result<(), StorageError> {
    if domain != expected
        || provider != policy.recipient_provider
        || version != policy.protocol_version
    {
        return Err(StorageError::Security(SecurityError::SigningScopeMismatch));
    }
    Ok(())
}
fn receipt_kind(operation: &Operation, outcome: &ReceivedOutcome) -> ReceiptKind {
    if matches!(outcome, ReceivedOutcome::Refused(_)) {
        return ReceiptKind::Refused;
    }
    if matches!(outcome,ReceivedOutcome::Lane(o) if !o.changed) {
        return ReceiptKind::NoChange;
    }
    if let ReceivedOutcome::Protocol(o) = outcome {
        if o.transition.protocol_events.is_empty() {
            return ReceiptKind::NoChange;
        }
        if matches!(operation,Operation::Protocol(c) if matches!(c.command(),ProtocolCommand::SubmitRequestToRecipient{..}))
            && !o.transition.delivered
        {
            return ReceiptKind::SettlementCommitted;
        }
    }

    match operation {
        Operation::Message(_) => ReceiptKind::AdmissionCommitted,
        Operation::Grant(_) | Operation::Control(_) => ReceiptKind::RelationshipDecisionCommitted,
        Operation::Protocol(c) => match c.command() {
            ProtocolCommand::IssueRequestTerms { .. } => ReceiptKind::ContactTermsIssued,
            ProtocolCommand::CreateRequest { .. } => ReceiptKind::ReservationCommitted,
            ProtocolCommand::SubmitRequestToRecipient { .. }
            | ProtocolCommand::AdmitFollowup { .. } => ReceiptKind::AdmissionCommitted,
            ProtocolCommand::AcceptRelationship { .. }
            | ProtocolCommand::RejectRelationship { .. }
            | ProtocolCommand::BlockRelationship { .. }
            | ProtocolCommand::UnblockRelationship { .. }
            | ProtocolCommand::RevokeRelationship { .. }
            | ProtocolCommand::SetFollowupPolicy { .. } => {
                ReceiptKind::RelationshipDecisionCommitted
            }
            _ => ReceiptKind::SettlementCommitted,
        },
    }
}
#[allow(clippy::too_many_lines)] // Request state, funding reservations and external work share one atomic boundary.
fn apply_protocol(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    authorized: &KernelCommand<ProtocolCommand>,
    now: CanonicalTime,
    policy: PolicySnapshot,
    journal_position: JournalPosition,
    content_available: bool,
) -> Result<DurableExecutionOutcome, StorageError> {
    let mut aggregate = load_locked_aggregate(transaction, aggregate_key)?;
    if matches!(
        authorized.command(),
        ProtocolCommand::IssueRequestTerms { .. }
            | ProtocolCommand::CreateRequest { .. }
            | ProtocolCommand::SubmitRequestToRecipient { .. }
            | ProtocolCommand::AdmitFollowup { .. }
    ) {
        let account =
            billing::identity_account(transaction, aggregate.state.relationship.key.recipient)?;
        if account.scope() != policy.financial.scope || !account.covers(now) {
            return Err(cs_mail_billing::BillingError::ServiceNotCovered.into());
        }
        billing::check_local_sender_coverage(
            transaction,
            aggregate.state.relationship.key.sender,
            now,
        )?;
    }
    let pricing = if let ProtocolCommand::IssueRequestTerms { class_id, .. } = authorized.command()
    {
        pricing::resolve_price(
            transaction,
            aggregate.state.relationship.key.recipient,
            *class_id,
        )?
    } else {
        cs_mail_protocol::pricing::QuotePricing::NotRequested
    };
    let target = match authorized.command() {
        ProtocolCommand::CreateRequest { request_id, .. }
        | ProtocolCommand::SubmitRequestToRecipient { request_id, .. }
        | ProtocolCommand::AdmitFollowup { request_id, .. }
        | ProtocolCommand::CancelRequestSubmission { request_id, .. }
        | ProtocolCommand::ExpireRequest { request_id, .. }
        | ProtocolCommand::RecordPayment { request_id, .. } => Some(*request_id),
        _ => None,
    };
    if target.is_some() {
        domain::load_request_records(
            transaction,
            aggregate_key,
            &mut aggregate.state,
            false,
            target,
        )?;
    }
    let blocks_relationship = matches!(
        authorized.command(),
        ProtocolCommand::BlockRelationship { .. }
    );
    let balances = load_balances(transaction, aggregate_key)?;
    let ledger = LedgerView::from_balances(aggregate.ledger_revision, aggregate.unit, balances);
    let (history, payments, messages) =
        load_domain_records(transaction, aggregate_key, &aggregate.state, false, target)?;
    let mut snapshot = SettlementSnapshot::complete(
        aggregate.revision,
        aggregate.state,
        history,
        payments,
        messages,
        ledger,
    );
    snapshot.lane = load_current_lane(transaction, aggregate_key)?;
    validate_issued_quote(transaction, aggregate_key, authorized.command())?;
    let next_revision = aggregate
        .revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    record_journal_position(
        transaction,
        aggregate_key,
        next_revision,
        "protocol",
        now,
        journal_position,
    )?;
    let admission = if content_available {
        assess_protocol_admission(
            transaction,
            aggregate_key,
            &snapshot.state,
            authorized,
            now,
            policy.protocol_version,
        )?
    } else {
        Err(cs_mail_protocol::admission::AdmissionFailure::ContentMissing)
    };
    let context = TransitionContext {
        admission,
        now,
        journal_position,
        protocol_version: policy.protocol_version,
        policy,
        pricing,
    };
    let manifest = transition(&snapshot, authorized, &context)?;
    // Charged terms must name the configured payment arrangement's processor. Checked only
    // when the kernel requires a charge, as before the pricing cutover.
    if matches!(
        manifest.terms_outcome,
        Some(TermsOutcome::ChargeRequired(_))
    ) && billing::arrangement(
        transaction,
        context.policy.financial.scope,
        context.policy.unit,
    )?
    .0 != context.policy.payment_provider_key
    {
        return Err(ProtocolError::PolicyInvalid.into());
    }
    match authorized.command() {
        ProtocolCommand::CreateRequest { request_id, .. } => {
            let account =
                billing::identity_account(transaction, snapshot.state.relationship.key.sender)?;
            let financials = manifest
                .next_payments
                .get(request_id)
                .ok_or(ProtocolError::MissingRecord)?;
            billing::reserve_source(transaction, account.id(), financials.capture())?;
        }
        ProtocolCommand::RecordPayment {
            request_id,
            receipt,
        } => {
            let financials = manifest
                .next_payments
                .get(request_id)
                .ok_or(ProtocolError::MissingRecord)?;
            if receipt.evidence.operation_id == financials.capture().id {
                billing::record_source(
                    transaction,
                    financials.capture(),
                    receipt,
                    financials.provider_key(),
                )?;
            }
        }
        _ => {}
    }
    let admission_failure = context.admission.err();
    let next_ledger = snapshot.ledger.apply(&manifest.ledger_batch)?;
    persist_manifest(
        transaction,
        aggregate_key,
        &manifest,
        &next_ledger,
        context.now,
        context.journal_position,
    )?;
    if blocks_relationship {
        revoke_lane_locked(
            transaction,
            aggregate_key,
            context.now,
            context.journal_position,
        )?;
    }
    Ok(DurableExecutionOutcome {
        admission_failure,
        transition: manifest.committed(journal_position),
        replayed: false,
    })
}

fn assess_protocol_admission(
    transaction: &mut Transaction<'_>,
    key: &str,
    state: &ProtocolState,
    command: &KernelCommand<ProtocolCommand>,
    now: CanonicalTime,
    protocol: ProtocolVersion,
) -> Result<Result<(), cs_mail_protocol::admission::AdmissionFailure>, StorageError> {
    let Some(message) =
        cs_mail_protocol::admission::proposed_message(state, command.command(), now)
    else {
        return Ok(Ok(()));
    };
    match validate_delivery(
        transaction,
        key,
        &message,
        cs_mail_protocol::admission::AdmissionContext {
            relationship: state.relationship.key,
            now,
            protocol,
            authority: cs_mail_primitives::DeclarationAuthority::NativeSender(
                command.operational_key(),
            ),
            capability: None,
        },
    ) {
        Ok(()) => Ok(Ok(())),
        Err(StorageError::AdmissionRefused(reason)) => Ok(Err(reason)),
        Err(error) => Err(error),
    }
}

fn apply_grant(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    grant: cs_mail_capabilities::ValidatedLaneGrant,
    now: CanonicalTime,
    received_position: JournalPosition,
    policy: PolicySnapshot,
) -> Result<LaneOperationOutcome, StorageError> {
    let aggregate = load_locked_aggregate(transaction, aggregate_key)?;
    let balances = load_balances(transaction, aggregate_key)?;
    let ledger = LedgerView::from_balances(aggregate.ledger_revision, aggregate.unit, balances);
    let (history, payments, messages) =
        load_domain_records(transaction, aggregate_key, &aggregate.state, false, None)?;
    let mut snapshot = SettlementSnapshot::complete(
        aggregate.revision,
        aggregate.state,
        history,
        payments,
        messages,
        ledger,
    );
    snapshot.lane = load_current_lane(transaction, aggregate_key)?;
    let context = TransitionContext {
        admission: Ok(()),
        now,
        journal_position: received_position,
        protocol_version: policy.protocol_version,
        policy,
        pricing: cs_mail_protocol::pricing::QuotePricing::NotRequested,
    };
    let lane = grant.clone().activate()?;
    let decision =
        cs_mail_application::relationships::LaneAcceptance::decide(&snapshot, grant, &context)
            .map_err(|e| match e {
                cs_mail_application::relationships::LaneAcceptanceError::Protocol(e) => {
                    StorageError::Protocol(e)
                }
                cs_mail_application::relationships::LaneAcceptanceError::Capability(e) => {
                    StorageError::Capability(e)
                }
            })?;
    let Some(decision) = decision else {
        return Ok(LaneOperationOutcome {
            lane,
            changed: false,
            replayed: false,
        });
    };
    let (lane, manifest) = decision.into_effects();
    record_journal_position(
        transaction,
        aggregate_key,
        aggregate
            .revision
            .checked_add(1)
            .ok_or(StorageError::NumericRange)?,
        "capability",
        now,
        received_position,
    )?;
    let ledger = snapshot.ledger.apply(&manifest.ledger_batch)?;
    persist_manifest(
        transaction,
        aggregate_key,
        &manifest,
        &ledger,
        now,
        received_position,
    )?;
    upsert_lane(transaction, aggregate_key, &lane, now)?;
    persist_schedules(transaction, aggregate_key, &[lane.schedule_change()])?;
    insert_capability_event(
        transaction,
        aggregate_key,
        received_position,
        &CapabilityEvent::LaneGranted(lane.grant.id),
    )?;
    Ok(LaneOperationOutcome {
        lane,
        changed: true,
        replayed: false,
    })
}

fn load_current_lane(tx: &mut Transaction<'_>, key: &str) -> Result<Option<Lane>, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1 FOR UPDATE",
            &[&key],
        )?
        .map(|r| r.get::<_, Json<Lane>>(0).0))
}

fn apply_control(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    control: &LaneControl,
    now: CanonicalTime,
    received_position: JournalPosition,
) -> Result<LaneOperationOutcome, StorageError> {
    let aggregate = load_locked_aggregate(transaction, aggregate_key)?;
    if control.recipient != aggregate.state.relationship.key.recipient {
        return Err(StorageError::BondFreeNotAuthorized);
    }

    let row = transaction
        .query_opt(
            "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1 FOR UPDATE",
            &[&aggregate_key],
        )?
        .ok_or(StorageError::Capability(CapabilityError::MissingLane))?;
    let mut lane = row.get::<_, Json<Lane>>("lane").0;
    if lane.grant.id != control.lane_id || lane.version != control.expected_version {
        return Err(StorageError::VersionConflict);
    }
    let event = match control.action {
        LaneControlAction::Reconfirm => {
            if aggregate.state.relationship.state == RelationshipState::Blocked {
                return Err(StorageError::Protocol(ProtocolError::ContactBlocked));
            }
            lane.reconfirm(now)?;
            CapabilityEvent::LaneReconfirmed(lane.grant.id)
        }
        LaneControlAction::Revoke => {
            lane.revoke()?;
            CapabilityEvent::LaneRevoked(lane.grant.id)
        }
    };
    let position = advance_aggregate_revision(
        transaction,
        aggregate_key,
        aggregate.revision,
        now,
        Some(received_position),
    )?;
    upsert_lane(transaction, aggregate_key, &lane, now)?;
    match control.action {
        LaneControlAction::Reconfirm => {
            persist_schedules(transaction, aggregate_key, &[lane.schedule_change()])?;
        }
        LaneControlAction::Revoke => delete_schedule(
            transaction,
            aggregate_key,
            ScheduleTask::LaneHorizon(lane.grant.id),
        )?,
    }
    insert_capability_event(transaction, aggregate_key, position, &event)?;

    Ok(LaneOperationOutcome {
        changed: true,
        lane,
        replayed: false,
    })
}

fn apply_free(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    request: &BondFreeAdmission,
    now: CanonicalTime,
    received_position: JournalPosition,
    content_available: bool,
) -> Result<BondFreeAdmissionOutcome, StorageError> {
    let aggregate = load_locked_aggregate(transaction, aggregate_key)?;
    let account =
        billing::identity_account(transaction, aggregate.state.relationship.key.recipient)?;
    if !account.covers(now) {
        return Err(cs_mail_billing::BillingError::ServiceNotCovered.into());
    }
    billing::check_local_sender_coverage(
        transaction,
        aggregate.state.relationship.key.sender,
        now,
    )?;

    let record = if content_available {
        transaction.query_opt("SELECT record FROM cs_encrypted_content WHERE aggregate_key=$1 AND content_ref=$2 FOR SHARE",&[&aggregate_key,&request.content_ref.0.to_string()])?.map(|r|r.get::<_,Json<EncryptedContentRecord>>(0).0)
    } else {
        None
    };
    let lane = transaction
        .query_opt(
            "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1 FOR UPDATE",
            &[&aggregate_key],
        )?
        .map(|r| r.get::<_, Json<Lane>>(0).0);
    let plan = cs_mail_protocol::admission::plan_free_admission(
        &aggregate.state,
        request,
        record.as_ref(),
        lane.as_ref(),
        now,
        &admission_policy(transaction, aggregate_key)?,
    )
    .map_err(|e| match e {
        cs_mail_protocol::admission::AdmissionPlanError::Policy(p) => {
            StorageError::AdmissionRefused(p)
        }
        cs_mail_protocol::admission::AdmissionPlanError::Protocol(p) => StorageError::Protocol(p),
        cs_mail_protocol::admission::AdmissionPlanError::Capability(p) => {
            StorageError::Capability(p)
        }
    })?;
    if transaction
        .query_opt(
            "SELECT 1 FROM cs_messages WHERE aggregate_key=$1 AND message_id=$2",
            &[&aggregate_key, &plan.message.id.0.to_string()],
        )?
        .is_some()
    {
        return Err(StorageError::DuplicateConflict);
    }
    persist_message(transaction, aggregate_key, &plan.message)?;
    let (authority, event) = if let Some(lane) = plan.next_lane {
        upsert_lane(transaction, aggregate_key, &lane, now)?;
        persist_schedules(transaction, aggregate_key, &[lane.schedule_change()])?;
        (
            BondFreeAuthority::ExpressLane(lane.grant.id),
            CapabilityEvent::LaneMessageAdmitted(lane.grant.id, request.message_id),
        )
    } else {
        (
            BondFreeAuthority::AcceptedRelationship,
            CapabilityEvent::AcceptedMessageAdmitted(request.message_id),
        )
    };
    let position = advance_aggregate_revision(
        transaction,
        aggregate_key,
        aggregate.revision,
        now,
        Some(received_position),
    )?;
    let intent = EffectIntent::DeliverMessage {
        message_id: request.message_id,
        content_ref: request.content_ref,
        delivery_intent_ref: request.delivery_intent_ref,
    };
    enqueue_effect(transaction, aggregate_key, position, now, &intent)?;
    insert_capability_event(transaction, aggregate_key, position, &event)?;
    let outcome = BondFreeAdmissionOutcome {
        authority,
        journal_position: position,
        replayed: false,
    };

    Ok(outcome)
}

fn admission_policy(
    tx: &mut Transaction<'_>,
    key: &str,
) -> Result<cs_mail_protocol::admission::AdmissionPolicy, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT policy FROM cs_admission_policies WHERE aggregate_key=$1",
            &[&key],
        )?
        .map(|r| {
            r.get::<_, Json<cs_mail_protocol::admission::AdmissionPolicy>>(0)
                .0
        })
        .unwrap_or_default())
}
fn validate_delivery(
    tx: &mut Transaction<'_>,
    key: &str,
    message: &Message,
    context: cs_mail_protocol::admission::AdmissionContext,
) -> Result<(), StorageError> {
    let row=tx.query_opt("SELECT record FROM cs_encrypted_content WHERE aggregate_key=$1 AND content_ref=$2 FOR SHARE",&[&key,&message.content.0.to_string()])?.ok_or(StorageError::AdmissionRefused(cs_mail_protocol::admission::AdmissionFailure::ContentMissing))?;
    let record = row.get::<_, Json<EncryptedContentRecord>>(0).0;
    cs_mail_protocol::admission::validate_message(
        &record,
        message,
        context,
        &admission_policy(tx, key)?,
    )
    .map_err(StorageError::AdmissionRefused)
}
impl PostgresEngine {
    /// Installs a versioned admission policy under the same ordering gate as receipt.
    /// # Errors
    /// Refuses older versions, conflicting definitions and overtaking pending commands.
    pub fn configure_admission_policy(
        &self,
        policy: &cs_mail_protocol::admission::AdmissionPolicy,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        lock_receipt_order(&mut tx)?;
        if let Some(row) = tx.query_opt(
            "SELECT policy FROM cs_admission_policies WHERE aggregate_key=$1 FOR UPDATE",
            &[&self.aggregate_key],
        )? {
            let old = row
                .get::<_, Json<cs_mail_protocol::admission::AdmissionPolicy>>(0)
                .0;
            if old == *policy {
                tx.commit()?;
                return Ok(());
            }
            if policy.version <= old.version {
                return Err(StorageError::VersionConflict);
            }
        }
        require_drained(&mut tx)?;
        tx.execute("INSERT INTO cs_admission_policies(aggregate_key,policy) VALUES($1,$2) ON CONFLICT(aggregate_key) DO UPDATE SET policy=EXCLUDED.policy",&[&self.aggregate_key,&Json(policy)])?;
        tx.commit()?;
        Ok(())
    }
}

impl PostgresEngine {
    /// Receives gateway-issued evidence. Sender-supplied JSON cannot construct the proof.
    /// # Errors
    /// Refuses wrong deployment/provider scope and conflicting replay.
    pub fn receive_legacy(
        &self,
        verified: &cs_mail_capabilities::VerifiedLegacyAdmission,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedCommand, StorageError> {
        let a = verified.admission();
        let replay = ReplayIdentity::trusted(a.idempotency_key, &a.signing_bytes()?);
        self.receive(deployment, clock, policy, replay, |_, _, _, policy| {
            let a = verified.admission();
            check_scope(
                a.deployment_domain,
                a.intended_provider,
                a.protocol_version,
                deployment,
                policy,
            )?;
            Ok((
                a.idempotency_key,
                Sha256::digest(a.signing_bytes()?).into(),
                Operation::Message(Box::new(a.clone())),
            ))
        })
    }
    /// Executes bounded, fenced artifact work. Signing occurs without a database lock.
    /// # Errors
    /// Returns storage errors. Missing signing authority blocks its own item, not the batch.
    pub fn sign_artifacts_batch(
        &self,
        signer: &cs_mail_security::ProviderSigner,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<WorkReport, StorageError> {
        let items = self.claim_work(work::RelationshipQueue::Artifacts, now, lease, limit)?;
        let mut report = WorkReport {
            claimed: items.len(),
            ..WorkReport::default()
        };
        for item in items {
            match self.materialize_artifacts(&item, signer) {
                Ok(()) => {
                    if self.complete_work(&item, now)? {
                        report.completed += 1;
                    } else {
                        report.lost_claims += 1;
                    }
                }
                Err(StorageError::Security(_)) => {
                    if self.block_work(&item, now, WorkFailure::SigningAuthority)? {
                        report.blocked += 1;
                    } else {
                        report.lost_claims += 1;
                    }
                }
                Err(_) => {
                    if self.retry_work(&item, now, WorkFailure::Storage)? {
                        report.retried += 1;
                    } else {
                        report.lost_claims += 1;
                    }
                }
            }
        }
        Ok(report)
    }
    fn materialize_artifacts(
        &self,
        item: &WorkItem,
        signer: &cs_mail_security::ProviderSigner,
    ) -> Result<(), StorageError> {
        let WorkPayload::Artifacts { position } = item.payload else {
            return Err(StorageError::VersionConflict);
        };
        let (payload, registry, outcome) = {
            let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
            if client
                .query_opt(
                    "SELECT 1 FROM cs_provider_receipts WHERE aggregate_key=$1 AND receipt_id=$2",
                    &[&self.aggregate_key, &position.0.to_string()],
                )?
                .is_some()
            {
                return Ok(());
            }
            let row=client.query_one("SELECT receipt,authority,outcome FROM cs_received_commands WHERE aggregate_key=$1 AND position=$2",&[&self.aggregate_key,&to_i64(position.0)?])?;
            (
                row.get::<_, Json<ReceiptPayload>>(0).0,
                row.get::<_, Json<cs_mail_security::AuthoritySnapshot>>(1).0,
                row.get::<_, Json<ReceivedOutcome>>(2).0,
            )
        };
        if payload.provider != signer.provider()
            || registry.active_verifying_key(
                signer.reference(),
                ActorRef::Provider(signer.provider()),
                payload.received_at,
            )? != signer.verifying_key_bytes()
        {
            return Err(StorageError::Security(SecurityError::SigningScopeMismatch));
        }
        let quote = match outcome {
            ReceivedOutcome::Protocol(o) => match o.transition.terms_outcome {
                Some(TermsOutcome::ChargeRequired(terms)) => {
                    Some(signer.sign_contact_terms(*terms)?)
                }
                _ => None,
            },
            _ => None,
        };
        let receipt = signer.sign_receipt(payload)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        if let Some(quote) = quote {
            tx.execute("UPDATE cs_contact_quotes SET provider_operational_key=$3,signature=$4,provider_verifying_key=$5 WHERE aggregate_key=$1 AND quote_id=$2 AND signature IS NULL",&[&self.aggregate_key,&quote.terms.quote_id.0.to_string(),&quote.provider_operational_key.0.to_string(),&quote.signature.as_slice(),&signer.verifying_key_bytes().as_slice()])?;
        }
        tx.execute("INSERT INTO cs_provider_receipts(aggregate_key,receipt_id,journal_position,payload,provider_operational_key,signature,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(aggregate_key,receipt_id) DO NOTHING",&[&self.aggregate_key,&position.0.to_string(),&to_i64(position.0)?,&Json(&receipt.payload),&receipt.provider_operational_key.0.to_string(),&receipt.signature.as_slice(),&to_i64(receipt.payload.received_at.0)?])?;
        tx.commit()?;
        Ok(())
    }
    /// Retrieves the stable signed artifact for an authenticated receipt handle.
    /// # Errors
    /// Returns decoding/storage errors; unsigned work returns `None`.
    pub fn receipt(&self, handle: &ReceivedCommand) -> Result<Option<SignedReceipt>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        client.query_opt("SELECT payload,provider_operational_key,signature FROM cs_provider_receipts WHERE aggregate_key=$1 AND receipt_id=$2",&[&handle.aggregate,&handle.position.0.to_string()])?.map(|r|Ok(SignedReceipt {payload:r.get::<_,Json<ReceiptPayload>>(0).0,provider_operational_key:OperationalKeyRef(r.get::<_,String>(1).parse().map_err(|_|StorageError::NumericRange)?),signature:r.get::<_,Vec<u8>>(2).try_into().map_err(|_|StorageError::NumericRange)?})).transpose()
    }
    /// Reads provider-signed terms already materialized by artifact work.
    /// # Errors
    /// Returns missing/signing/persistence errors.
    pub fn signed_quote(
        &self,
        id: cs_mail_primitives::QuoteId,
    ) -> Result<SignedContactTerms, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row=client.query_opt("SELECT terms,provider_operational_key,signature FROM cs_contact_quotes WHERE aggregate_key=$1 AND quote_id=$2",&[&self.aggregate_key,&id.0.to_string()])?.ok_or(StorageError::QuoteMissing)?;
        Ok(SignedContactTerms {
            terms: row.get::<_, Json<cs_mail_protocol::RequestTerms>>(0).0,
            provider_operational_key: OperationalKeyRef(
                row.get::<_, Option<String>>(1)
                    .ok_or(StorageError::QuoteNotSigned)?
                    .parse()
                    .map_err(|_| StorageError::NumericRange)?,
            ),
            signature: row
                .get::<_, Option<Vec<u8>>>(2)
                .ok_or(StorageError::QuoteNotSigned)?
                .try_into()
                .map_err(|_| StorageError::NumericRange)?,
        })
    }
}

/// A fingerprint includes the authentication evidence, so replay cannot turn an unsigned
/// copy of a command into a read capability after its signer has been revoked.
#[derive(Clone, Copy)]
struct ReplayIdentity {
    id: IdempotencyKey,
    fingerprint: [u8; 32],
}
impl ReplayIdentity {
    fn signed(id: IdempotencyKey, bytes: &[u8], signature: &[u8; 64]) -> Self {
        let mut h = Sha256::new();
        h.update(b"cs-mail/signed-replay/v1");
        h.update(bytes);
        h.update(signature);
        Self {
            id,
            fingerprint: h.finalize().into(),
        }
    }
    fn trusted(id: IdempotencyKey, bytes: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(b"cs-mail/trusted-replay/v1");
        h.update(bytes);
        Self {
            id,
            fingerprint: h.finalize().into(),
        }
    }
}
fn validate_ingress_scope(
    tx: &mut Transaction<'_>,
    key: &str,
    deployment: [u8; 32],
    policy: &PolicySnapshot,
) -> Result<(), StorageError> {
    let row=tx.query_opt("SELECT deployment,provider,protocol,financial_scope FROM cs_ingress_scopes WHERE aggregate_key=$1",&[&key])?.ok_or(StorageError::RegistryMissing)?;
    let scope = policy.financial.scope;
    if row.get::<_, Vec<u8>>(0) != deployment
        || row.get::<_, String>(1) != policy.recipient_provider.0.to_string()
        || row.get::<_, i32>(2) != i32::from(policy.protocol_version.0)
        || row.get::<_, Json<cs_mail_finance::FinancialScope>>(3).0 != scope
        || scope.deployment_domain != deployment
        || scope.operator != policy.recipient_provider
        || scope.protocol_version != policy.protocol_version
    {
        return Err(StorageError::Security(SecurityError::SigningScopeMismatch));
    }
    Ok(())
}
impl PostgresEngine {
    /// Pins deployment and provider scope at administrative startup; subsequent calls cannot replace it.
    /// # Errors
    /// Rejects conflicting configuration or storage failures.
    pub fn configure_ingress(
        &self,
        deployment: [u8; 32],
        policy: &PolicySnapshot,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        lock_receipt_order(&mut tx)?;
        tx.execute("INSERT INTO cs_ingress_scopes(aggregate_key,deployment,provider,protocol,financial_scope) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",&[&self.aggregate_key,&deployment.as_slice(),&policy.recipient_provider.0.to_string(),&i32::from(policy.protocol_version.0),&Json(policy.financial.scope)])?;
        validate_ingress_scope(&mut tx, &self.aggregate_key, deployment, policy)?;
        tx.commit()?;
        Ok(())
    }
}

impl PostgresEngine {
    /// Verifies the native content certificate and persists its ciphertext under one authority lock.
    /// # Errors
    /// Rejects invalid certificates, forged declaration authority, or conflicting ciphertext.
    pub fn store_authenticated_content(
        &self,
        record: &EncryptedContentRecord,
        certificate: &cs_mail_security::SignedContentKeyCertificate,
        clock: impl FnOnce() -> CanonicalTime,
        policy: &PolicySnapshot,
        deployment: [u8; 32],
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        lock_receipt_order(&mut tx)?;
        validate_ingress_scope(&mut tx, &self.aggregate_key, deployment, policy)?;
        let relationship = load_locked_aggregate(&mut tx, &self.aggregate_key)?
            .state
            .relationship
            .key;
        let digest = registry_locked(&mut tx, &self.aggregate_key)?
            .verify_content_key_certificate(
                certificate,
                clock(),
                policy.protocol_version,
                SigningScope {
                    deployment_domain: deployment,
                    intended_provider: policy.recipient_provider,
                    relationship: relationship.reference,
                },
            )?;
        let c = &certificate.certificate;
        if c.key.reference != record.envelope.sender_key
            || c.owner != record.binding.sender
            || record.binding.sender != relationship.sender
            || record.binding.recipient != relationship.recipient
            || record.binding.relationship != relationship.reference
            || record.binding.sender_certificate != digest
            || record.binding.declarations.origin.authority
                != cs_mail_primitives::DeclarationAuthority::NativeSender(c.operational_key)
        {
            return Err(StorageError::ContentScopeMismatch);
        }
        persist_content(
            &mut tx,
            &self.aggregate_key,
            record,
            policy.retention_policy_version,
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn classify_refusal(error: StorageError) -> Result<Refusal, StorageError> {
    Ok(match error {
        StorageError::Protocol(ProtocolError::AdmissionRefused(reason))
        | StorageError::AdmissionRefused(reason) => Refusal::Admission(reason),
        StorageError::Protocol(error) => Refusal::Protocol(error),
        StorageError::Billing(error) => Refusal::Billing(error),
        StorageError::Capability(error) => Refusal::Capability(error),
        StorageError::DuplicateConflict => Refusal::DuplicateConflict,
        StorageError::VersionConflict => Refusal::VersionConflict,
        StorageError::BondFreeNotAuthorized => Refusal::NotAuthorized,
        StorageError::QuoteMissing => Refusal::QuoteMissing,
        StorageError::Security(SecurityError::InvalidSignature) => Refusal::InvalidQuote,
        error => return Err(error),
    })
}
