//! The deterministic cs-mail protocol kernel.
//!
//! `transition` performs no I/O. It consumes a complete snapshot and an
//! authenticated command, then returns the entire atomic consequence.

use cs_mail_finance::{
    FinancialTerms, Forfeiture, PaymentKind, PaymentOperation, RequestFinancialContract,
    RequestFinancialEffects, RequestSettlement, SignedPaymentEvidence, request_payment_id,
};
use cs_mail_primitives::PaymentOperationId;
use std::collections::BTreeMap;

use cs_mail_ledger::{LedgerBatch, LedgerError};
use cs_mail_primitives::{
    CanonicalTime, ContentRef, DeliveryIntentRef, Duration, EventRef, IdempotencyKey,
    JournalPosition, LaneId, MessageDeclarationDigest, MessageId, MessageValidityUntil, Money,
    OperationalKeyRef, PolicyVersion, PrivacyProfileVersion, ProtocolIdentity, ProtocolVersion,
    ProviderRef, QuoteId, RelationshipVersion, RequestId, RequestVersion, RetentionPolicyVersion,
    SettlementUnit, Version,
};
pub use cs_mail_primitives::{ScheduleChange, ScheduleTask};
use serde::{Deserialize, Serialize};

mod model;
pub use model::*;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PolicySnapshot {
    pub pricing_policy_version: PolicyVersion,
    pub protocol_version: ProtocolVersion,
    pub policy_version: PolicyVersion,
    pub privacy_profile_version: PrivacyProfileVersion,
    pub retention_policy_version: RetentionPolicyVersion,
    pub recipient_provider: ProviderRef,
    pub unit: SettlementUnit,
    pub processing_charge: Money,
    pub collateral: Money,

    pub submission_window: Duration,
    pub decision_window: Duration,
    pub quote_lifetime: Duration,
    pub backoff: Vec<Duration>,
    pub financial: FinancialTerms,
    pub payment_provider_key: [u8; 32],
    pub expiry_cooldown: Duration,
    pub rejection_cooldown: Duration,
}

