use cs_mail_primitives::{
    AllocationId, FinancialEventId, Money, PaymentOperationId, RelationshipRef, RequestId,
    SettlementUnit,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum PaymentKind {
    Capture,
    Refund { capture: PaymentOperationId },
    MemberPayout { allocation: AllocationId },
}
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PaymentOperation {
    pub scope: crate::FinancialScope,
    pub id: PaymentOperationId,
    pub kind: PaymentKind,
    pub amount: Money,
    pub unit: SettlementUnit,
    /// Provider token for the original payment method or verified member destination.
    pub destination: [u8; 32],
}
impl PaymentOperation {
    /// # Errors
    /// Rejects inconsistent identifiers, amounts, operation purposes, or evidence history.
    pub fn validate(&self) -> Result<(), PaymentError> {
        if self.id.0 == 0 || self.amount.is_zero() || self.destination == [0; 32] {
            return Err(PaymentError::InvalidOperation);
        }
        match self.kind {
            PaymentKind::Refund { capture } if capture.0 == 0 || capture == self.id => {
                Err(PaymentError::InvalidOperation)
            }
            PaymentKind::MemberPayout { allocation } if allocation.0 == 0 => {
                Err(PaymentError::InvalidOperation)
            }
            _ => Ok(()),
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"cs-mail/payment-operation/v2");
        h.update(self.scope.canonical_bytes());
        h.update(self.id.0.to_be_bytes());
        match self.kind {
            PaymentKind::Capture => h.update([0]),
            PaymentKind::Refund { capture } => {
                h.update([1]);
                h.update(capture.0.to_be_bytes());
            }
            PaymentKind::MemberPayout { allocation } => {
                h.update([2]);
                h.update(allocation.0.to_be_bytes());
            }
        }
        h.update(self.amount.minor_units().to_be_bytes());
        h.update(self.unit.0.to_be_bytes());
        h.update(self.destination);
        h.finalize().into()
    }
}
pub fn request_payment_id(
    scope: crate::FinancialScope,
    relationship: RelationshipRef,
    request: RequestId,
    refund: bool,
) -> PaymentOperationId {
    let mut h = Sha256::new();
    h.update(b"cs-mail/request-payment/v2");
    h.update(scope.canonical_bytes());
    h.update(relationship.derivation_version().to_be_bytes());
    h.update(relationship.as_bytes());
    h.update(request.0.to_be_bytes());
    h.update([u8::from(refund)]);
    let hash = h.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&hash[..16]);
    PaymentOperationId(u128::from_be_bytes(bytes))
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum PaymentOutcome {
    /// Provider accepted the operation; no economic value may yet be supplied.
    Pending,
    /// Authenticated evidence satisfying the configured funding-finality policy.
    Settled,
    /// Definitive failure: no transfer occurred. This is not an unknown result.
    Failed,
    Voided,
    Reversed,
}
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PaymentEvidence {
    pub event_id: FinancialEventId,
    pub operation_id: PaymentOperationId,
    pub operation_digest: [u8; 32],
    pub outcome: PaymentOutcome,
}
impl PaymentEvidence {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut b = b"cs-mail/payment-evidence/v1".to_vec();
        b.extend_from_slice(&self.event_id.0.to_be_bytes());
        b.extend_from_slice(&self.operation_id.0.to_be_bytes());
        b.extend_from_slice(&self.operation_digest);
        b.push(match self.outcome {
            PaymentOutcome::Settled => 0,
            PaymentOutcome::Pending => 3,
            PaymentOutcome::Failed => 4,
            PaymentOutcome::Voided => 1,
            PaymentOutcome::Reversed => 2,
        });
        b
    }
}
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SignedPaymentEvidence {
    pub evidence: PaymentEvidence,
    pub signature: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PaymentError {
    FundingRestricted,
    InvalidSignature,
    OperationMismatch,
    DuplicateConflict,
    Unavailable,
    InvalidOperation,
}
impl core::fmt::Display for PaymentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PaymentError {}
impl SignedPaymentEvidence {
    /// Verifies provider authority and the complete original operation, including amount and unit.
    /// # Errors
    /// Rejects invalid signatures, mismatched operations, and invalid outcome kinds.
    pub fn verify(&self, key: &[u8; 32], operation: &PaymentOperation) -> Result<(), PaymentError> {
        if self.evidence.operation_id != operation.id
            || self.evidence.operation_digest != operation.digest()
        {
            return Err(PaymentError::OperationMismatch);
        }
        if self.evidence.outcome == PaymentOutcome::Voided && operation.kind != PaymentKind::Capture
        {
            return Err(PaymentError::InvalidOperation);
        }
        let key = VerifyingKey::from_bytes(key).map_err(|_| PaymentError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| PaymentError::InvalidSignature)?;
        key.verify_strict(&self.evidence.signing_bytes(), &signature)
            .map_err(|_| PaymentError::InvalidSignature)
    }
}
/// The provider must deduplicate operation IDs and expose authoritative lookup.
/// Unknown outcomes are retried with the same ID; they never create a new charge.
pub trait PaymentProcessor {
    /// # Errors
    /// Returns an error when authoritative provider status cannot be obtained.
    fn lookup(
        &mut self,
        id: PaymentOperationId,
    ) -> Result<Option<SignedPaymentEvidence>, PaymentError>;
    /// # Errors
    /// Returns transient provider errors or rejects conflicting/invalid operations.
    fn submit(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<SignedPaymentEvidence, PaymentError>;
    /// # Errors
    /// Rejects conflicting capture identities or unavailable processor status.
    fn cancel_capture(
        &mut self,
        request: &CaptureCancellation,
    ) -> Result<SignedPaymentEvidence, PaymentError>;
}
/// Nonredeemable simulator. No bank, card network, or real money is involved.
pub struct SimulatedProcessor {
    key: SigningKey,
    operations: BTreeMap<PaymentOperationId, (PaymentOperation, SignedPaymentEvidence)>,
    fail_before: bool,
    lose_response: bool,
    pending_next: bool,
}
impl SimulatedProcessor {
    pub fn new(secret: [u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(&secret),
            operations: BTreeMap::new(),
            fail_before: false,
            lose_response: false,
            pending_next: false,
        }
    }
    pub fn verifying_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    pub fn fail_next_submission(&mut self) {
        self.fail_before = true;
    }
    pub fn lose_next_response(&mut self) {
        self.lose_response = true;
    }
    pub fn pend_next_submission(&mut self) {
        self.pending_next = true;
    }
    /// Advances a simulated asynchronous operation with a distinct signed event.
    /// # Errors
    /// Rejects missing operations and contradictory terminal changes.
    pub fn resolve(
        &mut self,
        id: PaymentOperationId,
        event_id: FinancialEventId,
        outcome: PaymentOutcome,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        let (operation, previous) = self
            .operations
            .get(&id)
            .ok_or(PaymentError::InvalidOperation)?;
        if previous.evidence.outcome != PaymentOutcome::Pending
            || !matches!(
                outcome,
                PaymentOutcome::Settled | PaymentOutcome::Failed | PaymentOutcome::Voided
            )
        {
            return Err(PaymentError::InvalidOperation);
        }
        let operation = operation.clone();
        let receipt = self.sign(PaymentEvidence {
            event_id,
            operation_id: id,
            operation_digest: operation.digest(),
            outcome,
        });
        receipt.verify(&self.verifying_key(), &operation)?;
        self.operations.insert(id, (operation, receipt.clone()));
        Ok(receipt)
    }
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }
    /// Emits separately authenticated reversal evidence without changing the original confirmation.
    /// # Errors
    /// Requires a previously confirmed capture or payout.
    pub fn reversal(
        &self,
        id: PaymentOperationId,
        event_id: FinancialEventId,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        let (operation, receipt) = self
            .operations
            .get(&id)
            .ok_or(PaymentError::InvalidOperation)?;
        if receipt.evidence.outcome != PaymentOutcome::Settled
            || matches!(operation.kind, PaymentKind::Refund { .. })
        {
            return Err(PaymentError::InvalidOperation);
        }
        Ok(self.sign(PaymentEvidence {
            event_id,
            operation_id: id,
            operation_digest: operation.digest(),
            outcome: PaymentOutcome::Reversed,
        }))
    }
    fn sign(&self, evidence: PaymentEvidence) -> SignedPaymentEvidence {
        let signature = self.key.sign(&evidence.signing_bytes()).to_bytes().to_vec();
        SignedPaymentEvidence {
            evidence,
            signature,
        }
    }
}
impl PaymentProcessor for SimulatedProcessor {
    fn lookup(
        &mut self,
        id: PaymentOperationId,
    ) -> Result<Option<SignedPaymentEvidence>, PaymentError> {
        Ok(self.operations.get(&id).map(|(_, r)| r.clone()))
    }
    fn submit(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        if let Some((original, receipt)) = self.operations.get(&operation.id) {
            return if original == operation {
                Ok(receipt.clone())
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        if std::mem::take(&mut self.fail_before) {
            return Err(PaymentError::Unavailable);
        }
        operation.validate()?;
        if let PaymentKind::Refund { capture } = operation.kind {
            let (original, receipt) = self
                .operations
                .get(&capture)
                .ok_or(PaymentError::InvalidOperation)?;
            if original.scope != operation.scope
                || original.kind != PaymentKind::Capture
                || receipt.evidence.outcome != PaymentOutcome::Settled
                || original.unit != operation.unit
                || original.destination != operation.destination
                || operation.amount > original.amount
            {
                return Err(PaymentError::InvalidOperation);
            }
            let refunded = self
                .operations
                .values()
                .filter(|(o, r)| {
                    o.kind == PaymentKind::Refund { capture }
                        && !matches!(
                            r.evidence.outcome,
                            PaymentOutcome::Failed
                                | PaymentOutcome::Voided
                                | PaymentOutcome::Reversed
                        )
                })
                .try_fold(Money::ZERO, |sum, (o, _)| sum.checked_add(o.amount))
                .ok_or(PaymentError::InvalidOperation)?;
            if refunded
                .checked_add(operation.amount)
                .is_none_or(|total| total > original.amount)
            {
                return Err(PaymentError::InvalidOperation);
            }
        }
        let pending = std::mem::take(&mut self.pending_next);
        let receipt = self.sign(PaymentEvidence {
            event_id: FinancialEventId(operation.id.0),
            operation_id: operation.id,
            operation_digest: operation.digest(),
            outcome: if pending {
                PaymentOutcome::Pending
            } else {
                PaymentOutcome::Settled
            },
        });
        self.operations
            .insert(operation.id, (operation.clone(), receipt.clone()));
        if std::mem::take(&mut self.lose_response) {
            Err(PaymentError::Unavailable)
        } else {
            Ok(receipt)
        }
    }
    fn cancel_capture(
        &mut self,
        request: &CaptureCancellation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        let operation = request.operation();
        if let Some((original, receipt)) = self.operations.get(&operation.id) {
            if original != operation {
                return Err(PaymentError::DuplicateConflict);
            }
            if receipt.evidence.outcome != PaymentOutcome::Pending {
                return Ok(receipt.clone());
            }
        }
        if std::mem::take(&mut self.fail_before) {
            return Err(PaymentError::Unavailable);
        }
        let mut h = Sha256::new();
        h.update(b"cs-mail/capture-cancellation/v1");
        h.update(operation.digest());
        let digest = h.finalize();
        let mut id = [0; 16];
        id.copy_from_slice(&digest[..16]);
        let receipt = self.sign(PaymentEvidence {
            event_id: FinancialEventId(u128::from_be_bytes(id)),
            operation_id: operation.id,
            operation_digest: operation.digest(),
            outcome: PaymentOutcome::Voided,
        });
        self.operations
            .insert(operation.id, (operation.clone(), receipt.clone()));
        if std::mem::take(&mut self.lose_response) {
            Err(PaymentError::Unavailable)
        } else {
            Ok(receipt)
        }
    }
}

/// Only capture operations can become a cancellation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureCancellation(PaymentOperation);
impl CaptureCancellation {
    /// # Errors
    /// Rejects invalid or inconsistent domain inputs.
    pub fn new(operation: PaymentOperation) -> Result<Self, PaymentError> {
        operation.validate()?;
        if operation.kind != PaymentKind::Capture {
            return Err(PaymentError::InvalidOperation);
        }
        Ok(Self(operation))
    }
    pub fn operation(&self) -> &PaymentOperation {
        &self.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessorRequest {
    Submit(PaymentOperation),
    CancelCapture(CaptureCancellation),
}
