//! Bank-linked account evidence. Verification authority is configured by the host.
use crate::{FinancialScope, PaymentError};
use cs_mail_primitives::{BillingAccountId, MemberId, SettlementUnit};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BankVerification {
    pub scope: FinancialScope,
    pub account: BillingAccountId,
    pub member: MemberId,
    pub person: [u8; 32],
    pub bank_token: [u8; 32],
    pub unit: SettlementUnit,
    pub version: u64,
    pub signature: Vec<u8>,
}
impl BankVerification {
    fn bytes(&self) -> Result<Vec<u8>, PaymentError> {
        serde_json::to_vec(&(
            "cs-mail/bank-association/v1",
            self.scope,
            self.account,
            self.member,
            self.person,
            self.bank_token,
            self.unit,
            self.version,
        ))
        .map_err(|_| PaymentError::InvalidOperation)
    }
    /// # Errors
    /// Rejects evidence that cannot be encoded for signing.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, PaymentError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects invalid signatures, missing identity fields, or mismatched signed content.
    pub fn verify(&self, authority: &[u8; 32]) -> Result<VerifiedBankAccount, PaymentError> {
        if self.account.0 == 0
            || self.member.0 == 0
            || self.person == [0; 32]
            || self.bank_token == [0; 32]
            || self.version == 0
        {
            return Err(PaymentError::InvalidOperation);
        }
        VerifyingKey::from_bytes(authority)
            .map_err(|_| PaymentError::InvalidSignature)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature)
                    .map_err(|_| PaymentError::InvalidSignature)?,
            )
            .map_err(|_| PaymentError::InvalidSignature)?;
        Ok(VerifiedBankAccount {
            evidence: self.clone(),
            authority: *authority,
        })
    }
}
/// A verifier-produced capability, never deserialized from caller data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBankAccount {
    evidence: BankVerification,
    authority: [u8; 32],
}
impl VerifiedBankAccount {
    pub fn evidence(&self) -> &BankVerification {
        &self.evidence
    }
    pub fn authority(&self) -> &[u8; 32] {
        &self.authority
    }
}

/// Bank-verification proofs cannot be supplied by deserializing user input.
/// ```compile_fail
/// fn accepts_json<T: for<'de> serde::Deserialize<'de>>() {}
/// accepts_json::<cs_mail_finance::VerifiedBankAccount>();
/// ```
const _: () = ();
