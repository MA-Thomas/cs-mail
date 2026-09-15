//! One logical payment, with sequential processor attempts and verified terminal evidence.
use crate::{
    PaymentError, PaymentEvidence, PaymentKind, PaymentOperation, PaymentOutcome,
    SignedPaymentEvidence,
};
use cs_mail_primitives::{FinancialEventId, PaymentOperationId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PaymentState {
    Pending,
    Settled {
        evidence: FinancialEventId,
    },
    Failed {
        evidence: FinancialEventId,
    },
    Voided {
        evidence: FinancialEventId,
    },
    Reversed {
        settlement: FinancialEventId,
        reversal: FinancialEventId,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Attempt {
    operation: PaymentOperation,
    state: PaymentState,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredExecution", into = "StoredExecution")]
pub struct PaymentExecution {
    attempts: Vec<Attempt>,
    events: BTreeMap<FinancialEventId, PaymentEvidence>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredExecution {
    attempts: Vec<Attempt>,
    events: BTreeMap<FinancialEventId, PaymentEvidence>,
}
impl From<PaymentExecution> for StoredExecution {
    fn from(value: PaymentExecution) -> Self {
        Self {
            attempts: value.attempts,
            events: value.events,
        }
    }
}
impl TryFrom<StoredExecution> for PaymentExecution {
    type Error = PaymentError;
    fn try_from(value: StoredExecution) -> Result<Self, Self::Error> {
        let result = Self {
            attempts: value.attempts,
            events: value.events,
        };
        result.validate()?;
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentChange {
    None,
    Settled,
    Reversed,
}
impl PaymentExecution {
    /// # Errors
    /// Rejects invalid or inconsistent domain inputs.
    pub fn new(operation: PaymentOperation) -> Result<Self, PaymentError> {
        operation.validate()?;
        Ok(Self {
            attempts: vec![Attempt {
                operation,
                state: PaymentState::Pending,
            }],
            events: BTreeMap::new(),
        })
    }
    pub fn current(&self) -> &PaymentOperation {
        &self.attempts[self.attempts.len() - 1].operation
    }
    pub fn state(&self) -> PaymentState {
        self.attempts[self.attempts.len() - 1].state
    }
    pub fn attempt_count(&self) -> usize {
        self.attempts.len()
    }
    pub fn operation(&self, id: PaymentOperationId) -> Option<&PaymentOperation> {
        self.attempts
            .iter()
            .find(|a| a.operation.id == id)
            .map(|a| &a.operation)
    }
    pub fn pending(&self) -> Option<&PaymentOperation> {
        (self.state() == PaymentState::Pending).then(|| self.current())
    }
    /// # Errors
    /// Rejects an unresolved or successful attempt, or reuse of an existing attempt identifier.
    pub fn retry(&mut self, id: PaymentOperationId) -> Result<PaymentOperation, PaymentError> {
        if !matches!(
            self.state(),
            PaymentState::Failed { .. } | PaymentState::Voided { .. }
        ) || self.operation(id).is_some()
        {
            return Err(PaymentError::InvalidOperation);
        }
        let mut operation = self.current().clone();
        operation.id = id;
        operation.validate()?;
        self.attempts.push(Attempt {
            operation: operation.clone(),
            state: PaymentState::Pending,
        });
        Ok(operation)
    }
    /// # Errors
    /// Rejects unauthenticated, conflicting, or impossible processor evidence.
    pub fn record(
        &mut self,
        receipt: &SignedPaymentEvidence,
        key: &[u8; 32],
    ) -> Result<PaymentChange, PaymentError> {
        let index = self
            .attempts
            .iter()
            .position(|a| a.operation.id == receipt.evidence.operation_id)
            .ok_or(PaymentError::OperationMismatch)?;
        receipt.verify(key, &self.attempts[index].operation)?;
        if let Some(old) = self.events.get(&receipt.evidence.event_id) {
            return if old == &receipt.evidence {
                Ok(PaymentChange::None)
            } else {
                Err(PaymentError::DuplicateConflict)
            };
        }
        let (state, change) = next_state(self.attempts[index].state, &receipt.evidence)?;
        // A previous definitively failed attempt cannot acquire a new economic outcome.
        if index + 1 != self.attempts.len() && change != PaymentChange::None {
            return Err(PaymentError::InvalidOperation);
        }
        self.attempts[index].state = state;
        self.events
            .insert(receipt.evidence.event_id, receipt.evidence.clone());
        Ok(change)
    }
    /// # Errors
    /// Rejects inconsistent identifiers, amounts, operation purposes, or evidence history.
    pub fn validate(&self) -> Result<(), PaymentError> {
        let first = self
            .attempts
            .first()
            .ok_or(PaymentError::InvalidOperation)?;
        let mut ids = BTreeSet::new();
        for (i, attempt) in self.attempts.iter().enumerate() {
            attempt.operation.validate()?;
            let mut expected = first.operation.clone();
            expected.id = attempt.operation.id;
            if attempt.operation != expected
                || !ids.insert(attempt.operation.id)
                || (i + 1 != self.attempts.len()
                    && !matches!(
                        attempt.state,
                        PaymentState::Failed { .. } | PaymentState::Voided { .. }
                    ))
            {
                return Err(PaymentError::InvalidOperation);
            }
            let matches = |id: FinancialEventId, outcome: PaymentOutcome| {
                self.events.get(&id).is_some_and(|e| {
                    e.operation_id == attempt.operation.id
                        && e.operation_digest == attempt.operation.digest()
                        && e.outcome == outcome
                })
            };
            let valid = match attempt.state {
                PaymentState::Pending => true,
                PaymentState::Settled { evidence } => matches(evidence, PaymentOutcome::Settled),
                PaymentState::Failed { evidence } => matches(evidence, PaymentOutcome::Failed),
                PaymentState::Voided { evidence } => {
                    attempt.operation.kind == PaymentKind::Capture
                        && matches(evidence, PaymentOutcome::Voided)
                }
                PaymentState::Reversed {
                    settlement,
                    reversal,
                } => {
                    matches(settlement, PaymentOutcome::Settled)
                        && matches(reversal, PaymentOutcome::Reversed)
                }
            };
            if !valid {
                return Err(PaymentError::InvalidOperation);
            }
        }
        for (id, event) in &self.events {
            let operation = self
                .operation(event.operation_id)
                .ok_or(PaymentError::InvalidOperation)?;
            if *id != event.event_id || event.operation_digest != operation.digest() {
                return Err(PaymentError::InvalidOperation);
            }
            let state = self
                .attempts
                .iter()
                .find(|a| a.operation.id == event.operation_id)
                .ok_or(PaymentError::InvalidOperation)?
                .state;
            if next_state(state, event)?.0 != state {
                return Err(PaymentError::InvalidOperation);
            }
        }
        Ok(())
    }
}
fn next_state(
    state: PaymentState,
    evidence: &PaymentEvidence,
) -> Result<(PaymentState, PaymentChange), PaymentError> {
    use PaymentOutcome as O;
    use PaymentState as S;
    let id = evidence.event_id;
    Ok(match (state, evidence.outcome) {
        (S::Pending, O::Settled) => (S::Settled { evidence: id }, PaymentChange::Settled),
        (S::Pending, O::Failed) => (S::Failed { evidence: id }, PaymentChange::None),
        (S::Pending, O::Voided) => (S::Voided { evidence: id }, PaymentChange::None),
        (S::Settled { evidence }, O::Reversed) => (
            S::Reversed {
                settlement: evidence,
                reversal: id,
            },
            PaymentChange::Reversed,
        ),
        (_, O::Pending)
        | (S::Settled { .. } | S::Reversed { .. }, O::Settled)
        | (S::Failed { .. }, O::Failed)
        | (S::Voided { .. }, O::Voided)
        | (S::Reversed { .. }, O::Reversed) => (state, PaymentChange::None),
        _ => return Err(PaymentError::InvalidOperation),
    })
}
