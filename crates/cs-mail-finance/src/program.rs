use crate::membership::Member;
use crate::{
    Forfeiture, ForfeitureLot, LotLifecycle, MemberPayable, PayableLifecycle, QuarterAllocation,
    QuarterSchedule,
};
use crate::{PaymentError, PaymentOperation, SignedPaymentEvidence};
use cs_mail_ledger::{Account, LedgerBatch, LedgerError, LedgerState};
use cs_mail_primitives::{
    AllocationId, CanonicalTime, Duration, FinancialEventId, MemberId, Money, PaymentOperationId,
    PolicyVersion, QuarterId, SettlementUnit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const DAY_MILLIS: u64 = 86_400_000;
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FinancialTerms {
    pub scope: crate::FinancialScope,
    pub policy_version: PolicyVersion,
    pub corporate_basis_points: u16,
    pub maturity_delay: Duration,
}
impl FinancialTerms {
    /// # Errors
    /// Rejects a corporate rate greater than the entire eligible amount.
    pub fn validate(self) -> Result<(), ProgramError> {
        if self.corporate_basis_points > 10_000 {
            Err(ProgramError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MembershipStatus {
    pub opted_in: bool,
    pub verified: bool,
    pub suspended: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum IntentionalActivity {
    Read,
    Send,
    Decision,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProgramError {
    InvalidPolicy,
    DuplicateConflict,
    MissingRecord,
    TooEarly,
    ClosedPeriod,
    TimeRegression,
    InsufficientEvidence,
    InvalidPayment,
    ArithmeticOverflow,
    Ledger(LedgerError),
    Payment(PaymentError),
}
impl From<LedgerError> for ProgramError {
    fn from(e: LedgerError) -> Self {
        Self::Ledger(e)
    }
}
impl From<PaymentError> for ProgramError {
    fn from(e: PaymentError) -> Self {
        Self::Payment(e)
    }
}
impl core::fmt::Display for ProgramError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProgramError {}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProgramJournalEntry {
    pub at: CanonicalTime,
    pub cause: FinancialEventId,
    pub batch: LedgerBatch,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FinancialProgram {
    pub scope: crate::FinancialScope,
    pub unit: SettlementUnit,
    pub revision: u64,
    ledger: LedgerState,
    records: ProgramRecords,
    journal: Vec<ProgramJournalEntry>,
    last_event_at: CanonicalTime,
}
/// Explicit storage boundary: independent records, not an opaque whole-program document.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProgramRecords {
    pub funding: BTreeMap<PaymentOperationId, ForfeitureLot>,
    pub members: BTreeMap<MemberId, Member>,
    pub schedules: BTreeMap<QuarterId, QuarterSchedule>,
    pub quarters: BTreeMap<QuarterId, QuarterAllocation>,
    pub payables: BTreeMap<AllocationId, MemberPayable>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProgramMetadata {
    pub scope: crate::FinancialScope,
    pub unit: SettlementUnit,
    pub revision: u64,
    pub last_event_at: CanonicalTime,
}
impl FinancialProgram {
    pub fn new(scope: crate::FinancialScope, unit: SettlementUnit) -> Self {
        Self {
            scope,
            unit,
            revision: 0,
            ledger: LedgerState::new(unit),
            records: ProgramRecords::default(),
            journal: Vec::new(),
            last_event_at: CanonicalTime(0),
        }
    }
    pub fn metadata(&self) -> ProgramMetadata {
        ProgramMetadata {
            scope: self.scope,
            unit: self.unit,
            revision: self.revision,
            last_event_at: self.last_event_at,
        }
    }
    pub const fn records(&self) -> &ProgramRecords {
        &self.records
    }
    /// Assembles selected owners under the adapter's program lock. Journal history is
    /// loaded only for an audit snapshot; transitions start with an empty new-entry list.
    /// # Errors
    /// Rejects invalid ledger balances or a mismatched unit.
    pub fn restore(
        metadata: ProgramMetadata,
        records: ProgramRecords,
        ledger: cs_mail_ledger::LedgerView,
        journal: Vec<ProgramJournalEntry>,
    ) -> Result<Self, ProgramError> {
        if ledger.unit != metadata.unit {
            return Err(ProgramError::InvalidPolicy);
        }
        ledger.total_value()?;
        Ok(Self {
            scope: metadata.scope,
            unit: metadata.unit,
            revision: metadata.revision,
            last_event_at: metadata.last_event_at,
            records,
            ledger: LedgerState::from_view(ledger),
            journal,
        })
    }
    pub fn ledger(&self) -> cs_mail_ledger::LedgerView {
        self.ledger.view()
    }
    pub fn payables(&self) -> impl Iterator<Item = &MemberPayable> {
        self.records.payables.values()
    }
    pub fn quarter(&self, id: QuarterId) -> Option<&QuarterAllocation> {
        self.records.quarters.get(&id)
    }
    pub fn journal(&self) -> &[ProgramJournalEntry] {
        &self.journal
    }
    fn atomic<T>(
        &mut self,
        at: CanonicalTime,
        f: impl FnOnce(&mut Self) -> Result<T, ProgramError>,
    ) -> Result<T, ProgramError> {
        if at < self.last_event_at {
            return Err(ProgramError::TimeRegression);
        }
        let mut next = self.clone();
        let result = f(&mut next)?;
        next.last_event_at = at;
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        *self = next;
        Ok(result)
    }
    fn post(
        &mut self,
        at: CanonicalTime,
        cause: FinancialEventId,
        batch: LedgerBatch,
    ) -> Result<(), ProgramError> {
        self.ledger.apply(self.ledger.view().revision, &batch)?;
        self.journal.push(ProgramJournalEntry { at, cause, batch });
        Ok(())
    }
    /// Ingests a contribution exported atomically by a terminal request. Exact replay is harmless.
    /// # Errors
    /// Rejects conflicting contributions, units, terms, or timing.
    pub fn record_forfeiture(
        &mut self,
        source: Forfeiture,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        if let Some(f) = self.records.funding.get(&source.id) {
            return if f.source == source {
                Ok(())
            } else {
                Err(ProgramError::DuplicateConflict)
            };
        }
        self.atomic(at.max(self.last_event_at), |p| {
            source.terms.validate()?;
            if source.terms.scope != p.scope || source.unit != p.unit || source.forfeited_at > at {
                return Err(ProgramError::InvalidPolicy);
            }
            let mut batch = LedgerBatch::new();
            batch.transfer(
                Account::ProgramClearing(source.id),
                Account::PendingForfeiture(source.id),
                source.amount,
            );
            p.post(at, FinancialEventId(source.id.0), batch)?;
            let hold = source.requires_review;
            p.records.funding.insert(
                source.id,
                ForfeitureLot {
                    source,
                    lifecycle: LotLifecycle::Pending { held: hold },
                },
            );
            Ok(())
        })
    }
    /// Records a financial review hold; assessed funds cannot be silently clawed back.
    /// # Errors
    /// Requires an unassessed contribution and canonical ordering.
    pub fn set_hold(
        &mut self,
        id: PaymentOperationId,
        hold: bool,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            let f = p
                .records
                .funding
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?;
            f.set_hold(hold)?;
            Ok(())
        })
    }
    /// An authorized review attests that settlement and refund/dispute reserves are resolved.
    /// # Errors
    /// Rejects held, immature, already assessed, or unidentified evidence.
    pub fn clear_maturity(
        &mut self,
        id: PaymentOperationId,
        evidence: FinancialEventId,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            let f = p
                .records
                .funding
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?;
            f.clear(evidence, at)?;
            Ok(())
        })
    }
    /// Enrolls one verified-person identity digest; aliases cannot create additional membership.
    /// # Errors
    /// Rejects duplicate identifiers, duplicate identities, or backdated membership.
    pub fn enroll(
        &mut self,
        id: MemberId,
        identity_digest: [u8; 32],
        status: MembershipStatus,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            if identity_digest == [0; 32]
                || p.records.members.contains_key(&id)
                || p.records
                    .members
                    .values()
                    .any(|m| m.identity_digest == identity_digest)
            {
                return Err(ProgramError::DuplicateConflict);
            }
            p.records.members.insert(
                id,
                Member {
                    id,
                    identity_digest,
                    joined_at: at,
                    changes: vec![(at, status)],
                    activity_days: BTreeSet::new(),
                },
            );
            Ok(())
        })
    }
    /// # Errors
    /// Requires an enrolled member and canonical ordering.
    pub fn set_membership(
        &mut self,
        id: MemberId,
        status: MembershipStatus,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            p.records
                .members
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?
                .changes
                .push((at, status));
            Ok(())
        })
    }
    /// Records only the UTC day of authenticated intentional use, never correspondents or content.
    /// # Errors
    /// Requires an enrolled member and non-backdated evidence.
    pub fn record_activity(
        &mut self,
        id: MemberId,
        _kind: IntentionalActivity,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            p.records
                .members
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?
                .activity_days
                .insert(at.0 / DAY_MILLIS);
            Ok(())
        })
    }
    /// Publishes an immutable calendar and eligibility policy before the period begins.
    /// # Errors
    /// Rejects late publication, overlapping quarters, and changed definitions.
    pub fn publish_quarter(
        &mut self,
        schedule: QuarterSchedule,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        if let Some(old) = self.records.schedules.get(&schedule.id) {
            return if old == &schedule {
                Ok(())
            } else {
                Err(ProgramError::DuplicateConflict)
            };
        }
        self.atomic(at, |p| {
            if !schedule.is_calendar_quarter()
                || at > schedule.start
                || schedule.start >= schedule.cutoff
                || schedule.eligibility.minimum_active_days == 0
                || p.records
                    .schedules
                    .values()
                    .any(|s| s.start < schedule.cutoff && schedule.start < s.cutoff)
            {
                return Err(ProgramError::InvalidPolicy);
            }
            p.records.schedules.insert(schedule.id, schedule);
            Ok(())
        })
    }
    /// Freezes funding, equal allocations, assessment marks, and carryforward atomically.
    /// # Errors
    /// Rejects early or out-of-order closure and arithmetic failures.
    pub fn finalize_quarter(
        &mut self,
        id: QuarterId,
        at: CanonicalTime,
    ) -> Result<QuarterAllocation, ProgramError> {
        if let Some(q) = self.records.quarters.get(&id) {
            return Ok(q.clone());
        }
        self.atomic(at, |p| {
            let schedule = p
                .records
                .schedules
                .get(&id)
                .cloned()
                .ok_or(ProgramError::MissingRecord)?;
            if at < schedule.cutoff {
                return Err(ProgramError::TooEarly);
            }
            if p.records
                .schedules
                .values()
                .any(|s| s.cutoff <= schedule.start && !p.records.quarters.contains_key(&s.id))
            {
                return Err(ProgramError::ClosedPeriod);
            }
            let members = p.qualifying_members(&schedule);
            let funding: Vec<_> = p
                .records
                .funding
                .values()
                .filter(|f| f.eligible_at(schedule.cutoff))
                .map(|f| f.source.id)
                .collect();
            let mut batch = LedgerBatch::new();
            let (new, corporate, contribution) =
                p.assess_funding(&funding, &schedule, &mut batch)?;
            let available = p
                .ledger
                .balance(Account::RestrictedMemberFunds)
                .checked_add(contribution)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            let n = u64::try_from(members.len()).map_err(|_| ProgramError::ArithmeticOverflow)?;
            let each = Money::from_minor_units(if n == 0 {
                0
            } else {
                available.minor_units() / n
            });
            let remainder = Money::from_minor_units(if n == 0 {
                available.minor_units()
            } else {
                available.minor_units() % n
            });
            if !each.is_zero() {
                for member in &members {
                    let allocation = allocation_id(p.scope, p.unit, schedule.id, *member);
                    if p.records.payables.contains_key(&allocation) {
                        return Err(ProgramError::DuplicateConflict);
                    }
                    p.records.payables.insert(
                        allocation,
                        MemberPayable {
                            id: allocation,
                            member: *member,
                            quarter: schedule.id,
                            amount: each,
                            lifecycle: PayableLifecycle::Due,
                            previous_payments: Vec::new(),
                            events: BTreeMap::new(),
                            correction: None,
                        },
                    );
                    batch.transfer(
                        Account::RestrictedMemberFunds,
                        Account::MemberPayable(allocation),
                        each,
                    );
                }
            }
            p.post(at, FinancialEventId(schedule.id.0), batch)?;
            let result = QuarterAllocation {
                schedule,
                finalized_at: at,
                members,
                funding,
                newly_eligible: new,
                corporate_share: corporate,
                member_contribution: contribution,
                each,
                remainder,
            };
            p.records.quarters.insert(id, result.clone());
            Ok(result)
        })
    }
    /// Fixes a verified payout destination. Below-threshold obligations remain payable.
    /// # Errors
    /// Rejects missing payables, unverified destinations, or conflicting retries.
    pub fn prepare_payout(
        &mut self,
        id: AllocationId,
        destination: [u8; 32],
        minimum: Money,
        at: CanonicalTime,
    ) -> Result<Option<PaymentOperation>, ProgramError> {
        self.atomic(at, |p| {
            let payable = p
                .records
                .payables
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?;
            payable.prepare(p.scope, p.unit, destination, minimum)
        })
    }
    /// Discharges a fixed payable only after verified provider confirmation.
    /// # Errors
    /// Rejects unauthenticated, mismatched, or unsupported evidence.
    pub fn confirm_payout(
        &mut self,
        id: AllocationId,
        receipt: &SignedPaymentEvidence,
        key: &[u8; 32],
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        let previous = self
            .records
            .payables
            .get(&id)
            .ok_or(ProgramError::MissingRecord)?;
        let mut next = previous.clone();
        let batch = next.record_receipt(receipt, key)?;
        if &next == previous {
            return Ok(());
        }
        self.atomic(at, |p| {
            p.records.payables.insert(id, next);
            if !batch.transfers().is_empty() {
                p.post(at, receipt.evidence.event_id, batch)?;
            }
            Ok(())
        })
    }
}
fn allocation_id(
    scope: crate::FinancialScope,
    unit: SettlementUnit,
    quarter: QuarterId,
    member: MemberId,
) -> AllocationId {
    let mut h = Sha256::new();
    h.update(b"cs-mail/member-allocation/v2");
    h.update(scope.canonical_bytes());
    h.update(unit.0.to_be_bytes());
    h.update(quarter.0.to_be_bytes());
    h.update(member.0.to_be_bytes());
    let hash = h.finalize();
    let mut b = [0; 16];
    b.copy_from_slice(&hash[..16]);
    AllocationId(u128::from_be_bytes(b))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProgramCommand {
    CompensateMember {
        event: FinancialEventId,
        member: MemberId,
        quarter: QuarterId,
        amount: Money,
        reason: String,
    },
    Enroll {
        member: MemberId,
        identity_digest: [u8; 32],
        status: MembershipStatus,
    },
    SetMembership {
        member: MemberId,
        status: MembershipStatus,
    },
    RecordActivity {
        member: MemberId,
        activity: IntentionalActivity,
    },
    PublishQuarter(QuarterSchedule),
    Hold {
        source: PaymentOperationId,
        hold: bool,
    },
    ClearMaturity {
        source: PaymentOperationId,
        evidence: FinancialEventId,
    },
    FinalizeQuarter(QuarterId),
    PreparePayout {
        allocation: AllocationId,
        destination: [u8; 32],
        minimum: Money,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProgramOutcome {
    Recorded,
    Quarter(QuarterAllocation),
    Payout(Option<PaymentOperation>),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedProgramCommand {
    pub scope: crate::FinancialScope,
    pub unit: SettlementUnit,
    pub idempotency_key: cs_mail_primitives::IdempotencyKey,
    pub expected_revision: u64,
    pub command: ProgramCommand,
    pub signature: Vec<u8>,
}
impl SignedProgramCommand {
    fn bytes(&self) -> Result<Vec<u8>, ProgramError> {
        serde_json::to_vec(&(
            "cs-mail/financial-command/v2",
            self.scope,
            self.unit,
            self.idempotency_key,
            self.expected_revision,
            &self.command,
        ))
        .map_err(|_| ProgramError::InvalidPolicy)
    }
    /// Signs a command at the separately authorized program administration boundary.
    /// # Errors
    /// Rejects commands that cannot be canonically serialized.
    pub fn sign(
        scope: crate::FinancialScope,
        unit: SettlementUnit,
        idempotency_key: cs_mail_primitives::IdempotencyKey,
        expected_revision: u64,
        command: ProgramCommand,
        secret: &[u8; 32],
    ) -> Result<Self, ProgramError> {
        use ed25519_dalek::Signer;
        let mut result = Self {
            scope,
            unit,
            idempotency_key,
            expected_revision,
            command,
            signature: Vec::new(),
        };
        result.signature = ed25519_dalek::SigningKey::from_bytes(secret)
            .sign(&result.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(result)
    }
    /// # Errors
    /// Rejects invalid administration signatures.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), ProgramError> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(key)
            .map_err(|_| ProgramError::InvalidPayment)?;
        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|_| ProgramError::InvalidPayment)?;
        key.verify_strict(&self.bytes()?, &signature)
            .map_err(|_| ProgramError::InvalidPayment)
    }
}
impl FinancialProgram {
    /// Executes already-authorized program administration at canonical receipt time.
    /// # Errors
    /// Returns the applicable membership, cutoff, maturity, or accounting error.
    pub fn apply(
        &mut self,
        command: &ProgramCommand,
        at: CanonicalTime,
    ) -> Result<ProgramOutcome, ProgramError> {
        match command {
            ProgramCommand::CompensateMember {
                event,
                member,
                quarter,
                amount,
                reason,
            } => self.compensate_member(*event, *member, *quarter, *amount, reason, at)?,
            ProgramCommand::Enroll {
                member,
                identity_digest,
                status,
            } => self.enroll(*member, *identity_digest, *status, at)?,
            ProgramCommand::SetMembership { member, status } => {
                self.set_membership(*member, *status, at)?;
            }
            ProgramCommand::RecordActivity { member, activity } => {
                self.record_activity(*member, *activity, at)?;
            }
            ProgramCommand::PublishQuarter(schedule) => {
                self.publish_quarter(schedule.clone(), at)?;
            }
            ProgramCommand::Hold { source, hold } => self.set_hold(*source, *hold, at)?,
            ProgramCommand::ClearMaturity { source, evidence } => {
                self.clear_maturity(*source, *evidence, at)?;
            }
            ProgramCommand::FinalizeQuarter(id) => {
                return self.finalize_quarter(*id, at).map(ProgramOutcome::Quarter);
            }
            ProgramCommand::PreparePayout {
                allocation,
                destination,
                minimum,
            } => {
                return self
                    .prepare_payout(*allocation, *destination, *minimum, at)
                    .map(ProgramOutcome::Payout);
            }
        }
        Ok(ProgramOutcome::Recorded)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AllocationStatus {
    Allocated,
    AwaitingPayment,
    Confirmed,
    AwaitingReconciliation,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemberStatementEntry {
    pub quarter: QuarterId,
    pub amount: Money,
    pub status: AllocationStatus,
    pub correction: Option<FinancialEventId>,
}
impl FinancialProgram {
    pub fn member_statement(&self, member: MemberId) -> Vec<MemberStatementEntry> {
        self.records
            .payables
            .values()
            .filter(|p| p.member == member)
            .map(|p| MemberStatementEntry {
                quarter: p.quarter,
                amount: p.amount,
                status: match p.lifecycle {
                    PayableLifecycle::Due => AllocationStatus::Allocated,
                    PayableLifecycle::Pending(_) => AllocationStatus::AwaitingPayment,
                    PayableLifecycle::Paid(_) => AllocationStatus::Confirmed,
                    PayableLifecycle::Reversed(_) => AllocationStatus::AwaitingReconciliation,
                },
                correction: p.correction,
            })
            .collect()
    }
    /// Adds a separately authorized, company-funded correction without rewriting a quarter.
    /// # Errors
    /// Requires an existing member, finalized quarter, unique cause, and a bounded explanation.
    #[allow(clippy::too_many_arguments)]
    pub fn compensate_member(
        &mut self,
        event: FinancialEventId,
        member: MemberId,
        quarter: QuarterId,
        amount: Money,
        reason: &str,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at, |p| {
            if event.0 == 0
                || amount.is_zero()
                || reason.trim().is_empty()
                || reason.len() > 2000
                || !p.records.members.contains_key(&member)
                || !p.records.quarters.contains_key(&quarter)
            {
                return Err(ProgramError::InvalidPolicy);
            }
            let mut h = Sha256::new();
            h.update(b"cs-mail/member-correction/v2");
            h.update(p.scope.canonical_bytes());
            h.update(p.unit.0.to_be_bytes());
            h.update(event.0.to_be_bytes());
            let digest = h.finalize();
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&digest[..16]);
            let id = AllocationId(u128::from_be_bytes(bytes));
            if p.records.payables.contains_key(&id) {
                return Err(ProgramError::DuplicateConflict);
            }
            p.records.payables.insert(
                id,
                MemberPayable {
                    id,
                    member,
                    quarter,
                    amount,
                    lifecycle: PayableLifecycle::Due,
                    previous_payments: Vec::new(),
                    events: BTreeMap::new(),
                    correction: Some(event),
                },
            );
            let mut batch = LedgerBatch::new();
            batch.transfer(
                Account::CorporateLossClearing,
                Account::MemberPayable(id),
                amount,
            );
            p.post(at, event, batch)
        })
    }
}

impl FinancialProgram {
    fn qualifying_members(&self, schedule: &QuarterSchedule) -> Vec<MemberId> {
        self.records
            .members
            .values()
            .filter(|m| m.qualifies(schedule))
            .map(|m| m.id)
            .collect()
    }
    fn assess_funding(
        &mut self,
        funding: &[PaymentOperationId],
        schedule: &QuarterSchedule,
        batch: &mut LedgerBatch,
    ) -> Result<(Money, Money, Money), ProgramError> {
        let mut new = Money::ZERO;
        let mut corporate = Money::ZERO;
        // Round once per immutable rate cohort, leaving fractional corporate units for members.
        let mut cohorts: BTreeMap<PolicyVersion, (u16, Money)> = BTreeMap::new();
        for id in funding {
            let f = self
                .records
                .funding
                .get_mut(id)
                .ok_or(ProgramError::MissingRecord)?;
            let group = cohorts
                .entry(f.source.terms.policy_version)
                .or_insert((f.source.terms.corporate_basis_points, Money::ZERO));
            if group.0 != f.source.terms.corporate_basis_points {
                return Err(ProgramError::InvalidPolicy);
            }
            group.1 = group
                .1
                .checked_add(f.source.amount)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            new = new
                .checked_add(f.source.amount)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            batch.transfer(
                Account::PendingForfeiture(*id),
                Account::RestrictedMemberFunds,
                f.source.amount,
            );
            f.assess(schedule.id)?;
        }
        for (rate, amount) in cohorts.values() {
            let share = u128::from(amount.minor_units()) * u128::from(*rate) / 10_000;
            corporate = corporate
                .checked_add(Money::from_minor_units(
                    u64::try_from(share).map_err(|_| ProgramError::ArithmeticOverflow)?,
                ))
                .ok_or(ProgramError::ArithmeticOverflow)?;
        }
        let contribution = new
            .checked_sub(corporate)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        batch.transfer(
            Account::RestrictedMemberFunds,
            Account::CorporatePoolRevenue,
            corporate,
        );

        Ok((new, corporate, contribution))
    }
}

impl FinancialProgram {
    /// Provider reversal evidence blocks an unassessed lot without clawing back member rights.
    /// # Errors
    /// Requires a known contribution and ordered financial processing.
    pub fn note_reversal(
        &mut self,
        id: PaymentOperationId,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        self.atomic(at.max(self.last_event_at), |p| {
            let funding = p
                .records
                .funding
                .get_mut(&id)
                .ok_or(ProgramError::MissingRecord)?;
            if !matches!(funding.lifecycle, LotLifecycle::Assessed { .. }) {
                funding.set_hold(true)?;
            }
            Ok(())
        })
    }
}
