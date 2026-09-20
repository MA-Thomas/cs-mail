//! Operational-key signing and verification for cs-mail commands.

mod authority;
pub use authority::AuthoritySnapshot;
mod pricing;
pub use pricing::SignedCollateralPreference;

use std::collections::BTreeMap;
use std::fmt;

use cs_mail_content::{
    ContentKeyCertificate, content_key_certificate_bytes, content_key_certificate_digest,
};
use cs_mail_primitives::{
    CanonicalTime, JournalPosition, OperationalKeyRef, ProtocolVersion, ProviderRef, ReceiptRef,
    RecoveryAttemptRef, RecoveryFactorRef, RelationshipRef, Version, WireVersion,
};
use cs_mail_protocol::{ActorRef, KernelCommand, ProtocolCommand, RequestTerms};
use cs_mail_wire::{
    CanonicalCommandEnvelope, CommandTarget, WireError, decode_command_envelope,
    encode_command_envelope, encode_contact_terms_artifact,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CommandDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct OutcomeDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SigningScope {
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub relationship: RelationshipRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedCommandBytes {
    pub payload: Vec<u8>,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedContentKeyCertificate {
    pub certificate: ContentKeyCertificate,
    pub signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum KeyStatus {
    Active,
    Retired { valid_until: CanonicalTime },
    Revoked { revoked_at: CanonicalTime },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationalKeyRecord {
    pub reference: OperationalKeyRef,
    pub actor: ActorRef,
    pub verifying_key: [u8; 32],
    pub valid_from: CanonicalTime,
    pub status: KeyStatus,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TransparencyAction {
    Registered,
    Retired,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransparencyEntry {
    pub sequence: u64,
    pub key: OperationalKeyRef,
    pub actor: ActorRef,
    pub verifying_key: [u8; 32],
    pub action: TransparencyAction,
    pub at: CanonicalTime,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyTransparencyLog {
    entries: Vec<TransparencyEntry>,
}

impl KeyTransparencyLog {
    pub fn entries(&self) -> &[TransparencyEntry] {
        &self.entries
    }

    pub fn checkpoint(&self) -> Option<(u64, [u8; 32])> {
        self.entries
            .last()
            .map(|entry| (entry.sequence, entry.hash))
    }

    fn append(
        &mut self,
        record: &OperationalKeyRecord,
        action: TransparencyAction,
        at: CanonicalTime,
    ) {
        let previous_hash = self.entries.last().map_or([0; 32], |entry| entry.hash);
        let sequence = u64::try_from(self.entries.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut hasher = Sha256::new();
        hasher.update(b"cs-mail/key-transparency/v1");
        hasher.update(sequence.to_be_bytes());
        hasher.update(record.reference.0.to_be_bytes());
        hash_actor(&mut hasher, record.actor);
        hasher.update(record.verifying_key);
        hasher.update([match action {
            TransparencyAction::Registered => 0,
            TransparencyAction::Retired => 1,
            TransparencyAction::Revoked => 2,
        }]);
        hasher.update(at.0.to_be_bytes());
        hasher.update(previous_hash);
        let hash = hasher.finalize().into();
        self.entries.push(TransparencyEntry {
            sequence,
            key: record.reference,
            actor: record.actor,
            verifying_key: record.verifying_key,
            action,
            at,
            previous_hash,
            hash,
        });
    }

    pub fn verify(&self) -> bool {
        let mut previous = [0_u8; 32];
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.sequence != u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1)
                || entry.previous_hash != previous
            {
                return false;
            }
            let mut hasher = Sha256::new();
            hasher.update(b"cs-mail/key-transparency/v1");
            hasher.update(entry.sequence.to_be_bytes());
            hasher.update(entry.key.0.to_be_bytes());
            hash_actor(&mut hasher, entry.actor);
            hasher.update(entry.verifying_key);
            hasher.update([match entry.action {
                TransparencyAction::Registered => 0,
                TransparencyAction::Retired => 1,
                TransparencyAction::Revoked => 2,
            }]);
            hasher.update(entry.at.0.to_be_bytes());
            hasher.update(previous);
            if <[u8; 32]>::from(hasher.finalize()) != entry.hash {
                return false;
            }
            previous = entry.hash;
        }
        true
    }
}

fn hash_actor(hasher: &mut Sha256, actor: ActorRef) {
    match actor {
        ActorRef::Sender(identity) => {
            hasher.update([0]);
            hasher.update(identity.0.to_be_bytes());
        }
        ActorRef::Recipient(identity) => {
            hasher.update([1]);
            hasher.update(identity.0.to_be_bytes());
        }
        ActorRef::Provider(provider) => {
            hasher.update([2]);
            hasher.update(provider.0.to_be_bytes());
        }
        ActorRef::Scheduler(provider) => {
            hasher.update([3]);
            hasher.update(provider.0.to_be_bytes());
        }
    }
}

/// Authentication evidence cannot be fabricated by deserializing caller data.
/// ```compile_fail
/// fn accepts_json<T: for<'de> serde::Deserialize<'de>>() {}
/// accepts_json::<cs_mail_security::VerifiedCommand>();
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCommand {
    authorized: KernelCommand<ProtocolCommand>,
    target: RelationshipRef,
    digest: CommandDigest,
    canonical_bytes: Vec<u8>,
}

impl VerifiedCommand {
    pub const fn target(&self) -> RelationshipRef {
        self.target
    }

    pub const fn digest(&self) -> CommandDigest {
        self.digest
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub fn into_command(self) -> KernelCommand<ProtocolCommand> {
        self.authorized
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecurityError {
    Wire(WireError),
    UnknownKey,
    DuplicateKey,
    ActorMismatch,
    KeyNotYetValid,
    KeyNotValidAtReceipt,
    InvalidPublicKey,
    InvalidSignature,
    WireVersionMismatch,
    ProtocolVersionMismatch,
    SigningScopeMismatch,
    VersionConflict,
    InvalidRecoveryPolicy,
    UnknownRecoveryFactor,
    RecoveryAlreadyConsumed,
}

impl fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wire(error) => write!(formatter, "wire error: {error}"),
            Self::UnknownKey => formatter.write_str("unknown operational key"),
            Self::DuplicateKey => formatter.write_str("operational key already exists"),
            Self::ActorMismatch => formatter.write_str("operational key does not belong to actor"),
            Self::KeyNotYetValid => formatter.write_str("operational key is not yet valid"),
            Self::KeyNotValidAtReceipt => {
                formatter.write_str("operational key is not valid at canonical receipt time")
            }
            Self::InvalidPublicKey => formatter.write_str("invalid Ed25519 public key"),
            Self::InvalidSignature => formatter.write_str("invalid Ed25519 signature"),
            Self::WireVersionMismatch => formatter.write_str("wire version mismatch"),
            Self::ProtocolVersionMismatch => formatter.write_str("protocol version mismatch"),
            Self::SigningScopeMismatch => formatter.write_str("command signing scope mismatch"),
            Self::VersionConflict => formatter.write_str("key registry version conflict"),
            Self::InvalidRecoveryPolicy => formatter.write_str("invalid recovery policy"),
            Self::UnknownRecoveryFactor => formatter.write_str("unknown recovery factor"),
            Self::RecoveryAlreadyConsumed => {
                formatter.write_str("recovery attempt already consumed")
            }
        }
    }
}

impl std::error::Error for SecurityError {}

impl From<WireError> for SecurityError {
    fn from(value: WireError) -> Self {
        Self::Wire(value)
    }
}

pub struct CommandSigner {
    actor: ActorRef,
    reference: OperationalKeyRef,
    signing_key: SigningKey,
}

impl CommandSigner {
    pub fn from_secret_bytes(
        actor: ActorRef,
        reference: OperationalKeyRef,
        secret: &[u8; 32],
    ) -> Self {
        Self {
            actor,
            reference,
            signing_key: SigningKey::from_bytes(secret),
        }
    }

    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub const fn reference(&self) -> OperationalKeyRef {
        self.reference
    }

    /// Signs one versioned canonical command envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if canonical encoding fails.
    pub fn sign(
        &self,
        scope: SigningScope,
        protocol_version: ProtocolVersion,
        idempotency_key: cs_mail_primitives::IdempotencyKey,
        command: ProtocolCommand,
    ) -> Result<SignedCommandBytes, SecurityError> {
        let payload = encode_command_envelope(&CanonicalCommandEnvelope {
            wire_version: WireVersion(6),
            protocol_version,
            deployment_domain: scope.deployment_domain,
            intended_provider: scope.intended_provider,
            target: CommandTarget::Relationship(scope.relationship),
            actor: self.actor,
            operational_key: self.reference,
            idempotency_key,
            command,
        })?;
        let signature = self.signing_key.sign(&payload).to_bytes();
        Ok(SignedCommandBytes { payload, signature })
    }

    /// Binds an endpoint content key to this operational signing identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the certificate owner, key, or lifetime is invalid.
    pub fn sign_content_key_certificate(
        &self,
        certificate: ContentKeyCertificate,
    ) -> Result<SignedContentKeyCertificate, SecurityError> {
        if certificate.operational_key != self.reference
            || self.actor != ActorRef::Sender(certificate.owner)
            || certificate.valid_until <= certificate.valid_from
        {
            return Err(SecurityError::SigningScopeMismatch);
        }
        Ok(SignedContentKeyCertificate {
            signature: self
                .signing_key
                .sign(&content_key_certificate_bytes(&certificate))
                .to_bytes(),
            certificate,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyRegistry {
    keys: BTreeMap<OperationalKeyRef, OperationalKeyRecord>,
    version: Version,
    transparency: KeyTransparencyLog,
}

impl KeyRegistry {
    pub fn contains_key(&self, reference: OperationalKeyRef) -> bool {
        self.keys.contains_key(&reference)
    }
    /// Read-only records for validating ownership when installing a registry.
    pub fn records(&self) -> impl Iterator<Item = &OperationalKeyRecord> {
        self.keys.values()
    }

    /// Resolves current key ownership; callers still enforce purpose-specific grants.
    /// # Errors
    /// Rejects unknown, revoked, or not-yet-valid operational keys.
    pub fn active_actor(
        &self,
        reference: OperationalKeyRef,
        at: CanonicalTime,
    ) -> Result<(ActorRef, [u8; 32]), SecurityError> {
        let actor = self
            .keys
            .get(&reference)
            .ok_or(SecurityError::UnknownKey)?
            .actor;
        Ok((actor, self.active_verifying_key(reference, actor, at)?))
    }
    pub fn version(&self) -> Version {
        self.version
    }

    pub const fn transparency(&self) -> &KeyTransparencyLog {
        &self.transparency
    }

    /// Returns an actor-scoped key only when it is valid at canonical receipt time.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing key, wrong actor, or invalid lifetime.
    pub fn active_verifying_key(
        &self,
        reference: OperationalKeyRef,
        expected_actor: ActorRef,
        at: CanonicalTime,
    ) -> Result<[u8; 32], SecurityError> {
        let key = self.keys.get(&reference).ok_or(SecurityError::UnknownKey)?;
        if key.actor != expected_actor {
            return Err(SecurityError::ActorMismatch);
        }
        if at < key.valid_from {
            return Err(SecurityError::KeyNotYetValid);
        }
        match key.status {
            KeyStatus::Active => Ok(key.verifying_key),
            KeyStatus::Retired { valid_until } if at <= valid_until => Ok(key.verifying_key),
            KeyStatus::Retired { .. } | KeyStatus::Revoked { .. } => {
                Err(SecurityError::KeyNotValidAtReceipt)
            }
        }
    }

    /// Registers a new active operational key.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate keys or version overflow.
    pub fn register(
        &mut self,
        reference: OperationalKeyRef,
        actor: ActorRef,
        verifying_key: [u8; 32],
        valid_from: CanonicalTime,
    ) -> Result<(), SecurityError> {
        if self.keys.contains_key(&reference) {
            return Err(SecurityError::DuplicateKey);
        }
        VerifyingKey::from_bytes(&verifying_key).map_err(|_| SecurityError::InvalidPublicKey)?;
        self.version = next_version(self.version)?;
        let record = OperationalKeyRecord {
            reference,
            actor,
            verifying_key,
            valid_from,
            status: KeyStatus::Active,
            version: Version(0),
        };
        self.keys.insert(reference, record.clone());
        self.transparency
            .append(&record, TransparencyAction::Registered, valid_from);
        Ok(())
    }

    /// Retires one key and atomically registers its replacement.
    ///
    /// # Errors
    ///
    /// Returns an error for missing, inactive, duplicate, mismatched, or
    /// overflowing key state.
    pub fn rotate(
        &mut self,
        old_reference: OperationalKeyRef,
        expected_old_version: Version,
        new_reference: OperationalKeyRef,
        new_verifying_key: [u8; 32],
        at: CanonicalTime,
    ) -> Result<(), SecurityError> {
        if self.keys.contains_key(&new_reference) {
            return Err(SecurityError::DuplicateKey);
        }
        VerifyingKey::from_bytes(&new_verifying_key)
            .map_err(|_| SecurityError::InvalidPublicKey)?;
        let old = self
            .keys
            .get_mut(&old_reference)
            .ok_or(SecurityError::UnknownKey)?;
        if old.version != expected_old_version {
            return Err(SecurityError::VersionConflict);
        }
        if old.status != KeyStatus::Active {
            return Err(SecurityError::KeyNotValidAtReceipt);
        }
        if at < old.valid_from {
            return Err(SecurityError::KeyNotYetValid);
        }
        let actor = old.actor;
        old.status = KeyStatus::Retired { valid_until: at };
        old.version = next_version(old.version)?;
        let retired = old.clone();
        self.version = next_version(self.version)?;
        let replacement = OperationalKeyRecord {
            reference: new_reference,
            actor,
            verifying_key: new_verifying_key,
            valid_from: at,
            status: KeyStatus::Active,
            version: Version(0),
        };
        self.keys.insert(new_reference, replacement.clone());
        self.transparency
            .append(&retired, TransparencyAction::Retired, at);
        self.transparency
            .append(&replacement, TransparencyAction::Registered, at);
        Ok(())
    }

    /// Revokes a key at a canonical time.
    ///
    /// # Errors
    ///
    /// Returns an error for missing keys, stale versions, or version overflow.
    pub fn revoke(
        &mut self,
        reference: OperationalKeyRef,
        expected_version: Version,
        at: CanonicalTime,
    ) -> Result<(), SecurityError> {
        let key = self
            .keys
            .get_mut(&reference)
            .ok_or(SecurityError::UnknownKey)?;
        if key.version != expected_version {
            return Err(SecurityError::VersionConflict);
        }
        if at < key.valid_from {
            return Err(SecurityError::KeyNotYetValid);
        }
        key.status = KeyStatus::Revoked { revoked_at: at };
        key.version = next_version(key.version)?;
        let revoked = key.clone();
        self.version = next_version(self.version)?;
        self.transparency
            .append(&revoked, TransparencyAction::Revoked, at);
        Ok(())
    }

    /// Verifies canonical encoding, key scope, key lifetime, and signature.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed input, wrong versions or actors, invalid
    /// key state, invalid public keys, or invalid signatures.
    pub fn verify(
        &self,
        signed: &SignedCommandBytes,
        receipt_time: CanonicalTime,
        expected_protocol_version: ProtocolVersion,
        expected_scope: SigningScope,
    ) -> Result<VerifiedCommand, SecurityError> {
        let envelope = decode_command_envelope(&signed.payload)?;
        if envelope.wire_version != WireVersion(6) {
            return Err(SecurityError::WireVersionMismatch);
        }
        if envelope.protocol_version != expected_protocol_version {
            return Err(SecurityError::ProtocolVersionMismatch);
        }
        if envelope.deployment_domain != expected_scope.deployment_domain
            || envelope.intended_provider != expected_scope.intended_provider
            || envelope.target != CommandTarget::Relationship(expected_scope.relationship)
        {
            return Err(SecurityError::SigningScopeMismatch);
        }
        let key = self
            .keys
            .get(&envelope.operational_key)
            .ok_or(SecurityError::UnknownKey)?;
        if key.actor != envelope.actor {
            return Err(SecurityError::ActorMismatch);
        }
        if receipt_time < key.valid_from {
            return Err(SecurityError::KeyNotYetValid);
        }
        let valid = match key.status {
            KeyStatus::Active => true,
            KeyStatus::Retired { valid_until } => receipt_time <= valid_until,
            KeyStatus::Revoked { revoked_at } => receipt_time < revoked_at,
        };
        if !valid {
            return Err(SecurityError::KeyNotValidAtReceipt);
        }
        let verifying_key = VerifyingKey::from_bytes(&key.verifying_key)
            .map_err(|_| SecurityError::InvalidPublicKey)?;
        let signature = Signature::from_bytes(&signed.signature);
        verifying_key
            .verify_strict(&signed.payload, &signature)
            .map_err(|_| SecurityError::InvalidSignature)?;
        let digest = CommandDigest(Sha256::digest(&signed.payload).into());
        Ok(VerifiedCommand {
            authorized: KernelCommand::new(
                envelope.command,
                envelope.actor,
                envelope.operational_key,
                envelope.idempotency_key,
            ),
            target: expected_scope.relationship,
            digest,
            canonical_bytes: signed.payload.clone(),
        })
    }

    /// Verifies an endpoint content-key certificate at provider receipt time.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid scope, lifetime, key authority, or signature.
    pub fn verify_content_key_certificate(
        &self,
        signed: &SignedContentKeyCertificate,
        receipt_time: CanonicalTime,
        expected_protocol_version: ProtocolVersion,
        expected_scope: SigningScope,
    ) -> Result<cs_mail_content::ContentCertificateDigest, SecurityError> {
        let certificate = &signed.certificate;
        if certificate.wire_version != WireVersion(1) {
            return Err(SecurityError::WireVersionMismatch);
        }
        if certificate.protocol_version != expected_protocol_version {
            return Err(SecurityError::ProtocolVersionMismatch);
        }
        if certificate.deployment_domain != expected_scope.deployment_domain
            || certificate.intended_provider != expected_scope.intended_provider
            || certificate.relationship != expected_scope.relationship
            || receipt_time < certificate.valid_from
            || receipt_time > certificate.valid_until
        {
            return Err(SecurityError::SigningScopeMismatch);
        }
        let key = self.active_verifying_key(
            certificate.operational_key,
            ActorRef::Sender(certificate.owner),
            receipt_time,
        )?;
        VerifyingKey::from_bytes(&key)
            .map_err(|_| SecurityError::InvalidPublicKey)?
            .verify_strict(
                &content_key_certificate_bytes(certificate),
                &Signature::from_bytes(&signed.signature),
            )
            .map_err(|_| SecurityError::InvalidSignature)?;
        Ok(content_key_certificate_digest(certificate))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedContactTerms {
    pub terms: RequestTerms,
    pub provider_operational_key: OperationalKeyRef,
    pub signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReceiptKind {
    Refused,
    NoChange,
    ContactTermsIssued,
    ReservationCommitted,
    AdmissionCommitted,
    RelationshipDecisionCommitted,
    SettlementCommitted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReceiptPayload {
    pub deployment_domain: [u8; 32],
    pub receipt_id: ReceiptRef,
    pub kind: ReceiptKind,
    pub relationship: RelationshipRef,
    pub command_digest: CommandDigest,
    pub journal_position: JournalPosition,
    pub received_at: CanonicalTime,
    pub outcome_digest: OutcomeDigest,
    pub provider: ProviderRef,
    pub protocol_version: ProtocolVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedReceipt {
    pub payload: ReceiptPayload,
    pub provider_operational_key: OperationalKeyRef,
    pub signature: [u8; 64],
}

pub struct ProviderSigner {
    provider: ProviderRef,
    reference: OperationalKeyRef,
    signing_key: SigningKey,
}

impl ProviderSigner {
    pub fn from_secret_bytes(
        provider: ProviderRef,
        reference: OperationalKeyRef,
        secret: &[u8; 32],
    ) -> Self {
        Self {
            provider,
            reference,
            signing_key: SigningKey::from_bytes(secret),
        }
    }

    pub const fn provider(&self) -> ProviderRef {
        self.provider
    }

    pub const fn reference(&self) -> OperationalKeyRef {
        self.reference
    }

    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Signs the complete fixed quote issued by this provider.
    ///
    /// # Errors
    ///
    /// Returns an error when the terms name another provider or cannot be encoded.
    pub fn sign_contact_terms(
        &self,
        terms: RequestTerms,
    ) -> Result<SignedContactTerms, SecurityError> {
        if terms.recipient_provider != self.provider {
            return Err(SecurityError::SigningScopeMismatch);
        }
        let bytes = encode_contact_terms_artifact(&terms)?;
        Ok(SignedContactTerms {
            terms,
            provider_operational_key: self.reference,
            signature: self.signing_key.sign(&bytes).to_bytes(),
        })
    }

    /// Signs a committed protocol outcome receipt.
    ///
    /// # Errors
    ///
    /// Returns an error when the receipt names another provider.
    pub fn sign_receipt(&self, payload: ReceiptPayload) -> Result<SignedReceipt, SecurityError> {
        if payload.provider != self.provider {
            return Err(SecurityError::SigningScopeMismatch);
        }
        let bytes = receipt_signing_bytes(&payload);
        Ok(SignedReceipt {
            payload,
            provider_operational_key: self.reference,
            signature: self.signing_key.sign(&bytes).to_bytes(),
        })
    }
}

impl SignedContactTerms {
    /// Verifies the provider signature over the complete immutable quote.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid public key, encoding, or signature.
    pub fn verify(&self, provider_key: &[u8; 32]) -> Result<(), SecurityError> {
        let key =
            VerifyingKey::from_bytes(provider_key).map_err(|_| SecurityError::InvalidPublicKey)?;
        key.verify_strict(
            &encode_contact_terms_artifact(&self.terms)?,
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| SecurityError::InvalidSignature)
    }
}

impl SignedReceipt {
    /// Verifies the provider signature over the receipt payload.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid public key or signature.
    pub fn verify(&self, provider_key: &[u8; 32]) -> Result<(), SecurityError> {
        let key =
            VerifyingKey::from_bytes(provider_key).map_err(|_| SecurityError::InvalidPublicKey)?;
        key.verify_strict(
            &receipt_signing_bytes(&self.payload),
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| SecurityError::InvalidSignature)
    }
}

fn receipt_signing_bytes(payload: &ReceiptPayload) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(192);
    bytes.extend_from_slice(b"cs-mail/receipt/v2");
    bytes.extend_from_slice(&payload.deployment_domain);
    bytes.extend_from_slice(&payload.receipt_id.0.to_be_bytes());
    bytes.push(match payload.kind {
        ReceiptKind::Refused => 5,
        ReceiptKind::NoChange => 6,
        ReceiptKind::ContactTermsIssued => 0,
        ReceiptKind::ReservationCommitted => 1,
        ReceiptKind::AdmissionCommitted => 2,
        ReceiptKind::RelationshipDecisionCommitted => 3,
        ReceiptKind::SettlementCommitted => 4,
    });
    bytes.extend_from_slice(&payload.relationship.derivation_version().to_be_bytes());
    bytes.extend_from_slice(payload.relationship.as_bytes());
    bytes.extend_from_slice(&payload.command_digest.0);
    bytes.extend_from_slice(&payload.journal_position.0.to_be_bytes());
    bytes.extend_from_slice(&payload.received_at.0.to_be_bytes());
    bytes.extend_from_slice(&payload.outcome_digest.0);
    bytes.extend_from_slice(&payload.provider.0.to_be_bytes());
    bytes.extend_from_slice(&payload.protocol_version.0.to_be_bytes());
    bytes
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryPolicy {
    pub actor: ActorRef,
    pub threshold: usize,
    pub factors: std::collections::BTreeSet<RecoveryFactorRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryAttempt {
    pub reference: RecoveryAttemptRef,
    pub policy: RecoveryPolicy,
    approvals: std::collections::BTreeSet<RecoveryFactorRef>,
    consumed: bool,
}

impl RecoveryAttempt {
    /// Creates an explicit threshold recovery ceremony.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero, impossible, or empty threshold policy.
    pub fn new(
        reference: RecoveryAttemptRef,
        policy: RecoveryPolicy,
    ) -> Result<Self, SecurityError> {
        if policy.threshold == 0
            || policy.factors.is_empty()
            || policy.threshold > policy.factors.len()
        {
            return Err(SecurityError::InvalidRecoveryPolicy);
        }
        Ok(Self {
            reference,
            policy,
            approvals: std::collections::BTreeSet::new(),
            consumed: false,
        })
    }

    /// Adds a distinct recovery-factor approval.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown factor or consumed ceremony.
    pub fn approve(&mut self, factor: RecoveryFactorRef) -> Result<bool, SecurityError> {
        if self.consumed {
            return Err(SecurityError::RecoveryAlreadyConsumed);
        }
        if !self.policy.factors.contains(&factor) {
            return Err(SecurityError::UnknownRecoveryFactor);
        }
        self.approvals.insert(factor);
        Ok(self.approvals.len() >= self.policy.threshold)
    }

    /// Consumes the ceremony once its threshold is met.
    ///
    /// # Errors
    ///
    /// Returns an error if approval is incomplete or was already consumed.
    pub fn consume(&mut self) -> Result<ActorRef, SecurityError> {
        if self.consumed {
            return Err(SecurityError::RecoveryAlreadyConsumed);
        }
        if self.approvals.len() < self.policy.threshold {
            return Err(SecurityError::InvalidRecoveryPolicy);
        }
        self.consumed = true;
        Ok(self.policy.actor)
    }
}

fn next_version(version: Version) -> Result<Version, SecurityError> {
    version.checked_next().ok_or(SecurityError::VersionConflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{IdempotencyKey, ProtocolIdentity};
    use std::collections::BTreeSet;

    fn signer(reference: u128, secret: u8) -> CommandSigner {
        CommandSigner::from_secret_bytes(
            ActorRef::Recipient(ProtocolIdentity(2)),
            OperationalKeyRef(reference),
            &[secret; 32],
        )
    }

    fn scope() -> SigningScope {
        SigningScope {
            deployment_domain: [9; 32],
            intended_provider: ProviderRef(5),
            relationship: RelationshipRef::from_u128_for_test(6),
        }
    }

    #[test]
    fn signed_command_verifies_and_mutation_fails() {
        let command_signer = signer(1, 7);
        let mut registry = KeyRegistry::default();
        registry
            .register(
                OperationalKeyRef(1),
                ActorRef::Recipient(ProtocolIdentity(2)),
                command_signer.verifying_key_bytes(),
                CanonicalTime(0),
            )
            .unwrap();
        let signed_command = command_signer
            .sign(
                scope(),
                ProtocolVersion(2),
                IdempotencyKey(3),
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
            )
            .unwrap();
        let verified = registry
            .verify(
                &signed_command,
                CanonicalTime(1),
                ProtocolVersion(2),
                scope(),
            )
            .unwrap();
        assert_eq!(
            verified.canonical_bytes(),
            signed_command.payload.as_slice()
        );
        assert_eq!(
            verified.authorized.actor(),
            ActorRef::Recipient(ProtocolIdentity(2))
        );
        assert_eq!(
            registry.verify(
                &signed_command,
                CanonicalTime(1),
                ProtocolVersion(2),
                SigningScope {
                    deployment_domain: [8; 32],
                    intended_provider: ProviderRef(5),
                    relationship: RelationshipRef::from_u128_for_test(6),
                },
            ),
            Err(SecurityError::SigningScopeMismatch)
        );

        let mut modified = signed_command;
        let last = modified.payload.len() - 1;
        modified.payload[last] ^= 1;
        assert_eq!(
            registry.verify(&modified, CanonicalTime(1), ProtocolVersion(2), scope()),
            Err(SecurityError::InvalidSignature)
        );
    }

    #[test]
    fn previously_supported_command_wire_version_is_rejected() {
        let command_signer = signer(1, 7);
        let mut registry = KeyRegistry::default();
        registry
            .register(
                OperationalKeyRef(1),
                ActorRef::Recipient(ProtocolIdentity(2)),
                command_signer.verifying_key_bytes(),
                CanonicalTime(0),
            )
            .unwrap();
        let payload = encode_command_envelope(&CanonicalCommandEnvelope {
            wire_version: WireVersion(4),
            protocol_version: ProtocolVersion(2),
            deployment_domain: scope().deployment_domain,
            intended_provider: scope().intended_provider,
            target: CommandTarget::Relationship(scope().relationship),
            actor: ActorRef::Recipient(ProtocolIdentity(2)),
            operational_key: OperationalKeyRef(1),
            idempotency_key: IdempotencyKey(30),
            command: ProtocolCommand::AcceptRelationship {
                expected_version: Version(0),
            },
        })
        .unwrap();
        let signed = SignedCommandBytes {
            signature: command_signer.signing_key.sign(&payload).to_bytes(),
            payload,
        };

        assert_eq!(
            registry.verify(&signed, CanonicalTime(1), ProtocolVersion(2), scope()),
            Err(SecurityError::WireVersionMismatch)
        );
    }

    #[test]
    fn rotation_and_revocation_use_canonical_receipt_time() {
        let old = signer(1, 7);
        let new = signer(2, 8);
        let mut registry = KeyRegistry::default();
        registry
            .register(
                OperationalKeyRef(1),
                ActorRef::Recipient(ProtocolIdentity(2)),
                old.verifying_key_bytes(),
                CanonicalTime(0),
            )
            .unwrap();
        registry
            .rotate(
                OperationalKeyRef(1),
                Version(0),
                OperationalKeyRef(2),
                new.verifying_key_bytes(),
                CanonicalTime(10),
            )
            .unwrap();
        let old_command = old
            .sign(
                scope(),
                ProtocolVersion(2),
                IdempotencyKey(3),
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
            )
            .unwrap();
        assert!(
            registry
                .verify(&old_command, CanonicalTime(10), ProtocolVersion(2), scope())
                .is_ok()
        );
        assert_eq!(
            registry.verify(&old_command, CanonicalTime(11), ProtocolVersion(2), scope()),
            Err(SecurityError::KeyNotValidAtReceipt)
        );
        registry
            .revoke(OperationalKeyRef(2), Version(0), CanonicalTime(20))
            .unwrap();
        let new_command = new
            .sign(
                scope(),
                ProtocolVersion(2),
                IdempotencyKey(4),
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                },
            )
            .unwrap();
        assert!(
            registry
                .verify(&new_command, CanonicalTime(19), ProtocolVersion(2), scope())
                .is_ok()
        );
        assert_eq!(
            registry.verify(&new_command, CanonicalTime(20), ProtocolVersion(2), scope()),
            Err(SecurityError::KeyNotValidAtReceipt)
        );
        assert!(registry.transparency().verify());
        assert_eq!(registry.transparency().entries().len(), 4);
    }

    #[test]
    fn recovery_requires_distinct_threshold_approvals_and_is_one_shot() {
        let actor = ActorRef::Recipient(ProtocolIdentity(2));
        let mut recovery = RecoveryAttempt::new(
            RecoveryAttemptRef(1),
            RecoveryPolicy {
                actor,
                threshold: 2,
                factors: BTreeSet::from([RecoveryFactorRef(1), RecoveryFactorRef(2)]),
            },
        )
        .unwrap();
        assert!(!recovery.approve(RecoveryFactorRef(1)).unwrap());
        assert!(!recovery.approve(RecoveryFactorRef(1)).unwrap());
        assert!(recovery.approve(RecoveryFactorRef(2)).unwrap());
        assert_eq!(recovery.consume().unwrap(), actor);
        assert_eq!(
            recovery.consume(),
            Err(SecurityError::RecoveryAlreadyConsumed)
        );
    }
}
