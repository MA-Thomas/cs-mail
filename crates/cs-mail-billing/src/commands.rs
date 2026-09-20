use crate::BillingError;
use cs_mail_finance::FinancialScope;
use cs_mail_primitives::{BillingAccountId, IdempotencyKey, OperationalKeyRef, PolicyVersion};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BillingCommand {
    Inspect,
    PurchaseService {
        offer: PolicyVersion,
    },
    RetryCollection {
        contract: cs_mail_primitives::ServiceContractId,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedBillingCommand {
    pub account: BillingAccountId,
    pub operational_key: OperationalKeyRef,
    pub scope: FinancialScope,
    pub expected_revision: u64,
    pub idempotency_key: IdempotencyKey,
    pub command: BillingCommand,
    pub signature: Vec<u8>,
}
impl SignedBillingCommand {
    fn bytes(&self) -> Result<Vec<u8>, BillingError> {
        serde_json::to_vec(&(
            "cs-mail/billing-command/v3",
            self.operational_key,
            self.account,
            self.scope,
            self.expected_revision,
            self.idempotency_key,
            &self.command,
        ))
        .map_err(|_| BillingError::Conflict)
    }
    /// # Errors
    /// Rejects an unencodable command.
    pub fn sign(
        account: BillingAccountId,
        operational_key: OperationalKeyRef,
        scope: FinancialScope,
        expected_revision: u64,
        idempotency_key: IdempotencyKey,
        command: BillingCommand,
        secret: &[u8; 32],
    ) -> Result<Self, BillingError> {
        let mut signed = Self {
            account,
            operational_key,
            scope,
            expected_revision,
            idempotency_key,
            command,
            signature: Vec::new(),
        };
        signed.signature = SigningKey::from_bytes(secret)
            .sign(&signed.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(signed)
    }
    /// # Errors
    /// Rejects a command not authorized by the selected account authority.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), BillingError> {
        let key = VerifyingKey::from_bytes(key).map_err(|_| BillingError::Conflict)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| BillingError::Conflict)?;
        key.verify_strict(&self.bytes()?, &signature)
            .map_err(|_| BillingError::Conflict)
    }
}
