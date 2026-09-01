//! The deterministic cs-mail protocol kernel.
//!
//! `transition` performs no I/O. It consumes a complete snapshot and an
//! authenticated command, then returns the entire atomic consequence.

use std::collections::{BTreeMap, BTreeSet};

use cs_mail_ledger::{Account, LedgerBatch, LedgerError, LedgerView};
use cs_mail_primitives::{
    AttemptId, BondId, CanonicalTime, ContentRef, DeliveryIntentRef, Duration, EpisodeId, EventRef,
    IdempotencyKey, JournalPosition, LaneId, MessageId, Money, OperationalKeyRef,
    PersistenceReserveId, PolicyVersion, PrincipalRef, ProtocolIdentity, ProtocolVersion,
    ProviderRef, QuoteId, SettlementUnit, Version,
};
pub use cs_mail_primitives::{ScheduleChange, ScheduleTask};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RelationshipKey {
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum RelationshipState {
    Unknown,
    Accepted,
    Rejected,
    Revoked,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Relationship {
    pub key: RelationshipKey,
    pub state: RelationshipState,
    pub version: Version,
    pub last_event: Option<EventRef>,
    pub changed_at: CanonicalTime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepeatedAttemptState {
    pub principal: PrincipalRef,
    pub recipient: ProtocolIdentity,
    pub level: u32,
    pub earliest_next_admission: CanonicalTime,
    pub version: Version,
    pub last_event: Option<EventRef>,
    pub changed_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum BondState {
    Reserved,
    Admitted,
    Accepted { event: EventRef },
    Rejected { event: EventRef },
    Expired { event: EventRef },
    CancelledUnadmitted { event: EventRef },
}

impl BondState {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Reserved | Self::Admitted)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ContactTerms {
    pub quote_id: QuoteId,
    pub protocol_version: ProtocolVersion,
    pub policy_version: PolicyVersion,
    pub principal: PrincipalRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub recipient_provider: ProviderRef,
    pub relationship_version: Version,
    pub attempt_version: Version,
    pub processing_charge: Money,
    pub collateral: Money,
    pub persistence: Money,
    pub attempt_level: u32,
    pub eligibility_time: CanonicalTime,
    pub unit: SettlementUnit,
    pub admission_window: Duration,
    pub decision_window: Duration,
    pub persistence_release_at: CanonicalTime,
    pub issued_at: CanonicalTime,
    pub expires_at: CanonicalTime,
}

impl ContactTerms {
    /// Returns the `C + S` bond portion of the reservation.
    ///
    /// # Errors
    ///
    /// Returns `ArithmeticOverflow` if the quoted values cannot be added.
    pub fn bond_amount(&self) -> Result<Money, ProtocolError> {
        self.processing_charge
            .checked_add(self.collateral)
            .ok_or(ProtocolError::ArithmeticOverflow)
    }

    /// Returns the complete `C + S + L_k` reservation.
    ///
    /// # Errors
    ///
    /// Returns `ArithmeticOverflow` if the quoted values cannot be added.
    pub fn total_reservation(&self) -> Result<Money, ProtocolError> {
        self.bond_amount()?
            .checked_add(self.persistence)
            .ok_or(ProtocolError::ArithmeticOverflow)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Bond {
    pub id: BondId,
    pub reserve_id: PersistenceReserveId,
    pub attempt_id: AttemptId,
    pub message_id: MessageId,
    pub terms: ContactTerms,
    pub created_at: CanonicalTime,
    pub admission_deadline: CanonicalTime,
    pub admitted_at: Option<CanonicalTime>,
    pub decision_deadline: Option<CanonicalTime>,
    pub content_ref: Option<ContentRef>,
    pub delivery_intent_ref: Option<DeliveryIntentRef>,
    pub state: BondState,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ReserveState {
    Reserved,
    Released { event: EventRef },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistenceReserve {
    pub id: PersistenceReserveId,
    pub bond_id: BondId,
    pub principal: PrincipalRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub amount: Money,
    pub created_at: CanonicalTime,
    pub release_at: CanonicalTime,
    pub state: ReserveState,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum SolicitationStatus {
    Active,
    Closed,
    Lapsed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SolicitationEpisode {
    pub id: EpisodeId,
    pub relationship: RelationshipKey,
    pub generation: u64,
    pub attempts: BTreeSet<AttemptId>,
    pub opened_at: CanonicalTime,
    pub status: SolicitationStatus,
    pub version: Version,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolState {
    pub relationship: Relationship,
    pub attempt: RepeatedAttemptState,
    pub bonds: BTreeMap<BondId, Bond>,
    pub reserves: BTreeMap<PersistenceReserveId, PersistenceReserve>,
    pub solicitation: Option<SolicitationEpisode>,
}

impl ProtocolState {
    pub fn initial(
        principal: PrincipalRef,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Self {
        Self {
            relationship: Relationship {
                key: RelationshipKey { sender, recipient },
                state: RelationshipState::Unknown,
                version: Version::default(),
                last_event: None,
                changed_at: now,
            },
            attempt: RepeatedAttemptState {
                principal,
                recipient,
                level: 0,
                earliest_next_admission: now,
                version: Version::default(),
                last_event: None,
                changed_at: now,
            },
            bonds: BTreeMap::new(),
            reserves: BTreeMap::new(),
            solicitation: None,
        }
    }

    pub fn active_reserves(&self) -> impl Iterator<Item = &PersistenceReserve> {
        self.reserves
            .values()
            .filter(|reserve| matches!(reserve.state, ReserveState::Reserved))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettlementSnapshot {
    pub revision: u64,
    pub state: ProtocolState,
    pub ledger: LedgerView,
    complete: bool,
}

impl SettlementSnapshot {
    pub fn complete(revision: u64, state: ProtocolState, ledger: LedgerView) -> Self {
        Self {
            revision,
            state,
            ledger,
            complete: true,
        }
    }

    #[cfg(test)]
    fn incomplete(revision: u64, state: ProtocolState, ledger: LedgerView) -> Self {
        Self {
            revision,
            state,
            ledger,
            complete: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PolicySnapshot {
    pub protocol_version: ProtocolVersion,
    pub policy_version: PolicyVersion,
    pub recipient_provider: ProviderRef,
    pub unit: SettlementUnit,
    pub processing_charge: Money,
    pub collateral: Money,
    pub admission_window: Duration,
    pub decision_window: Duration,
    pub quote_lifetime: Duration,
    pub persistence_duration: Duration,
    pub backoff: Vec<Duration>,
    pub persistence: Vec<Money>,
}

impl PolicySnapshot {
    fn backoff_for(&self, level: u32) -> Duration {
        self.backoff
            .get(level as usize)
            .copied()
            .or_else(|| self.backoff.last().copied())
            .unwrap_or(Duration(0))
    }

    fn persistence_for(&self, level: u32) -> Money {
        self.persistence
            .get(level as usize)
            .copied()
            .or_else(|| self.persistence.last().copied())
            .unwrap_or(Money::ZERO)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.backoff.is_empty()
            || self.persistence.is_empty()
            || self.backoff[0] != Duration(0)
            || self.persistence[0] != Money::ZERO
            || !self.backoff.windows(2).all(|pair| pair[0] <= pair[1])
            || !self.persistence.windows(2).all(|pair| pair[0] <= pair[1])
        {
            return Err(ProtocolError::PolicyInvalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionContext {
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
pub struct Authorized<T> {
    command: T,
    actor: ActorRef,
    key: OperationalKeyRef,
    idempotency_key: IdempotencyKey,
}

impl<T> Authorized<T> {
    /// Constructs evidence at the security boundary. The first milestone does
    /// not yet implement signatures, so callers must make this trust assumption
    /// explicit.
    pub const fn assume_verified(
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
    AdmissionTimeout,
    PreAdmissionFailure,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ProtocolCommand {
    IssueContactTerms {
        quote_id: QuoteId,
    },
    ReserveAttempt {
        bond_id: BondId,
        reserve_id: PersistenceReserveId,
        attempt_id: AttemptId,
        message_id: MessageId,
        terms: ContactTerms,
    },
    AdmitAttempt {
        bond_id: BondId,
        expected_bond_version: Version,
        content_ref: ContentRef,
        delivery_intent_ref: DeliveryIntentRef,
    },
    CancelReservedAttempt {
        bond_id: BondId,
        expected_bond_version: Version,
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
    ExpireBond {
        bond_id: BondId,
        expected_bond_version: Version,
    },
    ReleasePersistenceReserve {
        reserve_id: PersistenceReserveId,
        expected_reserve_version: Version,
    },
    RevokeRelationship {
        expected_version: Version,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TermsOutcome {
    BondRequired(ContactTerms),
    NoBondRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum EffectIntent {
    DeliverMessage {
        message_id: MessageId,
        content_ref: ContentRef,
        delivery_intent_ref: DeliveryIntentRef,
    },
    EstablishRelationshipSolicitation {
        episode_id: EpisodeId,
    },
    ReviewLane {
        lane_id: LaneId,
        reconfirmation_required: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ProtocolEventKind {
    TermsIssued,
    AttemptReserved(BondId),
    AttemptAdmitted(BondId),
    ReservedAttemptCancelled(BondId),
    RelationshipAccepted,
    RelationshipRejected,
    RelationshipBlocked,
    RelationshipUnblocked,
    RelationshipRevoked,
    BondExpired(BondId),
    PersistenceReleased(PersistenceReserveId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProtocolEvent {
    pub reference: EventRef,
    pub at: CanonicalTime,
    pub kind: ProtocolEventKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionManifest {
    pub expected_snapshot_revision: u64,
    pub expected_ledger_revision: u64,
    pub next_state: ProtocolState,
    pub ledger_batch: LedgerBatch,
    pub schedule_changes: Vec<ScheduleChange>,
    pub protocol_events: Vec<ProtocolEvent>,
    pub outbox_intents: Vec<EffectIntent>,
    pub terms_outcome: Option<TermsOutcome>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProtocolError {
    AuthenticationFailed,
    QuoteExpired,
    QuoteVersionStale,
    RelationshipAccepted,
    ContactBlocked,
    BackoffActive { next_eligible: CanonicalTime },
    InsufficientFunds,
    AmountMismatch,
    AdmissionWindowClosed,
    ReservedAttemptCancelled,
    DecisionWindowClosed,
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
            expected_snapshot_revision: snapshot.revision,
            expected_ledger_revision: snapshot.ledger.revision,
            next_state: self.next,
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
    authorized: &Authorized<ProtocolCommand>,
    context: &TransitionContext,
) -> Result<TransitionManifest, ProtocolError> {
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

    let actor = authorized.actor();
    let command = authorized.command();
    authorize(
        &snapshot.state,
        actor,
        command,
        context.policy.recipient_provider,
    )?;
    let mut manifest = ManifestBuilder::new(snapshot, context);

    match command {
        ProtocolCommand::IssueContactTerms { quote_id } => {
            issue_terms(&mut manifest, context, *quote_id)?;
        }
        ProtocolCommand::ReserveAttempt {
            bond_id,
            reserve_id,
            attempt_id,
            message_id,
            terms,
        } => reserve_attempt(
            &mut manifest,
            context,
            *bond_id,
            *reserve_id,
            *attempt_id,
            *message_id,
            terms,
        )?,
        ProtocolCommand::AdmitAttempt {
            bond_id,
            expected_bond_version,
            content_ref,
            delivery_intent_ref,
        } => admit_attempt(
            &mut manifest,
            context,
            *bond_id,
            *expected_bond_version,
            *content_ref,
            *delivery_intent_ref,
        )?,
        ProtocolCommand::CancelReservedAttempt {
            bond_id,
            expected_bond_version,
            reason,
        } => cancel_reserved_attempt(
            &mut manifest,
            actor,
            *bond_id,
            *expected_bond_version,
            *reason,
        )?,
        ProtocolCommand::AcceptRelationship { expected_version } => {
            relationship_decision(&mut manifest, *expected_version, Decision::Accept)?;
        }
        ProtocolCommand::RejectRelationship { expected_version } => {
            relationship_decision(&mut manifest, *expected_version, Decision::Reject)?;
        }
        ProtocolCommand::BlockRelationship { expected_version } => {
            relationship_decision(&mut manifest, *expected_version, Decision::Block)?;
        }
        ProtocolCommand::UnblockRelationship { expected_version } => {
            unblock(&mut manifest, *expected_version)?;
        }
        ProtocolCommand::ExpireBond {
            bond_id,
            expected_bond_version,
        } => expire_bond(&mut manifest, *bond_id, *expected_bond_version)?,
        ProtocolCommand::ReleasePersistenceReserve {
            reserve_id,
            expected_reserve_version,
        } => release_persistence(&mut manifest, *reserve_id, *expected_reserve_version)?,
        ProtocolCommand::RevokeRelationship { expected_version } => {
            revoke(&mut manifest, *expected_version)?;
        }
    }

    manifest.finish(snapshot)
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
        ProtocolCommand::IssueContactTerms { .. }
        | ProtocolCommand::ReserveAttempt { .. }
        | ProtocolCommand::AdmitAttempt { .. } => actor == ActorRef::Sender(sender),
        ProtocolCommand::CancelReservedAttempt { reason, .. } => match reason {
            CancellationReason::SenderRequested => actor == ActorRef::Sender(sender),
            CancellationReason::AdmissionTimeout => actor == ActorRef::Scheduler(provider),
            CancellationReason::PreAdmissionFailure => actor == ActorRef::Provider(provider),
        },
        ProtocolCommand::AcceptRelationship { .. }
        | ProtocolCommand::RejectRelationship { .. }
        | ProtocolCommand::BlockRelationship { .. }
        | ProtocolCommand::UnblockRelationship { .. }
        | ProtocolCommand::RevokeRelationship { .. } => actor == ActorRef::Recipient(recipient),
        ProtocolCommand::ExpireBond { .. } | ProtocolCommand::ReleasePersistenceReserve { .. } => {
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
) -> Result<(), ProtocolError> {
    match manifest.next.relationship.state {
        RelationshipState::Accepted => {
            manifest.terms = Some(TermsOutcome::NoBondRequired);
        }
        RelationshipState::Blocked => return Err(ProtocolError::ContactBlocked),
        RelationshipState::Unknown | RelationshipState::Rejected | RelationshipState::Revoked => {
            let policy = &context.policy;
            let release_at = context
                .now
                .checked_add(policy.persistence_duration)
                .ok_or(ProtocolError::ArithmeticOverflow)?;
            let expires_at = context
                .now
                .checked_add(policy.quote_lifetime)
                .ok_or(ProtocolError::ArithmeticOverflow)?;
            let terms = ContactTerms {
                quote_id,
                protocol_version: context.protocol_version,
                policy_version: policy.policy_version,
                principal: manifest.next.attempt.principal,
                sender: manifest.next.relationship.key.sender,
                recipient: manifest.next.relationship.key.recipient,
                recipient_provider: policy.recipient_provider,
                relationship_version: manifest.next.relationship.version,
                attempt_version: manifest.next.attempt.version,
                processing_charge: policy.processing_charge,
                collateral: policy.collateral,
                persistence: policy.persistence_for(manifest.next.attempt.level),
                attempt_level: manifest.next.attempt.level,
                eligibility_time: manifest.next.attempt.earliest_next_admission,
                unit: policy.unit,
                admission_window: policy.admission_window,
                decision_window: policy.decision_window,
                persistence_release_at: release_at,
                issued_at: context.now,
                expires_at,
            };
            terms.total_reservation()?;
            manifest.terms = Some(TermsOutcome::BondRequired(terms));
        }
    }
    manifest.event(ProtocolEventKind::TermsIssued);
    Ok(())
}

fn reserve_attempt(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    bond_id: BondId,
    reserve_id: PersistenceReserveId,
    attempt_id: AttemptId,
    message_id: MessageId,
    terms: &ContactTerms,
) -> Result<(), ProtocolError> {
    match manifest.next.relationship.state {
        RelationshipState::Accepted => return Err(ProtocolError::RelationshipAccepted),
        RelationshipState::Blocked => return Err(ProtocolError::ContactBlocked),
        RelationshipState::Unknown | RelationshipState::Rejected | RelationshipState::Revoked => {}
    }
    let admission_deadline = validate_reservation_terms(manifest, context, terms)?;
    if manifest.next.bonds.contains_key(&bond_id)
        || manifest.next.reserves.contains_key(&reserve_id)
    {
        return Err(ProtocolError::DuplicateConflict);
    }
    let bond_amount = terms.bond_amount()?;
    manifest.batch.transfer(
        Account::Sender(terms.principal),
        Account::Bond(bond_id),
        bond_amount,
    );
    manifest.batch.transfer(
        Account::Sender(terms.principal),
        Account::PersistenceReserve(reserve_id),
        terms.persistence,
    );
    manifest.next.bonds.insert(
        bond_id,
        Bond {
            id: bond_id,
            reserve_id,
            attempt_id,
            message_id,
            terms: terms.clone(),
            created_at: context.now,
            admission_deadline,
            admitted_at: None,
            decision_deadline: None,
            content_ref: None,
            delivery_intent_ref: None,
            state: BondState::Reserved,
            version: Version::default(),
        },
    );
    manifest.next.reserves.insert(
        reserve_id,
        PersistenceReserve {
            id: reserve_id,
            bond_id,
            principal: terms.principal,
            sender: terms.sender,
            recipient: terms.recipient,
            amount: terms.persistence,
            created_at: context.now,
            release_at: terms.persistence_release_at,
            state: ReserveState::Reserved,
            version: Version::default(),
        },
    );
    manifest.schedules.push(ScheduleChange::Schedule {
        task: ScheduleTask::AdmissionTimeout(bond_id),
        at: admission_deadline,
    });
    manifest.schedules.push(ScheduleChange::Schedule {
        task: ScheduleTask::PersistenceRelease(reserve_id),
        at: terms.persistence_release_at,
    });
    manifest.event(ProtocolEventKind::AttemptReserved(bond_id));
    Ok(())
}

fn validate_reservation_terms(
    manifest: &ManifestBuilder,
    context: &TransitionContext,
    terms: &ContactTerms,
) -> Result<CanonicalTime, ProtocolError> {
    if terms.issued_at > context.now
        || terms.expires_at < terms.issued_at
        || context.now > terms.expires_at
    {
        return Err(ProtocolError::QuoteExpired);
    }
    if terms.protocol_version != context.protocol_version
        || terms.policy_version != context.policy.policy_version
        || terms.unit != context.policy.unit
        || terms.principal != manifest.next.attempt.principal
        || terms.sender != manifest.next.relationship.key.sender
        || terms.recipient != manifest.next.relationship.key.recipient
        || terms.recipient_provider != context.policy.recipient_provider
        || terms.processing_charge != context.policy.processing_charge
        || terms.collateral != context.policy.collateral
        || terms.persistence != context.policy.persistence_for(terms.attempt_level)
        || terms.admission_window != context.policy.admission_window
        || terms.decision_window != context.policy.decision_window
        || terms.attempt_level != manifest.next.attempt.level
        || terms.eligibility_time != manifest.next.attempt.earliest_next_admission
    {
        return Err(ProtocolError::AmountMismatch);
    }
    if terms.relationship_version != manifest.next.relationship.version
        || terms.attempt_version != manifest.next.attempt.version
    {
        return Err(ProtocolError::QuoteVersionStale);
    }
    let admission_deadline = context
        .now
        .checked_add(terms.admission_window)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    let latest_decision_deadline = admission_deadline
        .checked_add(terms.decision_window)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    if terms.persistence_release_at < latest_decision_deadline {
        return Err(ProtocolError::PolicyInvalid);
    }
    Ok(admission_deadline)
}

fn admit_attempt(
    manifest: &mut ManifestBuilder,
    context: &TransitionContext,
    bond_id: BondId,
    expected_version: Version,
    content_ref: ContentRef,
    delivery_intent_ref: DeliveryIntentRef,
) -> Result<(), ProtocolError> {
    if manifest.next.relationship.state == RelationshipState::Blocked {
        return Err(ProtocolError::ContactBlocked);
    }
    let mut bond = manifest
        .next
        .bonds
        .get(&bond_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if bond.version != expected_version {
        return Err(ProtocolError::VersionConflict);
    }
    if bond.state != BondState::Reserved {
        return if matches!(bond.state, BondState::CancelledUnadmitted { .. }) {
            Err(ProtocolError::ReservedAttemptCancelled)
        } else {
            Err(ProtocolError::AlreadyTerminal)
        };
    }
    if manifest.next.relationship.state == RelationshipState::Accepted {
        cancel_reserved(manifest, bond_id)?;
        return Ok(());
    }
    if context.now > bond.admission_deadline {
        cancel_reserved(manifest, bond_id)?;
        return Ok(());
    }
    if context.now < bond.terms.eligibility_time {
        return Err(ProtocolError::BackoffActive {
            next_eligible: bond.terms.eligibility_time,
        });
    }
    if bond.terms.attempt_version != manifest.next.attempt.version
        || bond.terms.relationship_version != manifest.next.relationship.version
        || bond.terms.policy_version != context.policy.policy_version
    {
        return Err(ProtocolError::QuoteVersionStale);
    }
    let deadline = context
        .now
        .checked_add(bond.terms.decision_window)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    bond.state = BondState::Admitted;
    bond.admitted_at = Some(context.now);
    bond.decision_deadline = Some(deadline);
    bond.content_ref = Some(content_ref);
    bond.delivery_intent_ref = Some(delivery_intent_ref);
    bond.version = next_version(bond.version)?;
    let attempt_id = bond.attempt_id;
    let message_id = bond.message_id;
    manifest.next.bonds.insert(bond_id, bond);

    let next_level = manifest
        .next
        .attempt
        .level
        .checked_add(1)
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    manifest.next.attempt.level = next_level;
    manifest.next.attempt.earliest_next_admission = context
        .now
        .checked_add(context.policy.backoff_for(next_level))
        .ok_or(ProtocolError::ArithmeticOverflow)?;
    manifest.next.attempt.version = next_version(manifest.next.attempt.version)?;
    manifest.next.attempt.last_event = Some(manifest.event);
    manifest.next.attempt.changed_at = context.now;

    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::AdmissionTimeout(bond_id),
    });
    manifest.schedules.push(ScheduleChange::Schedule {
        task: ScheduleTask::BondExpiry(bond_id),
        at: deadline,
    });
    manifest.effects.push(EffectIntent::DeliverMessage {
        message_id,
        content_ref,
        delivery_intent_ref,
    });
    open_or_join_solicitation(manifest, attempt_id)?;
    manifest.event(ProtocolEventKind::AttemptAdmitted(bond_id));
    Ok(())
}

fn open_or_join_solicitation(
    manifest: &mut ManifestBuilder,
    attempt_id: AttemptId,
) -> Result<(), ProtocolError> {
    if let Some(episode) = manifest.next.solicitation.as_mut()
        && episode.status == SolicitationStatus::Active
    {
        episode.attempts.insert(attempt_id);
        episode.version = next_version(episode.version)?;
        return Ok(());
    }
    let generation = manifest
        .next
        .solicitation
        .as_ref()
        .map_or(Ok(1), |episode| {
            episode
                .generation
                .checked_add(1)
                .ok_or(ProtocolError::ArithmeticOverflow)
        })?;
    let episode_id = EpisodeId(u128::from(generation));
    let mut attempts = BTreeSet::new();
    attempts.insert(attempt_id);
    manifest.next.solicitation = Some(SolicitationEpisode {
        id: episode_id,
        relationship: manifest.next.relationship.key,
        generation,
        attempts,
        opened_at: manifest.now,
        status: SolicitationStatus::Active,
        version: Version::default(),
    });
    manifest
        .effects
        .push(EffectIntent::EstablishRelationshipSolicitation { episode_id });
    Ok(())
}

fn cancel_reserved_attempt(
    manifest: &mut ManifestBuilder,
    actor: ActorRef,
    bond_id: BondId,
    expected_version: Version,
    reason: CancellationReason,
) -> Result<(), ProtocolError> {
    let bond = manifest
        .next
        .bonds
        .get(&bond_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if bond.state != BondState::Reserved {
        return if matches!(actor, ActorRef::Scheduler(_)) {
            Ok(())
        } else {
            Err(ProtocolError::AlreadyTerminal)
        };
    }
    if bond.version != expected_version {
        return Err(ProtocolError::VersionConflict);
    }
    if reason == CancellationReason::AdmissionTimeout && manifest.now < bond.admission_deadline {
        return Err(ProtocolError::InvalidState);
    }
    cancel_reserved(manifest, bond_id)
}

fn cancel_reserved(manifest: &mut ManifestBuilder, bond_id: BondId) -> Result<(), ProtocolError> {
    let mut bond = manifest
        .next
        .bonds
        .get(&bond_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if bond.state != BondState::Reserved {
        return Err(ProtocolError::AlreadyTerminal);
    }
    bond.state = BondState::CancelledUnadmitted {
        event: manifest.event,
    };
    bond.version = next_version(bond.version)?;
    let bond_amount = bond.terms.bond_amount()?;
    manifest.batch.transfer(
        Account::Bond(bond.id),
        Account::Sender(bond.terms.principal),
        bond_amount,
    );
    release_reserve(manifest, bond.reserve_id)?;
    manifest.next.bonds.insert(bond.id, bond.clone());
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::AdmissionTimeout(bond.id),
    });
    manifest.event(ProtocolEventKind::ReservedAttemptCancelled(bond.id));
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
    if manifest.next.relationship.version != expected_version {
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
    manifest.next.relationship.version = next_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;

    let key = manifest.next.relationship.key;
    let ids: Vec<_> = manifest
        .next
        .bonds
        .values()
        .filter(|bond| bond.terms.sender == key.sender && bond.terms.recipient == key.recipient)
        .map(|bond| bond.id)
        .collect();
    for id in ids {
        let state = manifest.next.bonds[&id].state;
        match state {
            BondState::Reserved => cancel_reserved(manifest, id)?,
            BondState::Admitted => {
                let deadline = manifest.next.bonds[&id]
                    .decision_deadline
                    .ok_or(ProtocolError::InvalidState)?;
                if manifest.now <= deadline {
                    match decision {
                        Decision::Accept => settle_accepted(manifest, id)?,
                        Decision::Reject | Decision::Block => settle_rejected(manifest, id)?,
                    }
                } else {
                    settle_expired(manifest, id)?;
                }
            }
            BondState::Accepted { .. }
            | BondState::Rejected { .. }
            | BondState::Expired { .. }
            | BondState::CancelledUnadmitted { .. } => {}
        }
    }

    if matches!(decision, Decision::Accept) {
        let key = manifest.next.relationship.key;
        let reserve_ids: Vec<_> = manifest
            .next
            .active_reserves()
            .filter(|reserve| reserve.sender == key.sender && reserve.recipient == key.recipient)
            .map(|reserve| reserve.id)
            .collect();
        for reserve_id in reserve_ids {
            release_reserve(manifest, reserve_id)?;
        }
    }
    close_solicitation(manifest)?;
    manifest.event(match decision {
        Decision::Accept => ProtocolEventKind::RelationshipAccepted,
        Decision::Reject => ProtocolEventKind::RelationshipRejected,
        Decision::Block => ProtocolEventKind::RelationshipBlocked,
    });
    Ok(())
}

fn settle_accepted(manifest: &mut ManifestBuilder, id: BondId) -> Result<(), ProtocolError> {
    let mut bond = manifest.next.bonds[&id].clone();
    if bond.state != BondState::Admitted {
        return Err(ProtocolError::InvalidState);
    }
    manifest.batch.transfer(
        Account::Bond(id),
        Account::Sender(bond.terms.principal),
        bond.terms.bond_amount()?,
    );
    bond.state = BondState::Accepted {
        event: manifest.event,
    };
    bond.version = next_version(bond.version)?;
    manifest.next.bonds.insert(id, bond);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::BondExpiry(id),
    });
    Ok(())
}

fn settle_rejected(manifest: &mut ManifestBuilder, id: BondId) -> Result<(), ProtocolError> {
    let mut bond = manifest.next.bonds[&id].clone();
    if bond.state != BondState::Admitted {
        return Err(ProtocolError::InvalidState);
    }
    manifest.batch.transfer(
        Account::Bond(id),
        Account::RecipientProvider(bond.terms.recipient_provider),
        bond.terms.processing_charge,
    );
    manifest.batch.transfer(
        Account::Bond(id),
        Account::Recipient(bond.terms.recipient),
        bond.terms.collateral,
    );
    bond.state = BondState::Rejected {
        event: manifest.event,
    };
    bond.version = next_version(bond.version)?;
    manifest.next.bonds.insert(id, bond);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::BondExpiry(id),
    });
    Ok(())
}

fn settle_expired(manifest: &mut ManifestBuilder, id: BondId) -> Result<(), ProtocolError> {
    let mut bond = manifest.next.bonds[&id].clone();
    if bond.state != BondState::Admitted {
        return Err(ProtocolError::InvalidState);
    }
    manifest.batch.transfer(
        Account::Bond(id),
        Account::RecipientProvider(bond.terms.recipient_provider),
        bond.terms.processing_charge,
    );
    manifest.batch.transfer(
        Account::Bond(id),
        Account::Sender(bond.terms.principal),
        bond.terms.collateral,
    );
    bond.state = BondState::Expired {
        event: manifest.event,
    };
    bond.version = next_version(bond.version)?;
    manifest.next.bonds.insert(id, bond);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::BondExpiry(id),
    });
    Ok(())
}

fn release_reserve(
    manifest: &mut ManifestBuilder,
    reserve_id: PersistenceReserveId,
) -> Result<(), ProtocolError> {
    let mut reserve = manifest
        .next
        .reserves
        .get(&reserve_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if !matches!(reserve.state, ReserveState::Reserved) {
        return Ok(());
    }
    manifest.batch.transfer(
        Account::PersistenceReserve(reserve_id),
        Account::Sender(reserve.principal),
        reserve.amount,
    );
    reserve.state = ReserveState::Released {
        event: manifest.event,
    };
    reserve.version = next_version(reserve.version)?;
    manifest.next.reserves.insert(reserve_id, reserve);
    manifest.schedules.push(ScheduleChange::Cancel {
        task: ScheduleTask::PersistenceRelease(reserve_id),
    });
    Ok(())
}

fn close_solicitation(manifest: &mut ManifestBuilder) -> Result<(), ProtocolError> {
    if let Some(episode) = manifest.next.solicitation.as_mut()
        && episode.status == SolicitationStatus::Active
    {
        episode.status = SolicitationStatus::Closed;
        episode.version = next_version(episode.version)?;
    }
    Ok(())
}

fn unblock(manifest: &mut ManifestBuilder, expected: Version) -> Result<(), ProtocolError> {
    if manifest.next.relationship.version != expected {
        return Err(ProtocolError::VersionConflict);
    }
    if manifest.next.relationship.state != RelationshipState::Blocked {
        return Err(ProtocolError::InvalidState);
    }
    manifest.next.relationship.state = RelationshipState::Rejected;
    manifest.next.relationship.version = next_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;
    manifest.event(ProtocolEventKind::RelationshipUnblocked);
    Ok(())
}

fn revoke(manifest: &mut ManifestBuilder, expected: Version) -> Result<(), ProtocolError> {
    if manifest.next.relationship.version != expected {
        return Err(ProtocolError::VersionConflict);
    }
    if manifest.next.relationship.state != RelationshipState::Accepted {
        return Err(ProtocolError::InvalidState);
    }
    manifest.next.relationship.state = RelationshipState::Revoked;
    manifest.next.relationship.version = next_version(manifest.next.relationship.version)?;
    manifest.next.relationship.last_event = Some(manifest.event);
    manifest.next.relationship.changed_at = manifest.now;
    manifest.event(ProtocolEventKind::RelationshipRevoked);
    Ok(())
}

fn expire_bond(
    manifest: &mut ManifestBuilder,
    bond_id: BondId,
    expected: Version,
) -> Result<(), ProtocolError> {
    let bond = manifest
        .next
        .bonds
        .get(&bond_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if bond.state.is_terminal() {
        return Ok(());
    }
    if bond.version != expected {
        return Err(ProtocolError::VersionConflict);
    }
    if bond.state != BondState::Admitted {
        return Err(ProtocolError::InvalidState);
    }
    let deadline = bond.decision_deadline.ok_or(ProtocolError::InvalidState)?;
    if manifest.now <= deadline {
        return Err(ProtocolError::DecisionWindowClosed);
    }
    settle_expired(manifest, bond_id)?;
    let key = manifest.next.relationship.key;
    let any_open = manifest.next.bonds.values().any(|candidate| {
        candidate.terms.sender == key.sender
            && candidate.terms.recipient == key.recipient
            && candidate.state == BondState::Admitted
            && candidate
                .decision_deadline
                .is_some_and(|time| manifest.now <= time)
    });
    if !any_open
        && let Some(episode) = manifest.next.solicitation.as_mut()
        && episode.status == SolicitationStatus::Active
    {
        episode.status = SolicitationStatus::Lapsed;
        episode.version = next_version(episode.version)?;
    }
    manifest.event(ProtocolEventKind::BondExpired(bond_id));
    Ok(())
}

fn release_persistence(
    manifest: &mut ManifestBuilder,
    reserve_id: PersistenceReserveId,
    expected: Version,
) -> Result<(), ProtocolError> {
    let reserve = manifest
        .next
        .reserves
        .get(&reserve_id)
        .cloned()
        .ok_or(ProtocolError::MissingRecord)?;
    if !matches!(reserve.state, ReserveState::Reserved) {
        return Ok(());
    }
    if reserve.version != expected {
        return Err(ProtocolError::VersionConflict);
    }
    if manifest.now < reserve.release_at {
        return Err(ProtocolError::InvalidState);
    }
    release_reserve(manifest, reserve_id)?;
    manifest.event(ProtocolEventKind::PersistenceReleased(reserve_id));
    Ok(())
}

fn next_version(version: Version) -> Result<Version, ProtocolError> {
    version
        .checked_next()
        .ok_or(ProtocolError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_ledger::LedgerState;

    fn fixture() -> (SettlementSnapshot, TransitionContext) {
        let principal = PrincipalRef(1);
        let sender = ProtocolIdentity(10);
        let recipient = ProtocolIdentity(20);
        let mut ledger = LedgerState::new(SettlementUnit(1));
        ledger
            .fund_for_test(Account::Sender(principal), Money::from_minor_units(1_000))
            .unwrap();
        let snapshot = SettlementSnapshot::complete(
            0,
            ProtocolState::initial(principal, sender, recipient, CanonicalTime(0)),
            ledger.view(),
        );
        let context = TransitionContext {
            now: CanonicalTime(1),
            journal_position: JournalPosition(1),
            protocol_version: ProtocolVersion(1),
            policy: PolicySnapshot {
                protocol_version: ProtocolVersion(1),
                policy_version: PolicyVersion(1),
                recipient_provider: ProviderRef(30),
                unit: SettlementUnit(1),
                processing_charge: Money::from_minor_units(2),
                collateral: Money::from_minor_units(8),
                admission_window: Duration(10),
                decision_window: Duration(20),
                quote_lifetime: Duration(5),
                persistence_duration: Duration(100),
                backoff: vec![Duration(0), Duration(10), Duration(20)],
                persistence: vec![Money::ZERO, Money::from_minor_units(5)],
            },
        };
        (snapshot, context)
    }

    #[test]
    fn incomplete_snapshots_are_rejected() {
        let (snapshot, context) = fixture();
        let snapshot =
            SettlementSnapshot::incomplete(snapshot.revision, snapshot.state, snapshot.ledger);
        let command = Authorized::assume_verified(
            ProtocolCommand::IssueContactTerms {
                quote_id: QuoteId(1),
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

    #[test]
    fn blocked_relationships_cannot_issue_terms() {
        let (mut snapshot, context) = fixture();
        snapshot.state.relationship.state = RelationshipState::Blocked;
        let command = Authorized::assume_verified(
            ProtocolCommand::IssueContactTerms {
                quote_id: QuoteId(1),
            },
            ActorRef::Sender(ProtocolIdentity(10)),
            OperationalKeyRef(1),
            IdempotencyKey(1),
        );
        assert_eq!(
            transition(&snapshot, &command, &context),
            Err(ProtocolError::ContactBlocked)
        );
    }

    #[test]
    fn acceptance_releases_only_the_accepted_public_identity_reserves() {
        let principal = PrincipalRef(1);
        let sender = ProtocolIdentity(10);
        let other_sender = ProtocolIdentity(11);
        let recipient = ProtocolIdentity(20);
        let mut state = ProtocolState::initial(principal, sender, recipient, CanonicalTime(0));
        for (id, public_identity) in [
            (PersistenceReserveId(1), sender),
            (PersistenceReserveId(2), other_sender),
        ] {
            state.reserves.insert(
                id,
                PersistenceReserve {
                    id,
                    bond_id: BondId(id.0),
                    principal,
                    sender: public_identity,
                    recipient,
                    amount: Money::from_minor_units(5),
                    created_at: CanonicalTime(0),
                    release_at: CanonicalTime(100),
                    state: ReserveState::Reserved,
                    version: Version(0),
                },
            );
        }
        state.attempt.level = 7;
        state.attempt.earliest_next_admission = CanonicalTime(50);
        let original_attempt = state.attempt.clone();
        let mut ledger = LedgerState::new(SettlementUnit(1));
        ledger
            .fund_for_test(
                Account::PersistenceReserve(PersistenceReserveId(1)),
                Money::from_minor_units(5),
            )
            .unwrap();
        ledger
            .fund_for_test(
                Account::PersistenceReserve(PersistenceReserveId(2)),
                Money::from_minor_units(5),
            )
            .unwrap();
        let snapshot = SettlementSnapshot::complete(0, state, ledger.view());
        let (_, context) = fixture();
        let command = Authorized::assume_verified(
            ProtocolCommand::AcceptRelationship {
                expected_version: Version(0),
            },
            ActorRef::Recipient(recipient),
            OperationalKeyRef(1),
            IdempotencyKey(1),
        );
        let manifest = transition(&snapshot, &command, &context).unwrap();
        assert!(matches!(
            manifest.next_state.reserves[&PersistenceReserveId(1)].state,
            ReserveState::Released { .. }
        ));
        assert!(matches!(
            manifest.next_state.reserves[&PersistenceReserveId(2)].state,
            ReserveState::Reserved
        ));
        assert_eq!(manifest.next_state.attempt, original_attempt);
        assert_eq!(manifest.ledger_batch.transfers().len(), 1);
    }
}
