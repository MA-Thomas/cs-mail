//! Financial execution outlives the request that caused it.
use crate::{
    FinancialTerms, Forfeiture, PaymentError, PaymentEvidence, PaymentKind, PaymentOperation,
    PaymentOutcome, SignedPaymentEvidence,
};
use cs_mail_ledger::{Account, LedgerBatch};
use cs_mail_primitives::{CanonicalTime, FinancialEventId, Money, PaymentOperationId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CaptureStatus {
    Pending,
    CancellationRequested,
    FundingFinalized,
    Voided,
    Reversed,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RefundStatus {
    None,
    Pending(PaymentOperation),
    Confirmed(PaymentOperation),
}
impl RefundStatus {
    pub const fn operation(&self) -> Option<&PaymentOperation> {
        match self {
            Self::None => None,
            Self::Pending(p) | Self::Confirmed(p) => Some(p),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredRequestFinancials", into = "StoredRequestFinancials")]
pub struct RequestFinancials {
    capture: PaymentOperation,
    capture_status: CaptureStatus,
    refund: RefundStatus,
    events: BTreeMap<FinancialEventId, PaymentEvidence>,
    contract: RequestFinancialContract,
    settlement: Option<RequestSettlement>,
}
impl RequestFinancials {
    pub fn capture(&self) -> &PaymentOperation {
        &self.capture
    }
    /// Creates a capture obligation from validated immutable financial terms.
    /// # Errors
    /// Rejects inconsistent amounts, scope, identifiers or provider authority.
    pub fn new(
        capture: PaymentOperation,
        contract: RequestFinancialContract,
    ) -> Result<Self, PaymentError> {
        capture.validate()?;
        if contract.terms.validate().is_err()
            || capture.kind != PaymentKind::Capture
            || capture.amount.is_zero()
            || capture.scope != contract.terms.scope
            || contract.processing_charge.checked_add(contract.collateral) != Some(capture.amount)
            || capture.id == contract.refund_id
            || contract.provider_key == [0; 32]
        {
            return Err(PaymentError::InvalidOperation);
        }
        Ok(Self {
            capture,
            contract,
            settlement: None,
            capture_status: CaptureStatus::Pending,
            refund: RefundStatus::None,
            events: BTreeMap::new(),
        })
    }
    pub const fn funding_finalized(&self) -> bool {
        matches!(
            self.capture_status,
            CaptureStatus::FundingFinalized | CaptureStatus::Reversed
        )
    }
    pub const fn capture_voided(&self) -> bool {
        matches!(self.capture_status, CaptureStatus::Voided)
    }
    pub const fn capture_reversed(&self) -> bool {
        matches!(self.capture_status, CaptureStatus::Reversed)
    }
    pub fn pending_payment(&self, id: PaymentOperationId) -> Option<crate::ProcessorRequest> {
        if id == self.capture.id
            && matches!(
                self.capture_status,
                CaptureStatus::Pending | CaptureStatus::CancellationRequested
            )
        {
            return if self.capture_status == CaptureStatus::CancellationRequested {
                crate::CaptureCancellation::new(self.capture.clone())
                    .ok()
                    .map(crate::ProcessorRequest::CancelCapture)
            } else {
                Some(crate::ProcessorRequest::Submit(self.capture.clone()))
            };
        }
        match &self.refund {
            RefundStatus::Pending(p) if p.id == id => {
                Some(crate::ProcessorRequest::Submit(p.clone()))
            }
            _ => None,
        }
    }
}

/// Immutable financial evidence; no message, principal or request lifecycle is needed
/// to reconcile an obligation after formation records have been deleted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestFinancialContract {
    pub refund_id: PaymentOperationId,
    pub processing_charge: Money,
    pub collateral: Money,
    pub terms: FinancialTerms,
    pub provider_key: [u8; 32],
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RequestSettlement {
    Accepted,
    Rejected,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestFinancialEffects {
    pub postings: LedgerBatch,
    pub operations: Vec<PaymentOperationId>,
    pub forfeitures: Vec<Forfeiture>,
    pub holds: Vec<PaymentOperationId>,
    pub capture_voided: bool,
}

impl RequestFinancials {
    pub const fn capture_status(&self) -> CaptureStatus {
        self.capture_status
    }
    pub const fn refund(&self) -> &RefundStatus {
        &self.refund
    }
    pub const fn settlement(&self) -> Option<RequestSettlement> {
        self.settlement
    }
    pub const fn provider_key(&self) -> &[u8; 32] {
        &self.contract.provider_key
    }
    pub fn operation(&self, id: PaymentOperationId) -> Option<&PaymentOperation> {
        if self.capture.id == id {
            Some(&self.capture)
        } else {
            self.refund.operation().filter(|o| o.id == id)
        }
    }
    /// Records an obligation, independently of provider completion. Failure is atomic.
    /// # Errors
    /// Rejects contradictory settlements or settlement before capture confirmation.
    pub fn settle(
        &mut self,
        settlement: RequestSettlement,
        at: CanonicalTime,
    ) -> Result<RequestFinancialEffects, PaymentError> {
        if let Some(previous) = self.settlement {
            return if previous == settlement {
                Ok(RequestFinancialEffects::default())
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        if settlement != RequestSettlement::Cancelled && !self.funding_finalized() {
            return Err(PaymentError::InvalidOperation);
        }
        let mut next = self.clone();
        next.settlement = Some(settlement);
        let mut effects = RequestFinancialEffects::default();
        match settlement {
            RequestSettlement::Cancelled if !next.funding_finalized() => {
                if !next.capture_voided() {
                    next.capture_status = CaptureStatus::CancellationRequested;
                    effects.operations.push(next.capture.id);
                }
            }
            RequestSettlement::Accepted | RequestSettlement::Cancelled => {
                next.create_refund(next.capture.amount, &mut effects)?;
            }
            RequestSettlement::Rejected | RequestSettlement::Expired => {
                effects.postings.transfer(
                    Account::RequestEscrow(next.capture.id),
                    Account::ProcessingRevenue,
                    next.contract.processing_charge,
                );
                if settlement == RequestSettlement::Expired {
                    next.create_refund(next.contract.collateral, &mut effects)?;
                } else if !next.contract.collateral.is_zero() {
                    effects.postings.transfer(
                        Account::RequestEscrow(next.capture.id),
                        Account::ProgramClearing(next.capture.id),
                        next.contract.collateral,
                    );
                    effects.forfeitures.push(Forfeiture {
                        id: next.capture.id,
                        amount: next.contract.collateral,
                        unit: next.capture.unit,
                        forfeited_at: at,
                        terms: next.contract.terms,
                        requires_review: next.capture_reversed(),
                    });
                }
            }
        }
        *self = next;
        Ok(effects)
    }
    fn create_refund(
        &mut self,
        amount: Money,
        effects: &mut RequestFinancialEffects,
    ) -> Result<(), PaymentError> {
        if amount.is_zero() {
            return Ok(());
        }
        let operation = PaymentOperation {
            scope: self.capture.scope,
            id: self.contract.refund_id,
            kind: PaymentKind::Refund {
                capture: self.capture.id,
            },
            amount,
            unit: self.capture.unit,
            destination: self.capture.destination,
        };
        if let Some(previous) = self.refund.operation() {
            return if previous == &operation {
                Ok(())
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        effects.postings.transfer(
            Account::RequestEscrow(self.capture.id),
            Account::RefundPayable(operation.id),
            amount,
        );
        effects.operations.push(operation.id);
        self.refund = RefundStatus::Pending(operation);
        Ok(())
    }
    /// Reconciles verified evidence without consulting a request's conversational state.
    /// # Errors
    /// Rejects unauthenticated, conflicting, or impossible provider transitions atomically.
    pub fn record_payment(
        &mut self,
        receipt: &SignedPaymentEvidence,
    ) -> Result<RequestFinancialEffects, PaymentError> {
        let operation = self
            .operation(receipt.evidence.operation_id)
            .ok_or(PaymentError::OperationMismatch)?;
        receipt.verify(&self.contract.provider_key, operation)?;
        if let Some(previous) = self.events.get(&receipt.evidence.event_id) {
            return if previous == &receipt.evidence {
                Ok(RequestFinancialEffects::default())
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        let mut next = self.clone();
        let mut effects = RequestFinancialEffects::default();
        match (operation.id == self.capture.id, receipt.evidence.outcome) {
            (_, PaymentOutcome::Pending) | (false, PaymentOutcome::Failed) => {}
            (true, PaymentOutcome::Settled) => {
                if !next.funding_finalized() {
                    next.capture_status = CaptureStatus::FundingFinalized;
                    effects.postings.transfer(
                        Account::ProcessorClearing,
                        Account::RequestEscrow(next.capture.id),
                        next.capture.amount,
                    );
                    if next.settlement == Some(RequestSettlement::Cancelled) {
                        next.create_refund(next.capture.amount, &mut effects)?;
                    }
                }
            }
            (true, PaymentOutcome::Voided | PaymentOutcome::Failed) => {
                if !next.funding_finalized() {
                    next.capture_status = CaptureStatus::Voided;
                    next.settlement = Some(RequestSettlement::Cancelled);
                    effects.capture_voided = true;
                }
            }
            (true, PaymentOutcome::Reversed) => {
                if !next.funding_finalized() {
                    return Err(PaymentError::InvalidOperation);
                }
                if !next.capture_reversed() {
                    next.capture_status = CaptureStatus::Reversed;
                    effects.postings.transfer(
                        Account::CorporateLossClearing,
                        Account::ProcessorClearing,
                        next.capture.amount,
                    );
                    if next.settlement == Some(RequestSettlement::Rejected)
                        && !next.contract.collateral.is_zero()
                    {
                        effects.holds.push(next.capture.id);
                    }
                }
            }
            (false, PaymentOutcome::Settled) => {
                if let RefundStatus::Pending(refund) = &next.refund {
                    effects.postings.transfer(
                        Account::RefundPayable(refund.id),
                        Account::ProcessorClearing,
                        refund.amount,
                    );
                    next.refund = RefundStatus::Confirmed(refund.clone());
                }
            }
            _ => return Err(PaymentError::InvalidOperation),
        }
        next.events
            .insert(receipt.evidence.event_id, receipt.evidence.clone());
        *self = next;
        Ok(effects)
    }
}

#[derive(Deserialize, Serialize)]
struct StoredRequestFinancials {
    capture: PaymentOperation,
    capture_status: CaptureStatus,
    refund: RefundStatus,
    events: BTreeMap<FinancialEventId, PaymentEvidence>,
    contract: RequestFinancialContract,
    settlement: Option<RequestSettlement>,
}
impl From<RequestFinancials> for StoredRequestFinancials {
    fn from(v: RequestFinancials) -> Self {
        Self {
            capture: v.capture,
            capture_status: v.capture_status,
            refund: v.refund,
            events: v.events,
            contract: v.contract,
            settlement: v.settlement,
        }
    }
}
impl TryFrom<StoredRequestFinancials> for RequestFinancials {
    type Error = PaymentError;
    fn try_from(v: StoredRequestFinancials) -> Result<Self, Self::Error> {
        let mut result = Self::new(v.capture, v.contract)?;
        result.capture_status = v.capture_status;
        result.refund = v.refund;
        result.events = v.events;
        result.settlement = v.settlement;
        for (id, e) in &result.events {
            let operation = result
                .operation(e.operation_id)
                .ok_or(PaymentError::OperationMismatch)?;
            if *id != e.event_id || operation.digest() != e.operation_digest {
                return Err(PaymentError::OperationMismatch);
            }
        }
        let has = |id, outcome| {
            result
                .events
                .values()
                .any(|e| e.operation_id == id && e.outcome == outcome)
        };
        let state_valid = match result.capture_status {
            CaptureStatus::Pending => result.settlement.is_none(),
            CaptureStatus::CancellationRequested => {
                result.settlement == Some(RequestSettlement::Cancelled)
            }
            CaptureStatus::FundingFinalized => has(result.capture.id, PaymentOutcome::Settled),
            CaptureStatus::Voided => {
                result.settlement == Some(RequestSettlement::Cancelled)
                    && (has(result.capture.id, PaymentOutcome::Voided)
                        || has(result.capture.id, PaymentOutcome::Failed))
            }
            CaptureStatus::Reversed => {
                has(result.capture.id, PaymentOutcome::Settled)
                    && has(result.capture.id, PaymentOutcome::Reversed)
            }
        };
        if !state_valid {
            return Err(PaymentError::InvalidOperation);
        }
        if let Some(refund) = result.refund.operation() {
            let expected = match result.settlement {
                Some(RequestSettlement::Accepted | RequestSettlement::Cancelled) => {
                    result.capture.amount
                }
                Some(RequestSettlement::Expired) => result.contract.collateral,
                _ => return Err(PaymentError::InvalidOperation),
            };
            if !result.funding_finalized()
                || refund.id != result.contract.refund_id
                || refund.kind
                    != (PaymentKind::Refund {
                        capture: result.capture.id,
                    })
                || refund.amount != expected
                || refund.scope != result.capture.scope
                || refund.unit != result.capture.unit
                || refund.destination != result.capture.destination
            {
                return Err(PaymentError::InvalidOperation);
            }
            if matches!(result.refund, RefundStatus::Confirmed(_))
                && !has(refund.id, PaymentOutcome::Settled)
            {
                return Err(PaymentError::InvalidOperation);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FinancialScope, PaymentProcessor, SimulatedProcessor};
    use cs_mail_ledger::LedgerState;
    use cs_mail_primitives::{
        Duration, PolicyVersion, ProgramRef, ProtocolVersion, ProviderRef, SettlementUnit,
    };
    fn fixture() -> (RequestFinancials, SimulatedProcessor, LedgerState) {
        let provider = SimulatedProcessor::new([7; 32]);
        let scope = FinancialScope::new(
            [1; 32],
            ProviderRef(1),
            ProgramRef(1),
            [2; 32],
            ProtocolVersion(2),
        );
        let capture = PaymentOperation {
            scope,
            id: PaymentOperationId(1),
            kind: PaymentKind::Capture,
            amount: Money::from_minor_units(10),
            unit: SettlementUnit(1),
            destination: [3; 32],
        };
        let contract = RequestFinancialContract {
            refund_id: PaymentOperationId(2),
            processing_charge: Money::from_minor_units(2),
            collateral: Money::from_minor_units(8),
            terms: FinancialTerms {
                scope,
                policy_version: PolicyVersion(1),
                corporate_basis_points: 300,
                maturity_delay: Duration(1),
            },
            provider_key: provider.verifying_key(),
        };
        (
            RequestFinancials::new(capture, contract).unwrap(),
            provider,
            LedgerState::new(SettlementUnit(1)),
        )
    }
    fn post(ledger: &mut LedgerState, effects: &RequestFinancialEffects) {
        ledger
            .apply(ledger.view().revision, &effects.postings)
            .unwrap();
        assert_eq!(ledger.view().total_value(), Ok(Money::ZERO));
    }
    #[test]
    fn every_settlement_owns_its_postings_and_is_idempotent() {
        for (settlement, refund, revenue, pool) in [
            (RequestSettlement::Accepted, 10, 0, 0),
            (RequestSettlement::Rejected, 0, 2, 8),
            (RequestSettlement::Expired, 8, 2, 0),
            (RequestSettlement::Cancelled, 10, 0, 0),
        ] {
            let (mut finances, mut provider, mut ledger) = fixture();
            let receipt = provider.submit(&finances.capture).unwrap();
            post(&mut ledger, &finances.record_payment(&receipt).unwrap());
            post(
                &mut ledger,
                &finances.settle(settlement, CanonicalTime(3)).unwrap(),
            );
            assert_eq!(
                ledger.balance(Account::RefundPayable(PaymentOperationId(2))),
                Money::from_minor_units(refund)
            );
            assert_eq!(
                ledger.balance(Account::ProcessingRevenue),
                Money::from_minor_units(revenue)
            );
            assert_eq!(
                ledger.balance(Account::RequestEscrow(PaymentOperationId(1))),
                Money::ZERO
            );
            assert_eq!(
                ledger
                    .view()
                    .signed_balance(Account::ProgramClearing(PaymentOperationId(1))),
                i128::from(pool)
            );
            assert_eq!(
                finances.settle(settlement, CanonicalTime(4)).unwrap(),
                RequestFinancialEffects::default()
            );
            assert_eq!(
                finances.record_payment(&receipt).unwrap(),
                RequestFinancialEffects::default()
            );
            if let Some(operation) = finances.refund().operation().cloned() {
                let receipt = provider.submit(&operation).unwrap();
                post(&mut ledger, &finances.record_payment(&receipt).unwrap());
                assert_eq!(
                    finances.record_payment(&receipt).unwrap(),
                    RequestFinancialEffects::default()
                );
            }
        }
    }
    #[test]
    fn cancelled_capture_can_arrive_late_without_a_request_object() {
        let (mut finances, mut provider, mut ledger) = fixture();
        let effects = finances
            .settle(RequestSettlement::Cancelled, CanonicalTime(1))
            .unwrap();
        assert_eq!(effects.operations, vec![finances.capture.id]);
        let receipt = provider.submit(&finances.capture).unwrap();
        post(&mut ledger, &finances.record_payment(&receipt).unwrap());
        assert_eq!(
            finances.refund().operation().unwrap().amount,
            Money::from_minor_units(10)
        );
        assert_eq!(finances.settlement(), Some(RequestSettlement::Cancelled));
    }
    #[test]
    fn reversals_are_monotonic_and_conflicting_evidence_is_atomic() {
        let (mut finances, mut provider, mut ledger) = fixture();
        let confirmation = provider.submit(&finances.capture).unwrap();
        let reversal = provider
            .reversal(finances.capture.id, FinancialEventId(9))
            .unwrap();
        let before = finances.clone();
        assert!(finances.record_payment(&reversal).is_err());
        assert_eq!(finances, before);
        post(
            &mut ledger,
            &finances.record_payment(&confirmation).unwrap(),
        );
        post(
            &mut ledger,
            &finances
                .settle(RequestSettlement::Rejected, CanonicalTime(2))
                .unwrap(),
        );
        let effects = finances.record_payment(&reversal).unwrap();
        assert_eq!(effects.holds, vec![finances.capture.id]);
        post(&mut ledger, &effects);
        assert_eq!(
            finances.record_payment(&confirmation).unwrap(),
            RequestFinancialEffects::default()
        );
        assert!(finances.capture_reversed());
        let conflict = provider
            .reversal(finances.capture.id, confirmation.evidence.event_id)
            .unwrap();
        let before = finances.clone();
        assert_eq!(
            finances.record_payment(&conflict),
            Err(PaymentError::DuplicateConflict)
        );
        assert_eq!(finances, before);
    }
}
