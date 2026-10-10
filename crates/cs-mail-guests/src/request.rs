//! Immutable preparation inputs. Only a keyed fingerprint is retained, never plaintext.
use crate::{GuestError, GuestMailbox};
use cs_mail_primitives::{IdempotencyKey, ProtocolIdentity, RequestClassId};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub struct GuestRequestInput<'a> {
    pub mailbox: &'a GuestMailbox,
    pub recipient: ProtocolIdentity,
    pub class: RequestClassId,
    pub payment_token: &'a [u8; 32],
    pub message: &'a str,
}

/// Equality binds an operation to its original inputs; it is not mailbox authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuestRequestFingerprint([u8; 32]);
impl GuestRequestInput<'_> {
    /// Domain separation and the operation identity prevent correlating equal messages
    /// across operations. The secret prevents offline guesses of short message text.
    /// # Panics
    /// Never: HMAC accepts a 32-byte key.
    pub fn fingerprint(
        &self,
        secret: &[u8; 32],
        deployment: &[u8; 32],
        operation: IdempotencyKey,
    ) -> GuestRequestFingerprint {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
        mac.update(b"cs-mail/guest-request-input/v1");
        mac.update(deployment);
        mac.update(&operation.0.to_be_bytes());
        let mailbox = self.mailbox.address();
        mac.update(&(mailbox.len() as u64).to_be_bytes());
        mac.update(mailbox.as_bytes());
        mac.update(&self.recipient.0.to_be_bytes());
        mac.update(&self.class.0.to_be_bytes());
        mac.update(self.payment_token);
        mac.update(self.message.as_bytes());
        GuestRequestFingerprint(mac.finalize().into_bytes().into())
    }
}
impl GuestRequestFingerprint {
    /// # Errors
    /// Rejects changing any input under an existing operation identity.
    pub fn check_retry(self, stored: Self) -> Result<(), GuestError> {
        if self == stored {
            Ok(())
        } else {
            Err(GuestError::AlteredRequest)
        }
    }
}
