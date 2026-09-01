//! Native endpoint helpers. Plaintext never crosses this API's provider boundary.

use cs_mail_content::{
    ContentBinding, ContentError, ContentKeyCertificate, EncryptedContentRecord, EndpointPublicKey,
    EndpointSecretKey, decrypt, encrypt, message_declaration_digest,
};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentKeyVersion, IdempotencyKey, MessageDeclarationDigest,
    MessageDeclarations, OperationalKeyRef, ProtocolIdentity, ProtocolVersion, WireVersion,
};
use cs_mail_protocol::{ActorRef, ProtocolCommand};
use cs_mail_security::{
    CommandSigner, SecurityError, SignedCommandBytes, SignedContentKeyCertificate, SigningScope,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationDisposition {
    Inbox,
    Quiet,
    Digest,
}

pub struct PresentationPolicyInput<'a> {
    pub sender: ProtocolIdentity,
    pub declarations: &'a MessageDeclarations,
}

/// Recipient-owned presentation policy. Its output has no protocol or ledger authority.
pub trait PresentationPolicy {
    fn decide(&self, input: &PresentationPolicyInput<'_>) -> PresentationDisposition;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPresentationPolicy;

impl PresentationPolicy for DefaultPresentationPolicy {
    fn decide(&self, _input: &PresentationPolicyInput<'_>) -> PresentationDisposition {
        PresentationDisposition::Inbox
    }
}

pub struct NativeClient {
    actor: ActorRef,
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
            actor,
            command_signer: CommandSigner::from_secret_bytes(
                actor,
                operational_key,
                signing_secret,
            ),
            content_secret,
            content_public,
        }
    }

    /// Certifies the endpoint content key under the operational command key.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-sender client or invalid certificate scope/lifetime.
    pub fn certify_content_key(
        &self,
        scope: SigningScope,
        protocol_version: ProtocolVersion,
        content_key_version: ContentKeyVersion,
        valid_from: CanonicalTime,
        valid_until: CanonicalTime,
    ) -> Result<SignedContentKeyCertificate, SecurityError> {
        let ActorRef::Sender(owner) = self.actor else {
            return Err(SecurityError::SigningScopeMismatch);
        };
        self.command_signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version,
                deployment_domain: scope.deployment_domain,
                intended_provider: scope.intended_provider,
                relationship: scope.relationship,
                owner,
                operational_key: self.command_signer.reference(),
                content_key_version,
                key: self.content_public,
                valid_from,
                valid_until,
            })
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

    /// Computes the declaration digest used by contact terms and bonded admission.
    ///
    /// # Errors
    ///
    /// Returns an error when the declaration set is invalid.
    pub fn declaration_digest(
        declarations: &MessageDeclarations,
    ) -> Result<MessageDeclarationDigest, ContentError> {
        message_declaration_digest(declarations)
    }

    /// Applies recipient-owned presentation policy without changing protocol state.
    pub fn presentation_disposition<P: PresentationPolicy>(
        policy: &P,
        record: &EncryptedContentRecord,
    ) -> PresentationDisposition {
        policy.decide(&PresentationPolicyInput {
            sender: record.binding.sender,
            declarations: &record.binding.declarations,
        })
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
    use cs_mail_content::ContentCertificateDigest;
    use cs_mail_primitives::{
        ContentRef, ContentScopeRef, DeclarationAuthority, DeclaredPurpose, KnownPurpose,
        MessageDeclarations, MessageId, MessageValidityUntil, OriginDeclaration, OriginMode,
        ProtocolIdentity, RelationshipRef,
    };

    struct DigestPersonal;

    impl PresentationPolicy for DigestPersonal {
        fn decide(&self, input: &PresentationPolicyInput<'_>) -> PresentationDisposition {
            if input.declarations.purpose == DeclaredPurpose::Known(KnownPurpose::Personal) {
                PresentationDisposition::Digest
            } else {
                PresentationDisposition::Inbox
            }
        }
    }

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
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(3),
                    message_id: MessageId(4),
                    sender: ProtocolIdentity(1),
                    recipient: ProtocolIdentity(2),
                    protocol_version: ProtocolVersion(1),
                    relationship: RelationshipRef::from_u128_for_test(5),
                    content_scope: ContentScopeRef::from_u128_for_test(6),
                    sender_certificate: ContentCertificateDigest([7; 32]),
                    declarations: MessageDeclarations {
                        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
                        origin: OriginDeclaration {
                            mode: OriginMode::HumanInitiated,
                            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
                        },
                        payload_schema: None,
                    },
                    message_valid_until: MessageValidityUntil(CanonicalTime(8)),
                    capability: None,
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

        assert_eq!(
            NativeClient::presentation_disposition(&DigestPersonal, &record),
            PresentationDisposition::Digest
        );
    }
}
