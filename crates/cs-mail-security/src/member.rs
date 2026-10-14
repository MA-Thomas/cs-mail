//! Signed member publications and reads that are not relationship commands: an address's
//! endpoint content key, and a member's authenticated view of its own records.
use super::{OperationalKeyRef, RecipientSigningScope, SecurityError, Signature, VerifyingKey};
use cs_mail_content::EndpointPublicKey;
use cs_mail_primitives::{CanonicalTime, Duration, ProtocolIdentity};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

/// The content key senders encrypt to when writing to `persona`. Versions increase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointKeyPublication {
    pub persona: ProtocolIdentity,
    pub version: u64,
    pub key: EndpointPublicKey,
}

/// An endpoint key publication signed by an active key of its address.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEndpointKey {
    pub scope: RecipientSigningScope,
    pub operational_key: OperationalKeyRef,
    pub publication: EndpointKeyPublication,
    pub signature: Vec<u8>,
}
impl SignedEndpointKey {
    fn bytes(&self) -> Result<Vec<u8>, SecurityError> {
        serde_json::to_vec(&(
            "cs-mail/endpoint-key/v1",
            self.scope,
            self.operational_key,
            &self.publication,
        ))
        .map_err(|_| SecurityError::InvalidSignature)
    }
    /// # Errors
    /// Rejects version zero, a zero persona and unencodable input.
    pub fn sign(
        scope: RecipientSigningScope,
        operational_key: OperationalKeyRef,
        publication: EndpointKeyPublication,
        secret: &[u8; 32],
    ) -> Result<Self, SecurityError> {
        if publication.version == 0 || publication.persona.0 == 0 {
            return Err(SecurityError::InvalidSignature);
        }
        let mut signed = Self {
            scope,
            operational_key,
            publication,
            signature: Vec::new(),
        };
        signed.signature = SigningKey::from_bytes(secret)
            .sign(&signed.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(signed)
    }
    /// # Errors
    /// Rejects a signature that does not cover exactly this publication and scope.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), SecurityError> {
        verify(key, &self.bytes()?, &self.signature)
    }
}

/// What a member asks to see. Every answer is limited to records of `persona`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum MemberRead {
    /// The member's account: identifiers, payment method, keys and service coverage.
    Account,
    /// The relationship in which `persona` sends to `recipient`.
    AsSender { recipient: ProtocolIdentity },
    /// The relationship in which `sender` writes to `persona`, with the content addressed
    /// to `persona`.
    AsRecipient { sender: ProtocolIdentity },
    /// Every relationship in which someone writes to `persona`.
    Inbox,
    /// Every relationship in which `persona` writes to someone else.
    Outbox,
}

/// A read signed by an active key of `persona`, valid briefly after `issued_at`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMemberRead {
    pub scope: RecipientSigningScope,
    pub persona: ProtocolIdentity,
    pub operational_key: OperationalKeyRef,
    pub issued_at: CanonicalTime,
    pub read: MemberRead,
    pub signature: Vec<u8>,
}
impl SignedMemberRead {
    /// How long after `issued_at`, and how far before it, a read is accepted.
    pub const FRESHNESS: Duration = Duration(300_000);

    fn bytes(&self) -> Result<Vec<u8>, SecurityError> {
        serde_json::to_vec(&(
            "cs-mail/member-read/v1",
            self.scope,
            self.persona,
            self.operational_key,
            self.issued_at,
            &self.read,
        ))
        .map_err(|_| SecurityError::InvalidSignature)
    }
    /// # Errors
    /// Rejects a zero persona and unencodable input.
    pub fn sign(
        scope: RecipientSigningScope,
        persona: ProtocolIdentity,
        operational_key: OperationalKeyRef,
        issued_at: CanonicalTime,
        read: MemberRead,
        secret: &[u8; 32],
    ) -> Result<Self, SecurityError> {
        if persona.0 == 0 {
            return Err(SecurityError::InvalidSignature);
        }
        let mut signed = Self {
            scope,
            persona,
            operational_key,
            issued_at,
            read,
            signature: Vec::new(),
        };
        signed.signature = SigningKey::from_bytes(secret)
            .sign(&signed.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(signed)
    }
    /// Verifies the signature and that `now` lies within [`Self::FRESHNESS`] of issuance.
    /// # Errors
    /// `KeyNotValidAtReceipt` for a stale or future read; `InvalidSignature` otherwise.
    pub fn verify(&self, key: &[u8; 32], now: CanonicalTime) -> Result<(), SecurityError> {
        let earliest = CanonicalTime(self.issued_at.0.saturating_sub(Self::FRESHNESS.0));
        let latest = self
            .issued_at
            .checked_add(Self::FRESHNESS)
            .ok_or(SecurityError::KeyNotValidAtReceipt)?;
        if now < earliest || now > latest {
            return Err(SecurityError::KeyNotValidAtReceipt);
        }
        verify(key, &self.bytes()?, &self.signature)
    }
}

fn verify(key: &[u8; 32], bytes: &[u8], signature: &[u8]) -> Result<(), SecurityError> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| SecurityError::InvalidSignature)?;
    let signature =
        Signature::from_slice(signature).map_err(|_| SecurityError::InvalidSignature)?;
    key.verify_strict(bytes, &signature)
        .map_err(|_| SecurityError::InvalidSignature)
}
