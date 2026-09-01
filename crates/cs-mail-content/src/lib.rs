//! Endpoint encryption and provider-opaque ciphertext records for native mail.

use core::fmt;

use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef, ContentScopeRef, LaneId,
    MessageDeclarationDigest, MessageDeclarations, MessageId, MessageValidityUntil,
    OperationalKeyRef, ProtocolIdentity, ProtocolVersion, ProviderRef, RelationshipRef,
    WireVersion,
};
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256, setup_receiver, setup_sender,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type Kem = X25519HkdfSha256;
type Kdf = HkdfSha256;
type Aead = ChaCha20Poly1305;

const INFO: &[u8] = b"cs-mail/native-content/v1";
pub const MAX_PLAINTEXT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ContentCipherSuite {
    HpkeX25519HkdfSha256ChaCha20Poly1305V1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EndpointPublicKey {
    pub reference: ContentKeyRef,
    pub bytes: [u8; 32],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentKeyCertificate {
    pub wire_version: WireVersion,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub relationship: RelationshipRef,
    pub owner: ProtocolIdentity,
    pub operational_key: OperationalKeyRef,
    pub content_key_version: ContentKeyVersion,
    pub key: EndpointPublicKey,
    pub valid_from: CanonicalTime,
    pub valid_until: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentCertificateDigest(pub [u8; 32]);

/// An endpoint-only secret. It deliberately has no serialization or `Debug` implementation.
pub struct EndpointSecretKey {
    reference: ContentKeyRef,
    private: <Kem as KemTrait>::PrivateKey,
}

impl EndpointSecretKey {
    pub fn generate(reference: ContentKeyRef) -> (Self, EndpointPublicKey) {
        let (private, public) = Kem::gen_keypair();
        let bytes = public.to_bytes();
        let mut public_bytes = [0_u8; 32];
        public_bytes.copy_from_slice(bytes.as_slice());
        (
            Self { reference, private },
            EndpointPublicKey {
                reference,
                bytes: public_bytes,
            },
        )
    }

    pub const fn reference(&self) -> ContentKeyRef {
        self.reference
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    pub wire_version: WireVersion,
    pub content_ref: ContentRef,
    pub message_id: MessageId,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub protocol_version: ProtocolVersion,
    pub relationship: RelationshipRef,
    pub content_scope: ContentScopeRef,
    pub sender_certificate: ContentCertificateDigest,
    pub declarations: MessageDeclarations,
    pub message_valid_until: MessageValidityUntil,
    pub capability: Option<LaneId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CiphertextEnvelope {
    pub suite: ContentCipherSuite,
    pub sender_key: ContentKeyRef,
    pub recipient_key: ContentKeyRef,
    pub encapsulated_key: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedContentRecord {
    pub binding: ContentBinding,
    pub envelope: CiphertextEnvelope,
    pub created_at: CanonicalTime,
    pub expires_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentError {
    PlaintextTooLarge,
    InvalidPublicKey,
    WrongRecipientKey,
    InvalidCiphertext,
    InvalidLifetime,
    InvalidDeclaration,
    UnsupportedWireVersion,
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlaintextTooLarge => formatter.write_str("native content exceeds the size limit"),
            Self::InvalidPublicKey => formatter.write_str("invalid endpoint content public key"),
            Self::WrongRecipientKey => {
                formatter.write_str("ciphertext targets another endpoint key")
            }
            Self::InvalidCiphertext => formatter.write_str("content authentication failed"),
            Self::InvalidLifetime => formatter.write_str("content expiry must follow creation"),
            Self::InvalidDeclaration => formatter.write_str("message declarations are invalid"),
            Self::UnsupportedWireVersion => {
                formatter.write_str("message binding wire version is unsupported")
            }
        }
    }
}

impl std::error::Error for ContentError {}

/// Encrypts native content for one recipient endpoint and authenticates its protocol binding.
///
/// # Errors
///
/// Returns an error for oversized input or an invalid recipient key.
pub fn encrypt(
    sender_key: &EndpointSecretKey,
    recipient_key: EndpointPublicKey,
    binding: ContentBinding,
    plaintext: &[u8],
    created_at: CanonicalTime,
    expires_at: CanonicalTime,
) -> Result<EncryptedContentRecord, ContentError> {
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(ContentError::PlaintextTooLarge);
    }
    if expires_at <= created_at || binding.message_valid_until.0 <= created_at {
        return Err(ContentError::InvalidLifetime);
    }
    validate_binding(&binding)?;
    let public = <Kem as KemTrait>::PublicKey::from_bytes(&recipient_key.bytes)
        .map_err(|_| ContentError::InvalidPublicKey)?;
    let sender_public = Kem::sk_to_pk(&sender_key.private);
    let (encapsulated, mut context) = setup_sender::<Aead, Kdf, Kem>(
        &OpModeS::Auth((sender_key.private.clone(), sender_public)),
        &public,
        INFO,
    )
    .map_err(|_| ContentError::InvalidPublicKey)?;
    let ciphertext = context
        .seal(plaintext, &binding_bytes(&binding)?)
        .map_err(|_| ContentError::InvalidCiphertext)?;
    Ok(EncryptedContentRecord {
        binding,
        envelope: CiphertextEnvelope {
            suite: ContentCipherSuite::HpkeX25519HkdfSha256ChaCha20Poly1305V1,
            sender_key: sender_key.reference,
            recipient_key: recipient_key.reference,
            encapsulated_key: encapsulated.to_bytes().as_slice().to_vec(),
            ciphertext,
        },
        created_at,
        expires_at,
    })
}

/// Authenticates and decrypts native content at the recipient endpoint.
///
/// # Errors
///
/// Returns an error for another endpoint key, malformed data, or authentication failure.
pub fn decrypt(
    recipient_key: &EndpointSecretKey,
    sender_key: EndpointPublicKey,
    record: &EncryptedContentRecord,
) -> Result<Vec<u8>, ContentError> {
    if recipient_key.reference != record.envelope.recipient_key
        || sender_key.reference != record.envelope.sender_key
    {
        return Err(ContentError::WrongRecipientKey);
    }
    let sender_public = <Kem as KemTrait>::PublicKey::from_bytes(&sender_key.bytes)
        .map_err(|_| ContentError::InvalidPublicKey)?;
    let encapsulated =
        <Kem as KemTrait>::EncappedKey::from_bytes(&record.envelope.encapsulated_key)
            .map_err(|_| ContentError::InvalidCiphertext)?;
    let mut context = setup_receiver::<Aead, Kdf, Kem>(
        &OpModeR::Auth(sender_public),
        &recipient_key.private,
        &encapsulated,
        INFO,
    )
    .map_err(|_| ContentError::InvalidCiphertext)?;
    validate_binding(&record.binding)?;
    context
        .open(
            &record.envelope.ciphertext,
            &binding_bytes(&record.binding)?,
        )
        .map_err(|_| ContentError::InvalidCiphertext)
}

fn validate_binding(binding: &ContentBinding) -> Result<(), ContentError> {
    if binding.wire_version != WireVersion(1) {
        return Err(ContentError::UnsupportedWireVersion);
    }
    binding
        .declarations
        .validate()
        .map_err(|_| ContentError::InvalidDeclaration)
}

fn binding_bytes(binding: &ContentBinding) -> Result<Vec<u8>, ContentError> {
    validate_binding(binding)?;
    let mut bytes = Vec::with_capacity(320);
    bytes.extend_from_slice(b"cs-mail/content-binding/v2");
    bytes.extend_from_slice(&binding.wire_version.0.to_be_bytes());
    bytes.extend_from_slice(&binding.content_ref.0.to_be_bytes());
    bytes.extend_from_slice(&binding.message_id.0.to_be_bytes());
    bytes.extend_from_slice(&binding.sender.0.to_be_bytes());
    bytes.extend_from_slice(&binding.recipient.0.to_be_bytes());
    bytes.extend_from_slice(&binding.protocol_version.0.to_be_bytes());
    bytes.extend_from_slice(&binding.relationship.derivation_version().to_be_bytes());
    bytes.extend_from_slice(binding.relationship.as_bytes());
    bytes.extend_from_slice(&binding.content_scope.derivation_version().to_be_bytes());
    bytes.extend_from_slice(binding.content_scope.as_bytes());
    bytes.extend_from_slice(&binding.sender_certificate.0);
    bytes.extend_from_slice(
        &binding
            .declarations
            .canonical_bytes()
            .map_err(|_| ContentError::InvalidDeclaration)?,
    );
    bytes.extend_from_slice(&binding.message_valid_until.0.0.to_be_bytes());
    match binding.capability {
        Some(lane_id) => {
            bytes.push(1);
            bytes.extend_from_slice(&lane_id.0.to_be_bytes());
        }
        None => bytes.push(0),
    }
    Ok(bytes)
}

/// Computes the canonical declaration digest fixed by contact terms and admission.
///
/// # Errors
///
/// Returns an error when the declaration set is invalid.
pub fn message_declaration_digest(
    declarations: &MessageDeclarations,
) -> Result<MessageDeclarationDigest, ContentError> {
    let bytes = declarations
        .canonical_bytes()
        .map_err(|_| ContentError::InvalidDeclaration)?;
    Ok(MessageDeclarationDigest(Sha256::digest(bytes).into()))
}

pub fn content_key_certificate_bytes(certificate: &ContentKeyCertificate) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(224);
    bytes.extend_from_slice(b"cs-mail/content-key-certificate/v1");
    bytes.extend_from_slice(&certificate.wire_version.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.protocol_version.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.deployment_domain);
    bytes.extend_from_slice(&certificate.intended_provider.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.relationship.derivation_version().to_be_bytes());
    bytes.extend_from_slice(certificate.relationship.as_bytes());
    bytes.extend_from_slice(&certificate.owner.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.operational_key.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.content_key_version.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.key.reference.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.key.bytes);
    bytes.extend_from_slice(&certificate.valid_from.0.to_be_bytes());
    bytes.extend_from_slice(&certificate.valid_until.0.to_be_bytes());
    bytes
}

pub fn content_key_certificate_digest(
    certificate: &ContentKeyCertificate,
) -> ContentCertificateDigest {
    ContentCertificateDigest(Sha256::digest(content_key_certificate_bytes(certificate)).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ContentBinding {
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: ContentRef(1),
            message_id: MessageId(2),
            sender: ProtocolIdentity(3),
            recipient: ProtocolIdentity(4),
            protocol_version: ProtocolVersion(1),
            relationship: RelationshipRef::from_u128_for_test(5),
            content_scope: ContentScopeRef::from_u128_for_test(6),
            sender_certificate: ContentCertificateDigest([7; 32]),
            declarations: MessageDeclarations {
                purpose: cs_mail_primitives::DeclaredPurpose::Known(
                    cs_mail_primitives::KnownPurpose::Personal,
                ),
                origin: cs_mail_primitives::OriginDeclaration {
                    mode: cs_mail_primitives::OriginMode::HumanInitiated,
                    authority: cs_mail_primitives::DeclarationAuthority::NativeSender(
                        OperationalKeyRef(7),
                    ),
                },
                payload_schema: None,
            },
            message_valid_until: MessageValidityUntil(CanonicalTime(50)),
            capability: None,
        }
    }

    #[test]
    fn only_target_endpoint_can_decrypt() {
        let (sender_secret, sender_public) = EndpointSecretKey::generate(ContentKeyRef(8));
        let (recipient_secret, recipient_public) = EndpointSecretKey::generate(ContentKeyRef(9));
        let (other_secret, _) = EndpointSecretKey::generate(ContentKeyRef(10));
        let record = encrypt(
            &sender_secret,
            recipient_public,
            binding(),
            b"subject and body",
            CanonicalTime(1),
            CanonicalTime(100),
        )
        .unwrap();
        assert_eq!(
            decrypt(&recipient_secret, sender_public, &record).unwrap(),
            b"subject and body"
        );
        assert_eq!(
            decrypt(&other_secret, sender_public, &record),
            Err(ContentError::WrongRecipientKey)
        );
    }

    #[test]
    fn content_and_binding_tampering_fail_authentication() {
        let (sender_secret, sender_public) = EndpointSecretKey::generate(ContentKeyRef(8));
        let (secret, public) = EndpointSecretKey::generate(ContentKeyRef(9));
        let record = encrypt(
            &sender_secret,
            public,
            binding(),
            b"message",
            CanonicalTime(1),
            CanonicalTime(100),
        )
        .unwrap();

        let mut changed_content = record.clone();
        changed_content.envelope.ciphertext[0] ^= 1;
        assert_eq!(
            decrypt(&secret, sender_public, &changed_content),
            Err(ContentError::InvalidCiphertext)
        );

        let mut changed_binding = record.clone();
        changed_binding.binding.recipient = ProtocolIdentity(99);
        assert_eq!(
            decrypt(&secret, sender_public, &changed_binding),
            Err(ContentError::InvalidCiphertext)
        );

        let mut changed_declaration = record.clone();
        changed_declaration.binding.declarations.purpose =
            cs_mail_primitives::DeclaredPurpose::Known(
                cs_mail_primitives::KnownPurpose::Transactional,
            );
        assert_eq!(
            decrypt(&secret, sender_public, &changed_declaration),
            Err(ContentError::InvalidCiphertext)
        );

        let mut changed_validity = record.clone();
        changed_validity.binding.message_valid_until = MessageValidityUntil(CanonicalTime(49));
        assert_eq!(
            decrypt(&secret, sender_public, &changed_validity),
            Err(ContentError::InvalidCiphertext)
        );

        let mut changed_capability = record;
        changed_capability.binding.capability = Some(LaneId(1));
        assert_eq!(
            decrypt(&secret, sender_public, &changed_capability),
            Err(ContentError::InvalidCiphertext)
        );
    }
}
