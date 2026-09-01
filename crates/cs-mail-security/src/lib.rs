//! Operational-key signing and verification for cs-mail commands.

use std::collections::BTreeMap;
use std::fmt;

use cs_mail_primitives::{
    CanonicalTime, OperationalKeyRef, ProtocolVersion, ProviderRef, RecoveryAttemptRef,
    RecoveryFactorRef, Version,
};
use cs_mail_protocol::{ActorRef, Authorized, ProtocolCommand};
use cs_mail_wire::{
    CanonicalCommandEnvelope, WireError, decode_command_envelope, encode_command_envelope,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SigningScope {
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedCommandBytes {
    pub payload: Vec<u8>,
    pub signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyStatus {
    Active,
    Retired { valid_until: CanonicalTime },
    Revoked { revoked_at: CanonicalTime },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalKeyRecord {
    pub reference: OperationalKeyRef,
    pub actor: ActorRef,
    pub verifying_key: [u8; 32],
    pub valid_from: CanonicalTime,
    pub status: KeyStatus,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransparencyAction {
    Registered,
    Retired,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCommand {
    pub authorized: Authorized<ProtocolCommand>,
    pub digest: CommandDigest,
    pub canonical_bytes: Vec<u8>,
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
            protocol_version,
            deployment_domain: scope.deployment_domain,
            intended_provider: scope.intended_provider,
            actor: self.actor,
            operational_key: self.reference,
            idempotency_key,
            command,
        })?;
        let signature = self.signing_key.sign(&payload).to_bytes();
        Ok(SignedCommandBytes { payload, signature })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyRegistry {
    keys: BTreeMap<OperationalKeyRef, OperationalKeyRecord>,
    version: Version,
    transparency: KeyTransparencyLog,
}

impl KeyRegistry {
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
        if envelope.protocol_version != expected_protocol_version {
            return Err(SecurityError::ProtocolVersionMismatch);
        }
        if envelope.deployment_domain != expected_scope.deployment_domain
            || envelope.intended_provider != expected_scope.intended_provider
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
            authorized: Authorized::assume_verified(
                envelope.command,
                envelope.actor,
                envelope.operational_key,
                envelope.idempotency_key,
            ),
            digest,
            canonical_bytes: signed.payload.clone(),
        })
    }
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
                ProtocolVersion(1),
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
                ProtocolVersion(1),
                scope(),
            )
            .unwrap();
        assert_eq!(
            verified.digest.0,
            [
                181, 130, 147, 228, 148, 162, 207, 9, 74, 183, 218, 42, 186, 67, 61, 72, 45, 103,
                38, 228, 123, 184, 100, 186, 163, 81, 250, 88, 215, 25, 46, 112,
            ]
        );
        assert_eq!(
            signed_command.signature,
            [
                1, 86, 100, 123, 8, 120, 143, 30, 221, 88, 55, 9, 175, 93, 164, 92, 19, 190, 143,
                177, 77, 190, 145, 73, 157, 211, 232, 119, 175, 150, 68, 112, 136, 126, 156, 72,
                84, 76, 80, 101, 109, 201, 140, 114, 255, 63, 117, 254, 221, 206, 213, 2, 75, 67,
                129, 130, 171, 174, 75, 53, 27, 6, 198, 12,
            ]
        );
        assert_eq!(
            verified.authorized.actor(),
            ActorRef::Recipient(ProtocolIdentity(2))
        );
        assert_eq!(
            registry.verify(
                &signed_command,
                CanonicalTime(1),
                ProtocolVersion(1),
                SigningScope {
                    deployment_domain: [8; 32],
                    intended_provider: ProviderRef(5),
                },
            ),
            Err(SecurityError::SigningScopeMismatch)
        );

        let mut modified = signed_command;
        let last = modified.payload.len() - 1;
        modified.payload[last] ^= 1;
        assert_eq!(
            registry.verify(&modified, CanonicalTime(1), ProtocolVersion(1), scope()),
            Err(SecurityError::InvalidSignature)
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
                ProtocolVersion(1),
                IdempotencyKey(3),
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
            )
            .unwrap();
        assert!(
            registry
                .verify(&old_command, CanonicalTime(10), ProtocolVersion(1), scope())
                .is_ok()
        );
        assert_eq!(
            registry.verify(&old_command, CanonicalTime(11), ProtocolVersion(1), scope()),
            Err(SecurityError::KeyNotValidAtReceipt)
        );
        registry
            .revoke(OperationalKeyRef(2), Version(0), CanonicalTime(20))
            .unwrap();
        let new_command = new
            .sign(
                scope(),
                ProtocolVersion(1),
                IdempotencyKey(4),
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                },
            )
            .unwrap();
        assert!(
            registry
                .verify(&new_command, CanonicalTime(19), ProtocolVersion(1), scope())
                .is_ok()
        );
        assert_eq!(
            registry.verify(&new_command, CanonicalTime(20), ProtocolVersion(1), scope()),
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
