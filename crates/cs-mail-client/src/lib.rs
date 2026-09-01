//! Native endpoint helpers. Plaintext never crosses this API's provider boundary.

use cs_mail_content::{
    ContentBinding, ContentError, EncryptedContentRecord, EndpointPublicKey, EndpointSecretKey,
    decrypt, encrypt,
};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, IdempotencyKey, OperationalKeyRef, ProtocolVersion,
};
use cs_mail_protocol::{ActorRef, ProtocolCommand};
use cs_mail_security::{CommandSigner, SecurityError, SignedCommandBytes, SigningScope};

pub struct NativeClient {
    command_signer: CommandSigner,
    content_secret: EndpointSecretKey,
    content_public: EndpointPublicKey,
}

impl NativeClient {
    pub fn new(
        actor: ActorRef,
        operational_key: OperationalKeyRef,
        signing_secret: &[u8; 32],
        content_key: ContentKeyRef,
    ) -> Self {
        let (content_secret, content_public) = EndpointSecretKey::generate(content_key);
        Self {
            command_signer: CommandSigner::from_secret_bytes(
                actor,
                operational_key,
                signing_secret,
            ),
            content_secret,
            content_public,
        }
    }

    pub fn operational_verifying_key(&self) -> [u8; 32] {
        self.command_signer.verifying_key_bytes()
    }

    pub const fn content_public_key(&self) -> EndpointPublicKey {
        self.content_public
    }

    /// Canonically signs a command for authenticated ingress.
    ///
    /// # Errors
    ///
    /// Returns an error if canonical encoding fails.
    pub fn sign_command(
        &self,
        scope: SigningScope,
        protocol_version: ProtocolVersion,
        idempotency_key: IdempotencyKey,
        command: ProtocolCommand,
    ) -> Result<SignedCommandBytes, SecurityError> {
        self.command_signer
            .sign(scope, protocol_version, idempotency_key, command)
    }

    /// Encrypts subject, body, and attachment bytes before upload.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid keys, lifetimes, or content size.
    pub fn encrypt_for(
        &self,
        recipient: EndpointPublicKey,
        binding: ContentBinding,
        plaintext: &[u8],
        created_at: CanonicalTime,
        expires_at: CanonicalTime,
    ) -> Result<EncryptedContentRecord, ContentError> {
        encrypt(
            &self.content_secret,
            recipient,
            binding,
            plaintext,
            created_at,
            expires_at,
        )
    }

    /// Authenticates and decrypts a record addressed to this endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong endpoint, tampering, or malformed ciphertext.
    pub fn decrypt(
        &self,
        sender: EndpointPublicKey,
        record: &EncryptedContentRecord,
    ) -> Result<Vec<u8>, ContentError> {
        decrypt(&self.content_secret, sender, record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{ContentRef, MessageId, ProtocolIdentity};

    #[test]
    fn clients_exchange_only_ciphertext_through_provider_records() {
        let alice = NativeClient::new(
            ActorRef::Sender(ProtocolIdentity(1)),
            OperationalKeyRef(1),
            &[1; 32],
            ContentKeyRef(1),
        );
        let bob = NativeClient::new(
            ActorRef::Recipient(ProtocolIdentity(2)),
            OperationalKeyRef(2),
            &[2; 32],
            ContentKeyRef(2),
        );
        let record = alice
            .encrypt_for(
                bob.content_public_key(),
                ContentBinding {
                    content_ref: ContentRef(3),
                    message_id: MessageId(4),
                    sender: ProtocolIdentity(1),
                    recipient: ProtocolIdentity(2),
                    protocol_version: ProtocolVersion(1),
                },
                b"private message",
                CanonicalTime(1),
                CanonicalTime(10),
            )
            .unwrap();
        assert!(
            !record
                .envelope
                .ciphertext
                .windows(7)
                .any(|part| part == b"private")
        );
        assert_eq!(
            bob.decrypt(alice.content_public_key(), &record).unwrap(),
            b"private message"
        );
    }
}
