//! The nonredeemable payment simulator's rules, shared by the in-memory simulator and the
//! durable development simulator. No bank, card network or real money is involved.
//!
//! Each function decides one provider interaction from the records it is given and returns
//! the record to store and the response to give; callers own storage.
use crate::{
    CaptureCancellation, PaymentError, PaymentEvidence, PaymentKind, PaymentOperation,
    PaymentOutcome, SignedPaymentEvidence,
};
use cs_mail_primitives::{FinancialEventId, Money};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The outcome a new submission is given.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum DirectedOutcome {
    /// Evidence satisfying funding finality.
    #[default]
    Settled,
    /// Accepted but unresolved until an explicit resolution.
    Pending,
    /// Definitively declined; no transfer occurred.
    Failed,
}
/// How the simulator treats the next new submission or cancellation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SimulationDirective {
    /// Refuse before recording anything, as an unavailable provider would.
    pub unavailable: bool,
    pub outcome: DirectedOutcome,
    /// Record the operation, then lose the response, so the outcome is unknown to the
    /// caller until it looks the operation up.
    pub lose_response: bool,
}
/// One operation as the simulated provider has recorded it, with its latest evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SimulationRecord {
    pub operation: PaymentOperation,
    pub receipt: SignedPaymentEvidence,
}
/// For a refund: the captured operation it refunds and the amount already refunded by
/// refunds that have not failed, been voided or been reversed.
pub struct RefundContext<'a> {
    pub capture: Option<&'a SimulationRecord>,
    pub refunded: Money,
}
/// The simulator's decision for one interaction.
#[derive(Debug)]
pub struct SimulationDecision {
    /// A new or replaced record to store.
    pub record: Option<SimulationRecord>,
    /// Whether the directive was used; a replay of a recorded operation does not use it.
    pub consumed_directive: bool,
    pub response: Result<SignedPaymentEvidence, PaymentError>,
}

fn sign(key: &SigningKey, evidence: PaymentEvidence) -> SignedPaymentEvidence {
    let signature = key.sign(&evidence.signing_bytes()).to_bytes().to_vec();
    SignedPaymentEvidence {
        evidence,
        signature,
    }
}
fn respond(record: SimulationRecord, directive: SimulationDirective) -> SimulationDecision {
    let response = if directive.lose_response {
        Err(PaymentError::Unavailable)
    } else {
        Ok(record.receipt.clone())
    };
    SimulationDecision {
        record: Some(record),
        consumed_directive: true,
        response,
    }
}

/// Decides a submission. A recorded operation is replayed exactly, and an altered one with
/// the same identity is refused.
pub fn submit(
    key: &SigningKey,
    existing: Option<&SimulationRecord>,
    refund: &RefundContext<'_>,
    operation: &PaymentOperation,
    directive: SimulationDirective,
) -> SimulationDecision {
    if let Some(existing) = existing {
        return SimulationDecision {
            record: None,
            consumed_directive: false,
            response: if existing.operation == *operation {
                Ok(existing.receipt.clone())
            } else {
                Err(PaymentError::DuplicateConflict)
            },
        };
    }
    let refuse = |error| SimulationDecision {
        record: None,
        consumed_directive: true,
        response: Err(error),
    };
    if directive.unavailable {
        return refuse(PaymentError::Unavailable);
    }
    if let Err(error) = operation.validate() {
        return refuse(error);
    }
    if let PaymentKind::Refund { .. } = operation.kind {
        let Some(original) = refund.capture else {
            return refuse(PaymentError::InvalidOperation);
        };
        let captured = &original.operation;
        if captured.scope != operation.scope
            || captured.kind != PaymentKind::Capture
            || original.receipt.evidence.outcome != PaymentOutcome::Settled
            || captured.unit != operation.unit
            || captured.destination != operation.destination
            || operation.amount > captured.amount
            || refund
                .refunded
                .checked_add(operation.amount)
                .is_none_or(|total| total > captured.amount)
        {
            return refuse(PaymentError::InvalidOperation);
        }
    }
    let receipt = sign(
        key,
        PaymentEvidence {
            event_id: FinancialEventId(operation.id.0),
            operation_id: operation.id,
            operation_digest: operation.digest(),
            outcome: match directive.outcome {
                DirectedOutcome::Settled => PaymentOutcome::Settled,
                DirectedOutcome::Pending => PaymentOutcome::Pending,
                DirectedOutcome::Failed => PaymentOutcome::Failed,
            },
        },
    );
    respond(
        SimulationRecord {
            operation: operation.clone(),
            receipt,
        },
        directive,
    )
}

