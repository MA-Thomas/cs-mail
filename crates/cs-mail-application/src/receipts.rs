//! Shared, database-independent outcome evidence. Typed serialization is the receipt commitment.
use cs_mail_capabilities::{CapabilityError, SenderScopedLane};
use cs_mail_primitives::{JournalPosition, LaneId};
use cs_mail_protocol::{CommittedTransition, ProtocolError};
use cs_mail_security::OutcomeDigest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DurableExecutionOutcome {
    pub admission_failure: Option<cs_mail_protocol::admission::AdmissionFailure>,
    pub transition: CommittedTransition,
    #[serde(skip)]
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BondFreeAuthority {
    AcceptedRelationship,
    ExpressLane(LaneId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BondFreeAdmissionOutcome {
    pub authority: BondFreeAuthority,
    pub journal_position: JournalPosition,
    #[serde(skip)]
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaneOperationOutcome {
    pub changed: bool,
    pub lane: SenderScopedLane,
    #[serde(skip)]
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReceivedOutcome {
    Protocol(Box<DurableExecutionOutcome>),
    Message(BondFreeAdmissionOutcome),
    Lane(Box<LaneOperationOutcome>),
    Refused(Refusal),
    /// Detailed evidence expired; the original signed receipt and replay identity remain.
    Retired {
        outcome_digest: OutcomeDigest,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Refusal {
    Billing(cs_mail_billing::BillingError),
    Protocol(ProtocolError),
    Admission(cs_mail_protocol::admission::AdmissionFailure),
    Capability(CapabilityError),
    DuplicateConflict,
    VersionConflict,
    NotAuthorized,
    QuoteMissing,
    InvalidQuote,
}

impl ReceivedOutcome {
    /// # Errors
    /// Rejects an altered outcome body even when its accompanying receipt is validly signed.
    pub fn verify(
        &self,
        receipt: &cs_mail_security::SignedReceipt,
        provider: &[u8; 32],
    ) -> Result<(), cs_mail_security::SecurityError> {
        receipt.verify(provider)?;
        if self
            .digest()
            .map_err(|_| cs_mail_security::SecurityError::InvalidSignature)?
            != receipt.payload.outcome_digest
        {
            return Err(cs_mail_security::SecurityError::InvalidSignature);
        }
        Ok(())
    }
    /// # Errors
    /// Serialization of the typed outcome failed.
    pub fn digest(&self) -> Result<OutcomeDigest, serde_json::Error> {
        Ok(OutcomeDigest(
            Sha256::digest(serde_json::to_vec(self)?).into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_receipt_does_not_authenticate_a_substituted_outcome() {
        // Conjecture: altering the outcome without altering the signed receipt cannot establish a result.
        // A successfully verified substituted body would let native request progress invent a domain outcome.
        use cs_mail_primitives::*;
        use cs_mail_security::*;
        let original = ReceivedOutcome::Refused(Refusal::Protocol(ProtocolError::PolicyInvalid));
        let signer =
            ProviderSigner::from_secret_bytes(ProviderRef(30), OperationalKeyRef(3), &[3; 32]);
        let receipt = signer
            .sign_receipt(ReceiptPayload {
                deployment_domain: [7; 32],
                receipt_id: ReceiptRef(1),
                kind: ReceiptKind::Refused,
                relationship: RelationshipRef::from_u128_for_test(1),
                command_digest: CommandDigest([8; 32]),
                journal_position: JournalPosition(1),
                received_at: CanonicalTime(1),
                outcome_digest: original.digest().unwrap(),
                provider: ProviderRef(30),
                protocol_version: ProtocolVersion(2),
            })
            .unwrap();
        original
            .verify(&receipt, &signer.verifying_key_bytes())
            .unwrap();
        let substituted = ReceivedOutcome::Refused(Refusal::Protocol(ProtocolError::MissingRecord));
        assert!(
            substituted
                .verify(&receipt, &signer.verifying_key_bytes())
                .is_err()
        );
    }
}
