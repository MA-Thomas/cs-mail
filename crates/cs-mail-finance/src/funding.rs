//! Current restrictions and durable dispatch grants for a verified bank funding source.
use crate::{
    PaymentError, PaymentExecution, PaymentKind, PaymentOperation, PaymentState,
    SignedPaymentEvidence, VerifiedBankAccount,
};
use cs_mail_primitives::PaymentOperationId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredSource", into = "StoredSource")]
pub struct FundingSource {
    token: [u8; 32],
    verification: u64,
    restricted: bool,
    maximum_unresolved: u32,
    attempts: BTreeMap<PaymentOperationId, PaymentExecution>,
    dispatched: BTreeSet<PaymentOperationId>,
}
#[derive(Deserialize, Serialize)]
struct StoredSource {
    token: [u8; 32],
    verification: u64,
    restricted: bool,
    maximum_unresolved: u32,
    attempts: BTreeMap<PaymentOperationId, PaymentExecution>,
    dispatched: BTreeSet<PaymentOperationId>,
}
impl FundingSource {
    /// # Errors
    /// Rejects an empty unresolved-attempt limit.
    pub fn verified(
        bank: &VerifiedBankAccount,
        maximum_unresolved: u32,
    ) -> Result<Self, PaymentError> {
        if maximum_unresolved == 0 {
            return Err(PaymentError::InvalidOperation);
        }
        Ok(Self {
            token: bank.evidence().bank_token,
            verification: bank.evidence().version,
            restricted: false,
            maximum_unresolved,
            attempts: BTreeMap::new(),
            dispatched: BTreeSet::new(),
        })
    }
    pub const fn restricted(&self) -> bool {
        self.restricted
    }
    /// # Errors
    /// Rejects restricted sources, exhausted unresolved capacity, or conflicting operation identities.
    pub fn reserve(&mut self, operation: &PaymentOperation) -> Result<(), PaymentError> {
        if let Some(old) = self.attempts.get(&operation.id) {
            return if old.current() == operation {
                Ok(())
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        if self.restricted
            || self
                .attempts
                .values()
                .filter(|p| p.state() == PaymentState::Pending)
                .count()
                >= self.maximum_unresolved as usize
            || operation.destination != self.token
            || operation.kind != PaymentKind::Capture
        {
            return Err(PaymentError::FundingRestricted);
        }
        self.attempts
            .insert(operation.id, PaymentExecution::new(operation.clone())?);
        Ok(())
    }
    /// Must be committed under the same source lock as restriction changes, before submission.
    /// # Errors
    /// Rejects a new dispatch when its source is restricted or the reserved attempt is not pending.
    pub fn authorize_dispatch(&mut self, id: PaymentOperationId) -> Result<(), PaymentError> {
        if self.dispatched.contains(&id) {
            return Ok(());
        }
        if self.restricted
            || self
                .attempts
                .get(&id)
                .is_none_or(|p| p.state() != PaymentState::Pending)
        {
            return Err(PaymentError::FundingRestricted);
        }
        self.dispatched.insert(id);
        Ok(())
    }
    /// # Errors
    /// Rejects unauthenticated, conflicting, or impossible processor evidence.
    pub fn record(
        &mut self,
        receipt: &SignedPaymentEvidence,
        key: &[u8; 32],
    ) -> Result<(), PaymentError> {
        if !self.dispatched.contains(&receipt.evidence.operation_id)
            && receipt.evidence.outcome != crate::PaymentOutcome::Voided
        {
            return Err(PaymentError::InvalidOperation);
        }
        let payment = self
            .attempts
            .get_mut(&receipt.evidence.operation_id)
            .ok_or(PaymentError::InvalidOperation)?;
        payment.record(receipt, key)?;
        if matches!(
            payment.state(),
            PaymentState::Failed { .. } | PaymentState::Reversed { .. }
        ) {
            self.restricted = true;
        }
        Ok(())
    }
    /// # Errors
    /// Rejects a different bank association or a replayed verification version.
    pub fn reverify(&mut self, bank: &VerifiedBankAccount) -> Result<(), PaymentError> {
        if bank.evidence().bank_token != self.token || bank.evidence().version <= self.verification
        {
            return Err(PaymentError::InvalidOperation);
        }
        self.verification = bank.evidence().version;
        self.restricted = false;
        Ok(())
    }
}
impl From<FundingSource> for StoredSource {
    fn from(v: FundingSource) -> Self {
        Self {
            token: v.token,
            verification: v.verification,
            restricted: v.restricted,
            maximum_unresolved: v.maximum_unresolved,
            attempts: v.attempts,
            dispatched: v.dispatched,
        }
    }
}
impl TryFrom<StoredSource> for FundingSource {
    type Error = PaymentError;
    fn try_from(v: StoredSource) -> Result<Self, Self::Error> {
        if v.token == [0; 32] || v.verification == 0 || v.maximum_unresolved == 0 {
            return Err(PaymentError::InvalidOperation);
        }
        for (id, p) in &v.attempts {
            if *id != p.current().id
                || p.attempt_count() != 1
                || p.current().kind != PaymentKind::Capture
                || p.current().destination != v.token
            {
                return Err(PaymentError::InvalidOperation);
            }
        }
        if !v.dispatched.iter().all(|id| v.attempts.contains_key(id)) {
            return Err(PaymentError::InvalidOperation);
        }
        Ok(Self {
            token: v.token,
            verification: v.verification,
            restricted: v.restricted,
            maximum_unresolved: v.maximum_unresolved,
            attempts: v.attempts,
            dispatched: v.dispatched,
        })
    }
}