/// Decides a capture cancellation; a capture that is no longer pending keeps its evidence.
pub fn cancel_capture(
    key: &SigningKey,
    existing: Option<&SimulationRecord>,
    request: &CaptureCancellation,
    directive: SimulationDirective,
) -> SimulationDecision {
    let operation = request.operation();
    if let Some(existing) = existing {
        if existing.operation != *operation {
            return SimulationDecision {
                record: None,
                consumed_directive: false,
                response: Err(PaymentError::DuplicateConflict),
            };
        }
        if existing.receipt.evidence.outcome != PaymentOutcome::Pending {
            return SimulationDecision {
                record: None,
                consumed_directive: false,
                response: Ok(existing.receipt.clone()),
            };
        }
    }
    if directive.unavailable {
        return SimulationDecision {
            record: None,
            consumed_directive: true,
            response: Err(PaymentError::Unavailable),
        };
    }
    let mut h = Sha256::new();
    h.update(b"cs-mail/capture-cancellation/v1");
    h.update(operation.digest());
    let digest = h.finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    let receipt = sign(
        key,
        PaymentEvidence {
            event_id: FinancialEventId(u128::from_be_bytes(id)),
            operation_id: operation.id,
            operation_digest: operation.digest(),
            outcome: PaymentOutcome::Voided,
        },
    );
    respond(
        SimulationRecord {
            operation: operation.clone(),
            receipt,
        },
        directive,
    )
}

/// Resolves a pending operation with a distinct signed event.
/// # Errors
/// Rejects an operation that is not pending and a non-terminal outcome.
pub fn resolve(
    key: &SigningKey,
    record: &SimulationRecord,
    event_id: FinancialEventId,
    outcome: PaymentOutcome,
) -> Result<SimulationRecord, PaymentError> {
    if record.receipt.evidence.outcome != PaymentOutcome::Pending
        || !matches!(
            outcome,
            PaymentOutcome::Settled | PaymentOutcome::Failed | PaymentOutcome::Voided
        )
    {
        return Err(PaymentError::InvalidOperation);
    }
    let receipt = sign(
        key,
        PaymentEvidence {
            event_id,
            operation_id: record.operation.id,
            operation_digest: record.operation.digest(),
            outcome,
        },
    );
    receipt.verify(&key.verifying_key().to_bytes(), &record.operation)?;
    Ok(SimulationRecord {
        operation: record.operation.clone(),
        receipt,
    })
}

/// Separately authenticated reversal evidence; the original confirmation is unchanged.
/// # Errors
/// Requires a settled capture or payout.
pub fn reversal(
    key: &SigningKey,
    record: &SimulationRecord,
    event_id: FinancialEventId,
) -> Result<SignedPaymentEvidence, PaymentError> {
    if record.receipt.evidence.outcome != PaymentOutcome::Settled
        || matches!(record.operation.kind, PaymentKind::Refund { .. })
    {
        return Err(PaymentError::InvalidOperation);
    }
    Ok(sign(
        key,
        PaymentEvidence {
            event_id,
            operation_id: record.operation.id,
            operation_digest: record.operation.digest(),
            outcome: PaymentOutcome::Reversed,
        },
    ))
}

/// Whether a refund record still counts against its capture.
pub const fn refund_counts(record: &SimulationRecord) -> bool {
    !matches!(
        record.receipt.evidence.outcome,
        PaymentOutcome::Failed | PaymentOutcome::Voided | PaymentOutcome::Reversed
    )
}
