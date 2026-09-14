use crate::{PaymentKind, PaymentOperation, PaymentOutcome, ProgramError, SignedPaymentEvidence};
use cs_mail_ledger::{Account, LedgerBatch};
use cs_mail_primitives::{
    AllocationId, FinancialEventId, MemberId, Money, PaymentOperationId, QuarterId, SettlementUnit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemberPayable {
    pub id: AllocationId,
    pub member: MemberId,
    pub quarter: QuarterId,
    pub amount: Money,
    pub lifecycle: PayableLifecycle,
    pub previous_payments: Vec<PaymentOperation>,
    pub correction: Option<FinancialEventId>,
    pub(crate) events: std::collections::BTreeMap<FinancialEventId, crate::PaymentEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PayableLifecycle {
    Due,
    Pending(PaymentOperation),
    Paid(PaymentOperation),
    Reversed(PaymentOperation),
}
impl PayableLifecycle {
    pub const fn operation(&self) -> Option<&PaymentOperation> {
        match self {
            Self::Due => None,
            Self::Pending(p) | Self::Paid(p) | Self::Reversed(p) => Some(p),
        }
    }
    pub const fn pending(&self) -> Option<&PaymentOperation> {
        match self {
            Self::Pending(p) => Some(p),
            _ => None,
        }
    }
}

impl MemberPayable {
    pub(crate) fn prepare(
        &mut self,
        scope: crate::FinancialScope,
        unit: SettlementUnit,
        destination: [u8; 32],
        minimum: Money,
    ) -> Result<Option<PaymentOperation>, ProgramError> {
        match &self.lifecycle {
            PayableLifecycle::Paid(_) => return Ok(None),
            PayableLifecycle::Reversed(previous) => self.previous_payments.push(previous.clone()),
            PayableLifecycle::Pending(operation) => {
                return if operation.destination == destination {
                    Ok(Some(operation.clone()))
                } else {
                    Err(ProgramError::DuplicateConflict)
                };
            }
            PayableLifecycle::Due => {}
        }
        self.lifecycle = PayableLifecycle::Due;
        if self.amount < minimum {
            return Ok(None);
        }
        if destination == [0; 32] {
            return Err(ProgramError::InvalidPayment);
        }
        let operation = PaymentOperation {
            scope,
            id: payout_id(scope, self.id, self.previous_payments.len()),
            kind: PaymentKind::MemberPayout {
                allocation: self.id,
            },
            amount: self.amount,
            unit,
            destination,
        };
        self.lifecycle = PayableLifecycle::Pending(operation.clone());
        Ok(Some(operation))
    }
    pub(crate) fn record_receipt(
        &mut self,
        receipt: &SignedPaymentEvidence,
        key: &[u8; 32],
    ) -> Result<LedgerBatch, ProgramError> {
        let operation = self
            .lifecycle
            .operation()
            .filter(|o| o.id == receipt.evidence.operation_id)
            .or_else(|| {
                self.previous_payments
                    .iter()
                    .find(|o| o.id == receipt.evidence.operation_id)
            })
            .ok_or(ProgramError::InvalidPayment)?;
        receipt.verify(key, operation)?;
        if let Some(previous) = self.events.get(&receipt.evidence.event_id) {
            return if previous == &receipt.evidence {
                Ok(LedgerBatch::new())
            } else {
                Err(ProgramError::DuplicateConflict)
            };
        }
        if self.previous_payments.iter().any(|o| o.id == operation.id) {
            self.events
                .insert(receipt.evidence.event_id, receipt.evidence.clone());
            return Ok(LedgerBatch::new());
        }
        let mut batch = LedgerBatch::new();
        match (&self.lifecycle, receipt.evidence.outcome) {
            (PayableLifecycle::Pending(operation), PaymentOutcome::Confirmed) => {
                self.lifecycle = PayableLifecycle::Paid(operation.clone());
                batch.transfer(
                    Account::MemberPayable(self.id),
                    Account::ProcessorClearing,
                    self.amount,
                );
            }
            (PayableLifecycle::Paid(operation), PaymentOutcome::Reversed) => {
                self.lifecycle = PayableLifecycle::Reversed(operation.clone());
                batch.transfer(
                    Account::ProcessorClearing,
                    Account::MemberPayable(self.id),
                    self.amount,
                );
            }
            (
                PayableLifecycle::Paid(_) | PayableLifecycle::Reversed(_),
                PaymentOutcome::Confirmed,
            )
            | (PayableLifecycle::Reversed(_), PaymentOutcome::Reversed) => {}
            _ => return Err(ProgramError::InvalidPayment),
        }
        self.events
            .insert(receipt.evidence.event_id, receipt.evidence.clone());
        Ok(batch)
    }
}

fn payout_id(scope: crate::FinancialScope, id: AllocationId, attempt: usize) -> PaymentOperationId {
    let mut h = Sha256::new();
    h.update(b"cs-mail/member-payment/v2");
    h.update(scope.canonical_bytes());
    h.update(id.0.to_be_bytes());
    h.update((attempt as u64).to_be_bytes());
    let digest = h.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    PaymentOperationId(u128::from_be_bytes(bytes))
}
