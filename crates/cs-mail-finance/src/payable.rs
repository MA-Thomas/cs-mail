//! One annual allocation, one bank payment. Rebate and excess are reporting values.
use crate::{
    DistributionTerms, FinancialScope, PaymentChange, PaymentExecution, PaymentKind,
    PaymentOperation, PaymentState, ProgramError, SignedPaymentEvidence, VerifiedBankAccount,
};
use cs_mail_ledger::{Account, LedgerBatch};
use cs_mail_primitives::{
    AllocationId, AnnualDistributionId, CanonicalTime, FinancialEventId, MemberId, Money,
    PaymentOperationId, SettlementUnit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredPayable", into = "StoredPayable")]
pub struct MemberPayable {
    pub(crate) id: AllocationId,
    pub(crate) member: MemberId,
    pub(crate) distribution: AnnualDistributionId,
    pub(crate) amount: Money,
    terms: DistributionTerms,
    scope: FinancialScope,
    unit: SettlementUnit,
    payment: Option<PaymentExecution>,
    pub(crate) correction: Option<FinancialEventId>,
}
#[derive(Deserialize, Serialize)]
struct StoredPayable {
    id: AllocationId,
    member: MemberId,
    distribution: AnnualDistributionId,
    amount: Money,
    terms: DistributionTerms,
    scope: FinancialScope,
    unit: SettlementUnit,
    payment: Option<PaymentExecution>,
    correction: Option<FinancialEventId>,
}
impl From<MemberPayable> for StoredPayable {
    fn from(v: MemberPayable) -> Self {
        Self {
            id: v.id,
            member: v.member,
            distribution: v.distribution,
            amount: v.amount,
            terms: v.terms,
            scope: v.scope,
            unit: v.unit,
            payment: v.payment,
            correction: v.correction,
        }
    }
}
impl TryFrom<StoredPayable> for MemberPayable {
    type Error = ProgramError;
    fn try_from(v: StoredPayable) -> Result<Self, Self::Error> {
        let result = Self {
            id: v.id,
            member: v.member,
            distribution: v.distribution,
            amount: v.amount,
            terms: v.terms,
            scope: v.scope,
            unit: v.unit,
            payment: v.payment,
            correction: v.correction,
        };
        result.validate(result.scope, result.unit)?;
        Ok(result)
    }
}
impl MemberPayable {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: AllocationId,
        member: MemberId,
        distribution: AnnualDistributionId,
        amount: Money,
        terms: DistributionTerms,
        scope: FinancialScope,
        unit: SettlementUnit,
        correction: Option<FinancialEventId>,
    ) -> Result<Self, ProgramError> {
        let value = Self {
            id,
            member,
            distribution,
            amount,
            terms,
            scope,
            unit,
            payment: None,
            correction,
        };
        value.validate(scope, unit)?;
        Ok(value)
    }
    pub const fn id(&self) -> AllocationId {
        self.id
    }
    pub const fn member(&self) -> MemberId {
        self.member
    }
    pub const fn distribution(&self) -> AnnualDistributionId {
        self.distribution
    }
    pub const fn amount(&self) -> Money {
        self.amount
    }
    pub const fn correction(&self) -> Option<FinancialEventId> {
        self.correction
    }
    pub fn terms(&self) -> &DistributionTerms {
        &self.terms
    }
    pub fn payment(&self) -> Option<&PaymentExecution> {
        self.payment.as_ref()
    }
    pub fn rebate(&self) -> Money {
        self.amount.min(self.terms.utility_charge())
    }
    pub fn excess(&self) -> Money {
        Money::from_minor_units(self.amount.minor_units() - self.rebate().minor_units())
    }
    pub fn outstanding(&self) -> Money {
        if self
            .payment
            .as_ref()
            .is_some_and(|p| matches!(p.state(), PaymentState::Settled { .. }))
        {
            Money::ZERO
        } else {
            self.amount
        }
    }
    pub fn pending(&self) -> impl Iterator<Item = &PaymentOperation> {
        self.payment
            .as_ref()
            .and_then(PaymentExecution::pending)
            .into_iter()
    }
    pub(crate) fn prepare(
        &mut self,
        bank: &VerifiedBankAccount,
        at: CanonicalTime,
    ) -> Result<Option<PaymentOperation>, ProgramError> {
        let evidence = bank.evidence();
        if evidence.scope != self.scope
            || evidence.unit != self.unit
            || evidence.member != self.member
        {
            return Err(ProgramError::InvalidPayment);
        }
        if at < self.terms.due_at() {
            return Err(ProgramError::TooEarly);
        }
        if let Some(payment) = &mut self.payment {
            if payment.current().destination != evidence.bank_token {
                return Err(ProgramError::InvalidPayment);
            }
            return match payment.state() {
                PaymentState::Pending => Ok(Some(payment.current().clone())),
                PaymentState::Failed { .. } => {
                    let id = payout_id(self.scope, self.id, payment.attempt_count());
                    Ok(Some(payment.retry(id)?))
                }
                PaymentState::Settled { .. } => Ok(None),
                _ => Err(ProgramError::InvalidPayment),
            };
        }
        if self.amount.is_zero() {
            return Ok(None);
        }
        let operation = PaymentOperation {
            scope: self.scope,
            id: payout_id(self.scope, self.id, 0),
            kind: PaymentKind::MemberPayout {
                allocation: self.id,
            },
            amount: self.amount,
            unit: self.unit,
            destination: evidence.bank_token,
        };
        self.payment = Some(PaymentExecution::new(operation.clone())?);
        Ok(Some(operation))
    }
    pub(crate) fn record_receipt(
        &mut self,
        receipt: &SignedPaymentEvidence,
        key: &[u8; 32],
    ) -> Result<LedgerBatch, ProgramError> {
        let payment = self.payment.as_mut().ok_or(ProgramError::InvalidPayment)?;
        let mut batch = LedgerBatch::new();
        match payment.record(receipt, key)? {
            PaymentChange::Settled => batch.transfer(
                Account::MemberPayable(self.id),
                Account::ProcessorClearing,
                self.amount,
            ),
            PaymentChange::Reversed => batch.transfer(
                Account::ProcessorClearing,
                Account::MemberPayable(self.id),
                self.amount,
            ),
            PaymentChange::None => {}
        }
        Ok(batch)
    }
    /// # Errors
    /// Rejects inconsistent identifiers, amounts, operation purposes, or evidence history.
    pub fn validate(
        &self,
        scope: FinancialScope,
        unit: SettlementUnit,
    ) -> Result<(), ProgramError> {
        let year = u16::try_from(self.distribution.0).map_err(|_| ProgramError::InvalidPolicy)?;
        let cutoff = cs_mail_primitives::calendar_date(
            year.checked_add(1).ok_or(ProgramError::InvalidPolicy)?,
            1,
            1,
        )
        .ok_or(ProgramError::InvalidPolicy)?;
        if self.id.0 == 0
            || self.member.0 == 0
            || self.scope != scope
            || self.unit != unit
            || self.terms.due_at() < cutoff
        {
            return Err(ProgramError::InvalidPayment);
        }
        if let Some(payment) = &self.payment {
            payment.validate()?;
            let op = payment.current();
            if op.scope != scope
                || op.unit != unit
                || op.amount != self.amount
                || op.kind
                    != (PaymentKind::MemberPayout {
                        allocation: self.id,
                    })
            {
                return Err(ProgramError::InvalidPayment);
            }
        }
        Ok(())
    }
}
fn payout_id(
    scope: FinancialScope,
    allocation: AllocationId,
    attempt: usize,
) -> PaymentOperationId {
    let mut h = Sha256::new();
    h.update(b"cs-mail/distribution-payment/v1");
    h.update(scope.canonical_bytes());
    h.update(allocation.0.to_be_bytes());
    h.update((attempt as u128).to_be_bytes());
    let digest = h.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    PaymentOperationId(u128::from_be_bytes(bytes))
}
