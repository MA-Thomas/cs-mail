//! Endpoint encryption and provider-opaque ciphertext records for native mail.

use core::fmt;

use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentRef, MessageId, ProtocolIdentity, ProtocolVersion,
};
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256, setup_receiver, setup_sender,
};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    pub content_ref: ContentRef,
    pub message_id: MessageId,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub protocol_version: ProtocolVersion,
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
    if expires_at <= created_at {
        return Err(ContentError::InvalidLifetime);
    }
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
        .seal(plaintext, &binding_bytes(binding))
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
    context
        .open(&record.envelope.ciphertext, &binding_bytes(record.binding))
        .map_err(|_| ContentError::InvalidCiphertext)
}

fn binding_bytes(binding: ContentBinding) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(83);
    bytes.extend_from_slice(b"cs-mail/content-binding/v1");
    bytes.extend_from_slice(&binding.content_ref.0.to_be_bytes());
    bytes.extend_from_slice(&binding.message_id.0.to_be_bytes());
    bytes.extend_from_slice(&binding.sender.0.to_be_bytes());
    bytes.extend_from_slice(&binding.recipient.0.to_be_bytes());
    bytes.extend_from_slice(&binding.protocol_version.0.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ContentBinding {
        ContentBinding {
            content_ref: ContentRef(1),
            message_id: MessageId(2),
            sender: ProtocolIdentity(3),
            recipient: ProtocolIdentity(4),
            protocol_version: ProtocolVersion(1),
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

        let mut changed_binding = record;
        changed_binding.binding.recipient = ProtocolIdentity(99);
        assert_eq!(
            decrypt(&secret, sender_public, &changed_binding),
            Err(ContentError::InvalidCiphertext)
        );
    }
}
