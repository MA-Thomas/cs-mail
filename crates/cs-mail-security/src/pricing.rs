use super::{
    ActorRef, CommandSigner, OperationalKeyRef, SecurityError, Signature, Signer, SigningScope,
    VerifyingKey,
};
use cs_mail_protocol::pricing::RecipientCollateralPreference;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedCollateralPreference {
    pub scope: SigningScope,
    pub operational_key: OperationalKeyRef,
    pub preference: RecipientCollateralPreference,
    pub signature: Vec<u8>,
}
impl SignedCollateralPreference {
    fn bytes(&self) -> Result<Vec<u8>, SecurityError> {
        serde_json::to_vec(&(
            "cs-mail/recipient-collateral/v1",
            self.scope,
            self.operational_key,
            &self.preference,
        ))
        .map_err(|_| SecurityError::InvalidSignature)
    }
    /// # Errors
    /// Rejects invalid signatures over the recipient, scope, version and amount.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), SecurityError> {
        let key = VerifyingKey::from_bytes(key).map_err(|_| SecurityError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| SecurityError::InvalidSignature)?;
        key.verify_strict(&self.bytes()?, &signature)
            .map_err(|_| SecurityError::InvalidSignature)
    }
}
impl CommandSigner {
    /// # Errors
    /// Requires the recipient's own operational signer.
    pub fn sign_collateral_preference(
        &self,
        scope: SigningScope,
        preference: RecipientCollateralPreference,
    ) -> Result<SignedCollateralPreference, SecurityError> {
        if self.actor != ActorRef::Recipient(preference.recipient) || preference.version == 0 {
            return Err(SecurityError::InvalidSignature);
        }
        let mut signed = SignedCollateralPreference {
            scope,
            operational_key: self.reference,
            preference,
            signature: Vec::new(),
        };
        signed.signature = self.signing_key.sign(&signed.bytes()?).to_bytes().to_vec();
        Ok(signed)
    }
}