impl PolicySnapshot {
    fn backoff_for(&self, level: u32) -> Duration {
        self.backoff
            .get(level as usize)
            .copied()
            .or_else(|| self.backoff.last().copied())
            .unwrap_or(Duration(0))
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self
            .processing_charge
            .checked_add(self.collateral)
            .is_none_or(Money::is_zero)
            || self.financial.validate().is_err()
            || self.payment_provider_key == [0; 32]
            || self.decision_window.0 == 0
            || self.backoff.is_empty()
            || self.backoff[0] != Duration(0)
            || !self.backoff.windows(2).all(|pair| pair[0] <= pair[1])
        {
            return Err(ProtocolError::PolicyInvalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionContext {
    /// Result of the host's content and policy checks at canonical receipt time.
    pub admission: Result<(), admission::AdmissionFailure>,
    pub now: CanonicalTime,
    pub journal_position: JournalPosition,
    pub protocol_version: ProtocolVersion,
    pub policy: PolicySnapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ActorRef {
    Sender(ProtocolIdentity),
    Recipient(ProtocolIdentity),
    Provider(ProviderRef),
    Scheduler(ProviderRef),
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct KernelCommand<T> {
    command: T,
    actor: ActorRef,
    key: OperationalKeyRef,
    idempotency_key: IdempotencyKey,
}

impl<T> KernelCommand<T> {
    /// Builds input for the pure transition kernel. This is data, not proof of authentication.
    /// Durable ingress accepts signed submissions and establishes authority itself.
    pub const fn new(
        command: T,
        actor: ActorRef,
        key: OperationalKeyRef,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            command,
            actor,
            key,
            idempotency_key,
        }
    }

    pub const fn command(&self) -> &T {
        &self.command
    }

    pub const fn actor(&self) -> ActorRef {
        self.actor
    }

    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }

    pub const fn operational_key(&self) -> OperationalKeyRef {
        self.key
    }

    pub fn into_parts(self) -> (T, ActorRef, OperationalKeyRef, IdempotencyKey) {
        (self.command, self.actor, self.key, self.idempotency_key)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum CancellationReason {
    SenderRequested,

    SubmissionTimeout,

    PreSubmissionFailure,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ProtocolCommand {
    RecordPayment {
        request_id: RequestId,
        receipt: SignedPaymentEvidence,
    },
    SetFollowupPolicy {
        expected_version: Version,
        policy: FollowupPolicy,
    },
    AdmitFollowup {
        request_id: RequestId,
        message_id: MessageId,
        content_ref: ContentRef,
        delivery_intent_ref: DeliveryIntentRef,
        declaration_digest: MessageDeclarationDigest,
        message_valid_until: MessageValidityUntil,
        expected_policy_version: Version,
    },
    IssueRequestTerms {
        quote_id: QuoteId,
        declaration_digest: Option<MessageDeclarationDigest>,
    },
    CreateRequest {
        payment_method: [u8; 32],
        request_id: RequestId,
        message_id: MessageId,
        terms: Box<RequestTerms>,
    },
    /// Commits the funded initial message for delivery and starts the decision window.
    SubmitRequestToRecipient {
        request_id: RequestId,
        expected_request_version: Version,
        content_ref: ContentRef,
        delivery_intent_ref: DeliveryIntentRef,
        declaration_digest: MessageDeclarationDigest,
        message_valid_until: MessageValidityUntil,
    },

    CancelRequestSubmission {
        request_id: RequestId,
        expected_request_version: Version,
        reason: CancellationReason,
    },
    AcceptRelationship {
        expected_version: Version,
    },
    RejectRelationship {
        expected_version: Version,
    },
    BlockRelationship {
        expected_version: Version,
    },
    UnblockRelationship {
        expected_version: Version,
    },
    ExpireRequest {
        request_id: RequestId,
        expected_request_version: Version,
    },
    RevokeRelationship {
        expected_version: Version,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TermsOutcome {
    ChargeRequired(Box<RequestTerms>),
    NoChargeRequired,
    ExistingRequest(RequestId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum EffectIntent {
    ExecutePayment {
        request_id: RequestId,
        operation_id: PaymentOperationId,
    },
    DeliverMessage {
        message_id: MessageId,
        content_ref: ContentRef,
        delivery_intent_ref: DeliveryIntentRef,
    },
    EstablishRequestSolicitation {
        request_id: RequestId,
        generation: u64,
    },
    ReviewLane {
        lane_id: LaneId,
        reconfirmation_required: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ProtocolEventKind {
    PaymentRecorded(RequestId),
    FollowupPolicyChanged,
    FollowupAdmitted(RequestId, MessageId),
    TermsIssued,
    RequestCreated(RequestId),

    RequestSubmitted(RequestId),

    RequestSubmissionCancelled(RequestId),
    RequestHistoryChanged(RequestId),
    MessageValidityClosed(RequestId),
    DeclarationMismatch(RequestId),
    RelationshipAccepted,
    RelationshipRejected,
    RelationshipBlocked,
    RelationshipUnblocked,
    RelationshipRevoked,
    RequestExpired(RequestId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProtocolEvent {
    pub reference: EventRef,
    pub at: CanonicalTime,
    pub kind: ProtocolEventKind,
}

/// Minimal replay evidence. Full transition snapshots are transient commit plans.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommittedTransition {
    pub journal_position: JournalPosition,
    pub protocol_events: Vec<ProtocolEvent>,
    pub terms_outcome: Option<TermsOutcome>,
    pub delivered: bool,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionManifest {
    pub forfeitures: Vec<Forfeiture>,
    pub forfeiture_holds: Vec<PaymentOperationId>,
    pub expected_snapshot_revision: u64,
    pub expected_ledger_revision: u64,
    pub next_state: ProtocolState,
    pub next_history: RequestHistory,
    pub next_payments: BTreeMap<RequestId, cs_mail_finance::RequestFinancials>,
    pub next_messages: BTreeMap<MessageId, Message>,

    pub ledger_batch: LedgerBatch,
    pub schedule_changes: Vec<ScheduleChange>,
    pub protocol_events: Vec<ProtocolEvent>,
    pub outbox_intents: Vec<EffectIntent>,
    pub terms_outcome: Option<TermsOutcome>,
}

impl TransitionManifest {
    pub fn committed(&self, journal_position: JournalPosition) -> CommittedTransition {
        CommittedTransition {
            journal_position,
            protocol_events: self.protocol_events.clone(),
            terms_outcome: self.terms_outcome.clone(),
            delivered: self
                .outbox_intents
                .iter()
                .any(|effect| matches!(effect, EffectIntent::DeliverMessage { .. })),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProtocolError {
    AuthenticationFailed,
    AdmissionRefused(admission::AdmissionFailure),
    QuoteExpired,
    QuoteVersionStale,
    RelationshipAccepted,
    ContactBlocked,
    BackoffActive { next_eligible: CanonicalTime },
    InsufficientFunds,
    RequestAlreadyExists,
    PaymentNotConfirmed,
    PaymentInvalid,
    FollowupNotAllowed,
    AmountMismatch,

    SubmissionWindowClosed,

    RequestSubmissionCancelled,
    DecisionWindowClosed,
    MessageValidityClosed,
    AlreadyTerminal,
    VersionConflict,
    DuplicateConflict,
    InvalidState,
    MissingRecord,
    IncompleteSnapshot,
    ProtocolVersionMismatch,
    PolicyInvalid,
    ArithmeticOverflow,
    LedgerInvariant,
}

impl From<LedgerError> for ProtocolError {
    fn from(error: LedgerError) -> Self {
        match error {
            LedgerError::InsufficientFunds { .. } => Self::InsufficientFunds,
            LedgerError::ArithmeticOverflow => Self::ArithmeticOverflow,
            LedgerError::VersionConflict => Self::VersionConflict,
            LedgerError::InvalidBatch => Self::LedgerInvariant,
        }
    }
}

struct ManifestBuilder {
    next: ProtocolState,
    history: RequestHistory,
    payments: BTreeMap<RequestId, cs_mail_finance::RequestFinancials>,
    messages: BTreeMap<MessageId, Message>,

    forfeitures: Vec<Forfeiture>,
    forfeiture_holds: Vec<PaymentOperationId>,
    batch: LedgerBatch,
    schedules: Vec<ScheduleChange>,
    events: Vec<ProtocolEvent>,
    effects: Vec<EffectIntent>,
    terms: Option<TermsOutcome>,
    event: EventRef,
    now: CanonicalTime,
}

impl ManifestBuilder {
    fn new(snapshot: &SettlementSnapshot, context: &TransitionContext) -> Self {
        Self {
            next: snapshot.state.clone(),
            history: snapshot.history.clone(),
            payments: snapshot.payments.clone(),
            messages: snapshot.messages.clone(),
            forfeitures: Vec::new(),
            forfeiture_holds: Vec::new(),
            batch: LedgerBatch::new(),
            schedules: Vec::new(),
            events: Vec::new(),
            effects: Vec::new(),
            terms: None,
            event: EventRef(context.journal_position),
            now: context.now,
        }
    }

    fn event(&mut self, kind: ProtocolEventKind) {
        self.events.push(ProtocolEvent {
            reference: self.event,
            at: self.now,
            kind,
        });
    }

    fn finish(self, snapshot: &SettlementSnapshot) -> Result<TransitionManifest, ProtocolError> {
        snapshot.ledger.apply(&self.batch)?;
        Ok(TransitionManifest {
            forfeitures: self.forfeitures,
            forfeiture_holds: self.forfeiture_holds,
            expected_snapshot_revision: snapshot.revision,
            expected_ledger_revision: snapshot.ledger.revision,
            next_state: self.next,
            next_history: self.history,
            next_payments: self.payments,
            next_messages: self.messages,
            ledger_batch: self.batch,
            schedule_changes: self.schedules,
            protocol_events: self.events,
            outbox_intents: self.effects,
            terms_outcome: self.terms,
        })
    }
}

/// Computes the complete atomic consequence of one authorized command.
///
/// # Errors
///
/// Returns a stable `ProtocolError` when authentication, versions, time,
/// funding, state, or completeness preconditions are not satisfied.
pub fn transition(
    snapshot: &SettlementSnapshot,
    authorized: &KernelCommand<ProtocolCommand>,
    context: &TransitionContext,
) -> Result<TransitionManifest, ProtocolError> {
    if context.protocol_version != ProtocolVersion(2) {
        return Err(ProtocolError::ProtocolVersionMismatch);
    }
    if !snapshot.complete {
        return Err(ProtocolError::IncompleteSnapshot);
    }
    if context.protocol_version != context.policy.protocol_version {
        return Err(ProtocolError::ProtocolVersionMismatch);
    }
    context.policy.validate()?;
    if snapshot.ledger.unit != context.policy.unit {
        return Err(ProtocolError::AmountMismatch);
    }

    if snapshot.history.reference != snapshot.state.relationship.history
        || snapshot.history.recipient != snapshot.state.relationship.key.recipient
        || snapshot.state.requests.values().any(|r| {
            snapshot
                .payments
                .get(&r.id)
                .is_none_or(|p| p.capture().id != r.funding)
        })
    {
        return Err(ProtocolError::IncompleteSnapshot);
    }
    let actor = authorized.actor();
    let command = authorized.command();
    authorize(
        &snapshot.state,
        actor,
        command,
        context.policy.recipient_provider,
    )?;
    let mut manifest = ManifestBuilder::new(snapshot, context);

    apply_command(&mut manifest, context, actor, command)?;
    manifest.finish(snapshot)
}

fn apply_command(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    actor: ActorRef,
    command: &ProtocolCommand,
) -> Result<(), ProtocolError> {
    match command {
        ProtocolCommand::RecordPayment {
            request_id,
            receipt,
        } => record_payment(manifest, *request_id, receipt)?,
        ProtocolCommand::SetFollowupPolicy {
            expected_version,
            policy,
        } => set_followup_policy(manifest, *expected_version, *policy)?,
        ProtocolCommand::AdmitFollowup {
            request_id,
            message_id,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
            expected_policy_version,
        } => admit_followup(
            manifest,
            *request_id,
            *message_id,
            *content_ref,
            *delivery_intent_ref,
            *declaration_digest,
            *message_valid_until,
            *expected_policy_version,
            context.admission,
        )?,
        ProtocolCommand::IssueRequestTerms {
            quote_id,
            declaration_digest,
        } => {
            issue_terms(manifest, context, *quote_id, *declaration_digest)?;
        }
        ProtocolCommand::CreateRequest {
            payment_method,
            request_id,
            message_id,
            terms,
        } => create_request(
            manifest,
            context,
            *request_id,
            *message_id,
            *payment_method,
            terms,
        )?,
        ProtocolCommand::SubmitRequestToRecipient {
            request_id,
            expected_request_version,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
        } => submit_request_to_recipient(
            manifest,
            context,
            RequestSubmissionInput {
                request_id: *request_id,
                expected_version: *expected_request_version,
                content_ref: *content_ref,
                delivery_intent_ref: *delivery_intent_ref,
                declaration_digest: *declaration_digest,
                message_valid_until: *message_valid_until,
            },
        )?,
        ProtocolCommand::CancelRequestSubmission {
            request_id,
            expected_request_version,
            reason,
        } => cancel_request_submission(
            manifest,
            actor,
            *request_id,
            *expected_request_version,
            *reason,
        )?,
        ProtocolCommand::AcceptRelationship { expected_version } => {
            relationship_decision(manifest, *expected_version, Decision::Accept)?;
        }
        ProtocolCommand::RejectRelationship { expected_version } => {
            relationship_decision(manifest, *expected_version, Decision::Reject)?;
        }
        ProtocolCommand::BlockRelationship { expected_version } => {
            relationship_decision(manifest, *expected_version, Decision::Block)?;
        }
        ProtocolCommand::UnblockRelationship { expected_version } => {
            unblock(manifest, *expected_version)?;
        }
        ProtocolCommand::ExpireRequest {
            request_id,
            expected_request_version,
        } => expire_request(manifest, *request_id, *expected_request_version)?,
        ProtocolCommand::RevokeRelationship { expected_version } => {
            revoke(manifest, *expected_version)?;
        }
    }

    Ok(())
}
fn set_followup_policy(
    manifest: &mut ManifestBuilder,
    expected_version: Version,
    mut policy: FollowupPolicy,
) -> Result<(), ProtocolError> {
    if manifest.next.followup_policy.version != expected_version {
        return Err(ProtocolError::VersionConflict);
    }
    if policy.max_messages > 0 && (policy.max_per_interval == 0 || policy.interval.0 == 0) {
        return Err(ProtocolError::PolicyInvalid);
    }
    policy.version = next_version(expected_version)?;
    manifest.next.followup_policy = policy;
    manifest.event(ProtocolEventKind::FollowupPolicyChanged);
    Ok(())
}

fn authorize(
    state: &ProtocolState,
    actor: ActorRef,
    command: &ProtocolCommand,
    provider: ProviderRef,
) -> Result<(), ProtocolError> {
    let sender = state.relationship.key.sender;
    let recipient = state.relationship.key.recipient;
    let authorized = match command {
        ProtocolCommand::RecordPayment { .. } => actor == ActorRef::Provider(provider),
        ProtocolCommand::AdmitFollowup { .. }
        | ProtocolCommand::IssueRequestTerms { .. }
        | ProtocolCommand::CreateRequest { .. }
        | ProtocolCommand::SubmitRequestToRecipient { .. } => actor == ActorRef::Sender(sender),
        ProtocolCommand::CancelRequestSubmission { reason, .. } => match reason {
            CancellationReason::SenderRequested => actor == ActorRef::Sender(sender),
            CancellationReason::SubmissionTimeout => actor == ActorRef::Scheduler(provider),
            CancellationReason::PreSubmissionFailure => actor == ActorRef::Provider(provider),
        },
        ProtocolCommand::SetFollowupPolicy { .. }
        | ProtocolCommand::AcceptRelationship { .. }
        | ProtocolCommand::RejectRelationship { .. }
        | ProtocolCommand::BlockRelationship { .. }
        | ProtocolCommand::UnblockRelationship { .. }
        | ProtocolCommand::RevokeRelationship { .. } => actor == ActorRef::Recipient(recipient),
        ProtocolCommand::ExpireRequest { .. } => {
            matches!(actor, ActorRef::Scheduler(p) | ActorRef::Provider(p) if p == provider)
        }
    };
    if authorized {
        Ok(())
    } else {
        Err(ProtocolError::AuthenticationFailed)
    }
}

fn issue_terms(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    quote_id: QuoteId,
    declaration_digest: Option<MessageDeclarationDigest>,
) -> Result<(), ProtocolError> {
    if let Some(request) = manifest.next.active_request() {
        if manifest.next.relationship.state == RelationshipState::Blocked {
            return Err(ProtocolError::ContactBlocked);
        }
        manifest.terms = Some(TermsOutcome::ExistingRequest(request.id));
        manifest.event(ProtocolEventKind::TermsIssued);
        return Ok(());
    }
    match manifest.next.relationship.state {
        RelationshipState::Accepted => {
            manifest.terms = Some(TermsOutcome::NoChargeRequired);
        }
        RelationshipState::Blocked => return Err(ProtocolError::ContactBlocked),
        RelationshipState::Unknown | RelationshipState::Rejected | RelationshipState::Revoked => {
            if manifest.history.pending_submission.is_some() {
                return Err(ProtocolError::RequestAlreadyExists);
            }
            if context.now < manifest.history.earliest_next_submission {
                return Err(ProtocolError::BackoffActive {
                    next_eligible: manifest.history.earliest_next_submission,
                });
            }
            let policy = &context.policy;
            let expires_at = context
                .now
                .checked_add(policy.quote_lifetime)
                .ok_or(ProtocolError::ArithmeticOverflow)?;
            let terms = RequestTerms {
                pricing_policy_version: policy.pricing_policy_version,
                quote_id,
                protocol_version: context.protocol_version,
                policy_version: policy.policy_version,
                privacy_profile_version: policy.privacy_profile_version,
                retention_policy_version: policy.retention_policy_version,
                relationship: manifest.next.relationship.key.reference,
                request_history: manifest.history.reference,
                sender: manifest.next.relationship.key.sender,
                recipient: manifest.next.relationship.key.recipient,
                recipient_provider: policy.recipient_provider,
                relationship_version: manifest.next.relationship.version,
                history_version: manifest.history.version,
                processing_charge: policy.processing_charge,
                collateral: policy.collateral,
                request_level: manifest.history.level,
                eligibility_time: manifest.history.earliest_next_submission,
                unit: policy.unit,
                submission_window: policy.submission_window,
                decision_window: policy.decision_window,
                issued_at: context.now,
                expires_at,
                declaration_digest,
                financial: policy.financial,
                payment_provider_key: policy.payment_provider_key,
                expiry_cooldown: policy.expiry_cooldown,
                rejection_cooldown: policy.rejection_cooldown,
                next_request_backoff: policy.backoff_for(
                    manifest
                        .history
                        .level
                        .checked_add(1)
                        .ok_or(ProtocolError::ArithmeticOverflow)?,
                ),
            };
            terms.charge_amount()?;
            if manifest.next.quotes.contains_key(&quote_id)
                || manifest.next.used_quotes.contains(&quote_id)
            {
                return Err(ProtocolError::DuplicateConflict);
            }
            manifest.next.quotes.insert(quote_id, terms.clone());
            manifest.terms = Some(TermsOutcome::ChargeRequired(Box::new(terms)));
        }
    }
    manifest.event(ProtocolEventKind::TermsIssued);
    Ok(())
}

fn create_request(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    request_id: RequestId,
    message_id: MessageId,
    payment_method: [u8; 32],
    terms: &RequestTerms,
) -> Result<(), ProtocolError> {
    match manifest.next.relationship.state {
        RelationshipState::Accepted => return Err(ProtocolError::RelationshipAccepted),
        RelationshipState::Blocked => return Err(ProtocolError::ContactBlocked),
        RelationshipState::Unknown | RelationshipState::Rejected | RelationshipState::Revoked => {}
    }
    let submission_deadline = validate_request_terms(manifest, context, terms)?;
    if manifest.next.requests.contains_key(&request_id)
        || manifest.payments.contains_key(&request_id)
    {
        return Err(ProtocolError::DuplicateConflict);
    }
    if manifest
        .next
        .requests
        .values()
        .any(|r| !r.lifecycle.is_terminal())
        || manifest.history.pending_submission.is_some()
    {
        return Err(ProtocolError::RequestAlreadyExists);
    }
    if manifest.next.used_quotes.contains(&terms.quote_id)
        || manifest.next.quotes.get(&terms.quote_id) != Some(terms)
    {
        return Err(ProtocolError::QuoteVersionStale);
    }
    if payment_method == [0; 32] {
        return Err(ProtocolError::PolicyInvalid);
    }
    if context.now < manifest.history.earliest_next_submission {
        return Err(ProtocolError::BackoffActive {
            next_eligible: manifest.history.earliest_next_submission,
        });
    }
    let capture = PaymentOperation {
        scope: terms.financial.scope,
        id: request_payment_id(terms.financial.scope, terms.relationship, request_id, false),
        kind: PaymentKind::Capture,
        amount: terms.charge_amount()?,
        unit: terms.unit,
        destination: payment_method,
    };
    if capture.amount.is_zero() {
        return Err(ProtocolError::PolicyInvalid);
    }
    manifest.history.pending_submission = Some(RequestReservation {
        relationship: terms.relationship,
        request: request_id,
    });
    manifest.next.used_quotes.insert(terms.quote_id);
    manifest.effects.push(EffectIntent::ExecutePayment {
        request_id,
        operation_id: capture.id,
    });
    let generation = manifest
        .next
        .relationship
        .generation
        .checked_add(1)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    manifest.next.relationship.generation = generation;
    manifest.payments.insert(
        request_id,
        cs_mail_finance::RequestFinancials::new(
            capture.clone(),
            RequestFinancialContract {
                refund_id: request_payment_id(
                    terms.financial.scope,
                    terms.relationship,
                    request_id,
                    true,
                ),
                processing_charge: terms.processing_charge,
                collateral: terms.collateral,
                terms: terms.financial,
                provider_key: terms.payment_provider_key,
            },
        )
        .map_err(payment_error)?,
    );
    manifest.next.requests.insert(
        request_id,
        RelationshipRequest {
            id: request_id,
            generation,
            initial_message: message_id,
            funding: capture.id,
            terms: terms.clone(),
            created_at: context.now,
            lifecycle: RequestLifecycle::PreparingSubmission(SubmissionPreparation {
                deadline: submission_deadline,
            }),
            version: RequestVersion::default(),
        },
    );
    manifest.schedules.push(ScheduleChange::Schedule {
        task: ScheduleTask::SubmissionTimeout(request_id),
        at: submission_deadline,
    });
    manifest.event(ProtocolEventKind::RequestCreated(request_id));
    Ok(())
}

fn validate_request_terms(
    manifest: &ManifestBuilder,
    context: &TransitionContext,
    terms: &RequestTerms,
) -> Result<CanonicalTime, ProtocolError> {
    if terms.issued_at > context.now
        || terms.expires_at < terms.issued_at
        || context.now > terms.expires_at
    {
        return Err(ProtocolError::QuoteExpired);
    }
    if terms.protocol_version != context.protocol_version
        || terms.unit != context.policy.unit
        || terms.relationship != manifest.next.relationship.key.reference
        || terms.request_history != manifest.history.reference
        || terms.sender != manifest.next.relationship.key.sender
        || terms.recipient != manifest.next.relationship.key.recipient
        || terms.recipient_provider != context.policy.recipient_provider
        || terms.request_level != manifest.history.level
        || terms.eligibility_time != manifest.history.earliest_next_submission
    {
        return Err(ProtocolError::AmountMismatch);
    }
    if terms.relationship_version != manifest.next.relationship.version
        || terms.history_version != manifest.history.version
    {
        return Err(ProtocolError::QuoteVersionStale);
    }
    let submission_deadline = context
        .now
        .checked_add(terms.submission_window)
        .ok_or(ProtocolError::ArithmeticOverflow)?;

    Ok(submission_deadline)
}

#[derive(Clone, Copy)]
struct RequestSubmissionInput {
    request_id: RequestId,
    expected_version: Version,
    content_ref: ContentRef,
    delivery_intent_ref: DeliveryIntentRef,
    declaration_digest: MessageDeclarationDigest,
    message_valid_until: MessageValidityUntil,
}

fn submit_request_to_recipient(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    submission: RequestSubmissionInput,
) -> Result<(), ProtocolError> {
    let request_id = submission.request_id;
    if manifest.next.relationship.state == RelationshipState::Blocked {
        return Err(ProtocolError::ContactBlocked);
    }
    let mut request = manifest
        .next
        .requests
        .get(&request_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if request.version != submission.expected_version.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if !request.lifecycle.is_preparing_submission() {
        return if matches!(request.lifecycle, RequestLifecycle::Cancelled { .. }) {
            Err(ProtocolError::RequestSubmissionCancelled)
        } else {
            Err(ProtocolError::AlreadyTerminal)
        };
    }
    if let Some(reason) = submission_failure(manifest, &request, submission)? {
        cancel_submission(manifest, request_id, reason)?;
        return Ok(());
    }
    if context.admission.is_err() {
        cancel_submission(manifest, request_id, ReservedCancellation::Ordinary)?;
        return Ok(());
    }
    validate_submission_authority(manifest, context, &request)?;
    let deadline = context
        .now
        .checked_add(request.terms.decision_window)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    request.lifecycle = RequestLifecycle::AwaitingRecipientDecision(RequestSubmission {
        at: context.now,
        decision_deadline: deadline,
    });
    if manifest.messages.contains_key(&request.initial_message) {
        return Err(ProtocolError::DuplicateConflict);
    }
    manifest.messages.insert(
        request.initial_message,
        Message {
            id: request.initial_message,
            relationship: request.terms.relationship,
            content: submission.content_ref,
            delivery: submission.delivery_intent_ref,
            declaration: submission.declaration_digest,
            valid_until: submission.message_valid_until,
            admitted_at: context.now,
            basis: AdmissionBasis::InitialRequest {
                request: request_id,
            },
        },
    );
    request.version = next_request_version(request.version)?;
    let backoff = request.terms.next_request_backoff;
    manifest.history.record_submission(
        request.reservation(),
        context.now,
        backoff,
        manifest.event,
    )?;
    let generation = request.generation;
    let message_id = request.initial_message;
    manifest.next.requests.insert(request_id, request);

    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::SubmissionTimeout(request_id),
    });
    manifest.schedules.push(ScheduleChange::Schedule {
        task: ScheduleTask::RequestExpiry(request_id),
        at: deadline,
    });
    manifest.effects.push(EffectIntent::DeliverMessage {
        message_id,
        content_ref: submission.content_ref,
        delivery_intent_ref: submission.delivery_intent_ref,
    });
    manifest
        .effects
        .push(EffectIntent::EstablishRequestSolicitation {
            request_id,
            generation,
        });
    manifest.event(ProtocolEventKind::RequestSubmitted(request_id));
    Ok(())
}

/// Classifies why a preparation must be cancelled before any submission effect is created.
fn submission_failure(
    manifest: &ManifestBuilder,
    request: &RelationshipRequest,
    submission: RequestSubmissionInput,
) -> Result<Option<ReservedCancellation>, ProtocolError> {
    let preparation = request
        .lifecycle
        .submission_preparation()
        .ok_or(ProtocolError::InvalidState)?;
    Ok(
        if manifest.next.relationship.state == RelationshipState::Accepted
            || manifest.now > preparation.deadline
        {
            Some(ReservedCancellation::Ordinary)
        } else if manifest.now > submission.message_valid_until.0 {
            Some(ReservedCancellation::MessageValidityClosed)
        } else if request
            .terms
            .declaration_digest
            .is_some_and(|expected| expected != submission.declaration_digest)
        {
            Some(ReservedCancellation::DeclarationMismatch)
        } else if request.terms.history_version != manifest.history.version
            || manifest.now < manifest.history.earliest_next_submission
        {
            Some(ReservedCancellation::HistoryChanged)
        } else {
            None
        },
    )
}

fn cancel_request_submission(
    manifest: &mut ManifestBuilder,
    actor: ActorRef,
    request_id: RequestId,
    expected_version: Version,
    reason: CancellationReason,
) -> Result<(), ProtocolError> {
    let request = manifest
        .next
        .requests
        .get(&request_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if !request.lifecycle.is_preparing_submission() {
        return if matches!(actor, ActorRef::Scheduler(_)) {
            Ok(())
        } else {
            Err(ProtocolError::AlreadyTerminal)
        };
    }
    if request.version != expected_version.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if reason == CancellationReason::SubmissionTimeout
        && manifest.now
            < request
                .lifecycle
                .submission_preparation()
                .ok_or(ProtocolError::InvalidState)?
                .deadline
    {
        return Err(ProtocolError::InvalidState);
    }
    cancel_submission(manifest, request_id, ReservedCancellation::Ordinary)
}

#[derive(Clone, Copy)]
enum ReservedCancellation {
    HistoryChanged,
    Ordinary,
    MessageValidityClosed,
    DeclarationMismatch,
}

fn cancel_submission(
    manifest: &mut ManifestBuilder,
    request_id: RequestId,
    cancellation: ReservedCancellation,
) -> Result<(), ProtocolError> {
    let mut request = manifest
        .next
        .requests
        .get(&request_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if !request.lifecycle.is_preparing_submission() {
        return Err(ProtocolError::AlreadyTerminal);
    }
    request.lifecycle = RequestLifecycle::Cancelled {
        submission_preparation: request
            .lifecycle
            .submission_preparation()
            .ok_or(ProtocolError::InvalidState)?,
        event: manifest.event,
    };
    request.version = next_request_version(request.version)?;
    if manifest.history.pending_submission == Some(request.reservation()) {
        manifest.history.pending_submission = None;
    }
    settle_financials(manifest, request.id, RequestSettlement::Cancelled)?;
    manifest.next.requests.insert(request.id, request.clone());
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::SubmissionTimeout(request.id),
    });
    manifest.event(match cancellation {
        ReservedCancellation::HistoryChanged => {
            ProtocolEventKind::RequestHistoryChanged(request.id)
        }
        ReservedCancellation::Ordinary => ProtocolEventKind::RequestSubmissionCancelled(request.id),
        ReservedCancellation::MessageValidityClosed => {
            ProtocolEventKind::MessageValidityClosed(request.id)
        }
        ReservedCancellation::DeclarationMismatch => {
            ProtocolEventKind::DeclarationMismatch(request.id)
        }
    });
    Ok(())
}

#[derive(Clone, Copy)]
enum Decision {
    Accept,
    Reject,
    Block,
}

fn relationship_decision(
    manifest: &mut ManifestBuilder,
    expected_version: Version,
    decision: Decision,
) -> Result<(), ProtocolError> {
    if manifest.next.relationship.version != expected_version.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if matches!(decision, Decision::Reject)
        && manifest.next.relationship.state == RelationshipState::Accepted
    {
        return Err(ProtocolError::InvalidState);
    }
    if !matches!(decision, Decision::Block)
        && manifest.next.relationship.state == RelationshipState::Blocked
    {
        return Err(ProtocolError::ContactBlocked);
    }
    if matches!(decision, Decision::Accept)
        && manifest.next.relationship.state == RelationshipState::Accepted
    {
        return Err(ProtocolError::InvalidState);
    }

    let target = match decision {
        Decision::Accept => RelationshipState::Accepted,
        Decision::Reject => RelationshipState::Rejected,
        Decision::Block => RelationshipState::Blocked,
    };
    manifest.next.relationship.state = target;
    manifest.next.relationship.version =
        next_relationship_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;

    if let Some(request) = manifest.next.active_request().cloned() {
        let id = request.id;
        match request.lifecycle {
            RequestLifecycle::PreparingSubmission(_) => {
                cancel_submission(manifest, id, ReservedCancellation::Ordinary)?;
            }
            RequestLifecycle::AwaitingRecipientDecision(submission) => {
                let deadline = submission.decision_deadline;
                if manifest.now <= deadline {
                    match decision {
                        Decision::Accept => settle_accepted(manifest, id)?,
                        Decision::Reject | Decision::Block => settle_rejected(manifest, id)?,
                    }
                } else {
                    settle_expired(manifest, id)?;
                }
            }
            RequestLifecycle::Accepted { .. }
            | RequestLifecycle::Rejected { .. }
            | RequestLifecycle::Expired { .. }
            | RequestLifecycle::Cancelled { .. } => {}
        }
    }

    manifest.event(match decision {
        Decision::Accept => ProtocolEventKind::RelationshipAccepted,
        Decision::Reject => ProtocolEventKind::RelationshipRejected,
        Decision::Block => ProtocolEventKind::RelationshipBlocked,
    });
    Ok(())
}

fn settle_accepted(manifest: &mut ManifestBuilder, id: RequestId) -> Result<(), ProtocolError> {
    let mut request = manifest.next.requests[&id].clone();
    if !request.lifecycle.is_awaiting_recipient_decision() {
        return Err(ProtocolError::InvalidState);
    }
    settle_financials(manifest, id, RequestSettlement::Accepted)?;
    request.lifecycle = RequestLifecycle::Accepted {
        submission: request
            .lifecycle
            .submission()
            .ok_or(ProtocolError::InvalidState)?,
        event: manifest.event,
    };
    request.version = next_request_version(request.version)?;
    manifest.next.requests.insert(id, request);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::RequestExpiry(id),
    });
    Ok(())
}

fn settle_rejected(manifest: &mut ManifestBuilder, id: RequestId) -> Result<(), ProtocolError> {
    let mut request = manifest.next.requests[&id].clone();
    if !request.lifecycle.is_awaiting_recipient_decision() {
        return Err(ProtocolError::InvalidState);
    }
    settle_financials(manifest, id, RequestSettlement::Rejected)?;
    apply_cooldown(manifest, manifest.now, request.terms.rejection_cooldown)?;
    request.lifecycle = RequestLifecycle::Rejected {
        submission: request
            .lifecycle
            .submission()
            .ok_or(ProtocolError::InvalidState)?,
        event: manifest.event,
    };
    request.version = next_request_version(request.version)?;
    manifest.next.requests.insert(id, request);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::RequestExpiry(id),
    });
    Ok(())
}

fn settle_expired(manifest: &mut ManifestBuilder, id: RequestId) -> Result<(), ProtocolError> {
    let mut request = manifest.next.requests[&id].clone();
    if !request.lifecycle.is_awaiting_recipient_decision() {
        return Err(ProtocolError::InvalidState);
    }
    settle_financials(manifest, id, RequestSettlement::Expired)?;
    apply_cooldown(
        manifest,
        request
            .lifecycle
            .submission()
            .ok_or(ProtocolError::InvalidState)?
            .decision_deadline,
        request.terms.expiry_cooldown,
    )?;
    request.lifecycle = RequestLifecycle::Expired {
        submission: request
            .lifecycle
            .submission()
            .ok_or(ProtocolError::InvalidState)?,
        event: manifest.event,
    };
    request.version = next_request_version(request.version)?;
    manifest.next.requests.insert(id, request);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::RequestExpiry(id),
    });
    Ok(())
}

fn unblock(manifest: &mut ManifestBuilder, expected: Version) -> Result<(), ProtocolError> {
    if manifest.next.relationship.version != expected.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if manifest.next.relationship.state != RelationshipState::Blocked {
        return Err(ProtocolError::InvalidState);
    }
    manifest.next.relationship.state = RelationshipState::Rejected;
    manifest.next.relationship.version =
        next_relationship_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;
    manifest.event(ProtocolEventKind::RelationshipUnblocked);
    Ok(())
}

fn revoke(manifest: &mut ManifestBuilder, expected: Version) -> Result<(), ProtocolError> {
    if manifest.next.relationship.version != expected.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if manifest.next.relationship.state != RelationshipState::Accepted {
        return Err(ProtocolError::InvalidState);
    }
    manifest.next.relationship.state = RelationshipState::Revoked;
    manifest.next.relationship.version =
        next_relationship_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;
    manifest.event(ProtocolEventKind::RelationshipRevoked);
    Ok(())
}

fn expire_request(
    manifest: &mut ManifestBuilder,
    request_id: RequestId,
    expected: Version,
) -> Result<(), ProtocolError> {
    let request = manifest
        .next
        .requests
        .get(&request_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if request.lifecycle.is_terminal() {
        return Ok(());
    }
    if request.version != expected.into() {
        return Err(ProtocolError::VersionConflict);
    }
    if !request.lifecycle.is_awaiting_recipient_decision() {
        return Err(ProtocolError::InvalidState);
    }
    let deadline = request
        .lifecycle
        .submission()
        .ok_or(ProtocolError::InvalidState)?
        .decision_deadline;
    if manifest.now <= deadline {
        return Err(ProtocolError::DecisionWindowClosed);
    }
    settle_expired(manifest, request_id)?;
    manifest.event(ProtocolEventKind::RequestExpired(request_id));
    Ok(())
}

fn next_version(version: Version) -> Result<Version, ProtocolError> {
    version
        .checked_next()
        .ok_or(ProtocolError::ArithmeticOverflow)
}

fn next_relationship_version(
    version: RelationshipVersion,
) -> Result<RelationshipVersion, ProtocolError> {
    version
        .checked_next()
        .ok_or(ProtocolError::ArithmeticOverflow)
}

fn next_request_version(version: RequestVersion) -> Result<RequestVersion, ProtocolError> {
    version
        .checked_next()
        .ok_or(ProtocolError::ArithmeticOverflow)
}

fn apply_cooldown(
    manifest: &mut ManifestBuilder,
    from: CanonicalTime,
    duration: Duration,
) -> Result<(), ProtocolError> {
    manifest
        .history
        .extend_cooldown(from, duration, manifest.now, manifest.event)
}
fn apply_financial_effects(
    manifest: &mut ManifestBuilder,
    id: RequestId,
    effects: RequestFinancialEffects,
) {
    for posting in effects.postings.transfers() {
        manifest
            .batch
            .transfer(posting.from, posting.to, posting.amount);
    }
    manifest
        .effects
        .extend(
            effects
                .operations
                .into_iter()
                .map(|operation_id| EffectIntent::ExecutePayment {
                    request_id: id,
                    operation_id,
                }),
        );
    manifest.forfeitures.extend(effects.forfeitures);
    manifest.forfeiture_holds.extend(effects.holds);
}
fn payment_error(error: cs_mail_finance::PaymentError) -> ProtocolError {
    match error {
        cs_mail_finance::PaymentError::DuplicateConflict => ProtocolError::DuplicateConflict,
        _ => ProtocolError::PaymentInvalid,
    }
}
fn settle_financials(
    manifest: &mut ManifestBuilder,
    id: RequestId,
    settlement: RequestSettlement,
) -> Result<(), ProtocolError> {
    let effects = manifest
        .payments
        .get_mut(&id)
        .ok_or(ProtocolError::IncompleteSnapshot)?
        .settle(settlement, manifest.now)
        .map_err(payment_error)?;
    apply_financial_effects(manifest, id, effects);
    Ok(())
}
fn record_payment(
    manifest: &mut ManifestBuilder,
    id: RequestId,
    receipt: &SignedPaymentEvidence,
) -> Result<(), ProtocolError> {
    let effects = manifest
        .payments
        .get_mut(&id)
        .ok_or(ProtocolError::MissingRecord)?
        .record_payment(receipt)
        .map_err(payment_error)?;
    if effects.capture_voided
        && let Some(request) = manifest.next.requests.get_mut(&id)
        && let RequestLifecycle::PreparingSubmission(preparation) = request.lifecycle
    {
        request.lifecycle = RequestLifecycle::Cancelled {
            submission_preparation: preparation,
            event: manifest.event,
        };
        request.version = next_request_version(request.version)?;
        if manifest.history.pending_submission == Some(request.reservation()) {
            manifest.history.pending_submission = None;
        }
        manifest.schedules.push(ScheduleChange::Cancel {
            task: ScheduleTask::SubmissionTimeout(id),
        });
    }
    apply_financial_effects(manifest, id, effects);
    manifest.event(ProtocolEventKind::PaymentRecorded(id));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn admit_followup(
    manifest: &mut ManifestBuilder,
    id: RequestId,
    message_id: MessageId,
    content_ref: ContentRef,
    delivery_intent_ref: DeliveryIntentRef,
    digest: MessageDeclarationDigest,
    valid_until: MessageValidityUntil,
    expected_policy: Version,
    admission: Result<(), admission::AdmissionFailure>,
) -> Result<(), ProtocolError> {
    if manifest.next.relationship.state == RelationshipState::Blocked {
        return Err(ProtocolError::ContactBlocked);
    }
    let policy = manifest.next.followup_policy;
    if policy.version != expected_policy {
        return Err(ProtocolError::VersionConflict);
    }
    let request = manifest
        .next
        .requests
        .get_mut(&id)
        .ok_or(ProtocolError::MissingRecord)?;
    if !request.lifecycle.is_awaiting_recipient_decision()
        || request
            .lifecycle
            .submission()
            .is_none_or(|a| manifest.now > a.decision_deadline)
    {
        return Err(ProtocolError::DecisionWindowClosed);
    }
    if manifest.now > valid_until.0 {
        return Err(ProtocolError::MessageValidityClosed);
    }
    admission.map_err(ProtocolError::AdmissionRefused)?;
    if message_id == request.initial_message || manifest.messages.contains_key(&message_id) {
        return Err(ProtocolError::DuplicateConflict);
    }
    let followups: Vec<_> = manifest
        .messages
        .values()
        .filter(
            |m| matches!(m.basis, AdmissionBasis::RequestFollowup { request, .. } if request == id),
        )
        .collect();
    let recent = followups
        .iter()
        .filter(|m| manifest.now.0.saturating_sub(m.admitted_at.0) < policy.interval.0)
        .count();
    if policy.max_messages == 0
        || followups.len() >= policy.max_messages as usize
        || recent >= policy.max_per_interval as usize
    {
        return Err(ProtocolError::FollowupNotAllowed);
    }
    manifest.messages.insert(
        message_id,
        Message {
            id: message_id,
            relationship: request.terms.relationship,
            content: content_ref,
            delivery: delivery_intent_ref,
            declaration: digest,
            valid_until,
            admitted_at: manifest.now,
            basis: AdmissionBasis::RequestFollowup {
                request: id,
                policy_version: policy.version,
            },
        },
    );
    manifest.effects.push(EffectIntent::DeliverMessage {
        message_id,
        content_ref,
        delivery_intent_ref,
    });
    manifest.event(ProtocolEventKind::FollowupAdmitted(id, message_id));
    Ok(())
}

fn validate_submission_authority(
    manifest: &ManifestBuilder,
    context: &TransitionContext,
    request: &RelationshipRequest,
) -> Result<(), ProtocolError> {
    if !manifest.payments[&request.id].funding_finalized()
        || manifest.payments[&request.id].capture_reversed()
    {
        return Err(ProtocolError::PaymentNotConfirmed);
    }
    if context.now < request.terms.eligibility_time {
        return Err(ProtocolError::BackoffActive {
            next_eligible: request.terms.eligibility_time,
        });
    }
    if manifest.history.pending_submission != Some(request.reservation())
        || request.terms.relationship_version != manifest.next.relationship.version
    {
        return Err(ProtocolError::QuoteVersionStale);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::PrincipalRef;
    #[test]
    fn incomplete_snapshots_are_refused_before_any_transition() {
        let state = ProtocolState::initial(
            PrincipalRef(1),
            ProtocolIdentity(10),
            ProtocolIdentity(20),
            CanonicalTime(0),
        );
        let snapshot = SettlementSnapshot::incomplete(
            0,
            state,
            cs_mail_ledger::LedgerState::new(SettlementUnit(1)).view(),
        );
        let policy = PolicySnapshot {
            pricing_policy_version: cs_mail_primitives::PolicyVersion(1),
            protocol_version: ProtocolVersion(2),
            policy_version: PolicyVersion(1),
            privacy_profile_version: PrivacyProfileVersion(1),
            retention_policy_version: RetentionPolicyVersion(1),
            recipient_provider: ProviderRef(30),
            unit: SettlementUnit(1),
            processing_charge: Money::from_minor_units(2),
            collateral: Money::from_minor_units(8),
            submission_window: Duration(10),
            decision_window: Duration(50),
            quote_lifetime: Duration(20),
            backoff: vec![Duration(0)],
            financial: FinancialTerms {
                scope: cs_mail_finance::FinancialScope::new(
                    [7; 32],
                    cs_mail_primitives::ProviderRef(30),
                    cs_mail_primitives::ProgramRef(1),
                    [9; 32],
                    cs_mail_primitives::ProtocolVersion(2),
                ),
                policy_version: PolicyVersion(1),
                corporate_basis_points: 300,
                maturity_delay: Duration(10),
            },
            payment_provider_key: [1; 32],
            expiry_cooldown: Duration(30),
            rejection_cooldown: Duration(90),
        };
        let context = TransitionContext {
            admission: Ok(()),
            now: CanonicalTime(1),
            journal_position: JournalPosition(1),
            protocol_version: ProtocolVersion(2),
            policy,
        };
        let command = KernelCommand::new(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(1),
                declaration_digest: None,
            },
            ActorRef::Sender(ProtocolIdentity(10)),
            OperationalKeyRef(1),
            IdempotencyKey(1),
        );
        assert_eq!(
            transition(&snapshot, &command, &context),
            Err(ProtocolError::IncompleteSnapshot)
        );
    }
}

pub mod admission;
pub mod pricing;
