use super::{
    ActorRef, CommandSigner, OperationalKeyRef, SecurityError, Signature, Signer, VerifyingKey,
};
use cs_mail_primitives::ProviderRef;
use cs_mail_protocol::pricing::RecipientRequestClasses;
use serde::{Deserialize, Serialize};

/// Scope of a recipient's own publication: the deployment and its provider. It names no
/// relationship, so one publication applies to every sender.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecipientSigningScope {
    pub deployment_domain: [u8; 32],
    pub provider: ProviderRef,
}

/// A recipient address's request classes, signed by a key of that address.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedRequestClasses {
    pub scope: RecipientSigningScope,
    pub operational_key: OperationalKeyRef,
    pub classes: RecipientRequestClasses,
    pub signature: Vec<u8>,
}
impl SignedRequestClasses {
    fn bytes(&self) -> Result<Vec<u8>, SecurityError> {
        serde_json::to_vec(&(
            "cs-mail/recipient-request-classes/v2",
            self.scope,
            self.operational_key,
            &self.classes,
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
    pub fn sign_request_classes(
        &self,
        scope: RecipientSigningScope,
        classes: RecipientRequestClasses,
    ) -> Result<SignedRequestClasses, SecurityError> {
        if self.actor != ActorRef::Recipient(classes.recipient()) || classes.version() == 0 {
            return Err(SecurityError::InvalidSignature);
        }
        let mut signed = SignedRequestClasses {
            scope,
            operational_key: self.reference,
            classes,
            signature: Vec::new(),
        };
        signed.signature = self.signing_key.sign(&signed.bytes()?).to_bytes().to_vec();
        Ok(signed)
    }
}
