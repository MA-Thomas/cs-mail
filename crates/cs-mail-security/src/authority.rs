//! Receipt-time composition of independently owned authority registries.
use super::{
    ActorRef, CanonicalTime, KeyRegistry, OperationalKeyRef, ProtocolVersion, SecurityError,
    SignedCommandBytes, SignedContentKeyCertificate, SigningScope, VerifiedCommand,
    decode_command_envelope,
};
use serde::{Deserialize, Serialize};

use cs_mail_primitives::{AccountId, ProtocolIdentity};
use std::collections::{BTreeMap, BTreeSet};

/// Private ownership maps are validated on construction and deserialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SnapshotData")]
pub struct AuthoritySnapshot {
    relationship: KeyRegistry,
    accounts: BTreeMap<AccountId, AccountAuthority>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct AccountAuthority {
    personas: BTreeSet<ProtocolIdentity>,
    registry: KeyRegistry,
}
#[derive(Deserialize)]
struct SnapshotData {
    relationship: KeyRegistry,
    accounts: BTreeMap<AccountId, AccountAuthority>,
}
impl TryFrom<SnapshotData> for AuthoritySnapshot {
    type Error = SecurityError;
    fn try_from(value: SnapshotData) -> Result<Self, Self::Error> {
        let mut snapshot = Self::new(value.relationship)?;
        for (id, account) in value.accounts {
            snapshot.add_account(id, account.personas, account.registry)?;
        }
        Ok(snapshot)
    }
}
impl AuthoritySnapshot {
    /// # Errors
    /// Relationship authority may contain only provider and scheduler keys.
    pub fn new(relationship: KeyRegistry) -> Result<Self, SecurityError> {
        if relationship
            .records()
            .any(|r| matches!(r.actor, ActorRef::Sender(_) | ActorRef::Recipient(_)))
        {
            return Err(SecurityError::ActorMismatch);
        }
        Ok(Self {
            relationship,
            accounts: BTreeMap::new(),
        })
    }
    /// # Errors
    /// Rejects duplicate account/persona/key ownership and keys belonging to foreign personas.
    pub fn add_account(
        &mut self,
        id: AccountId,
        personas: BTreeSet<ProtocolIdentity>,
        registry: KeyRegistry,
    ) -> Result<(), SecurityError> {
        if id.0 == 0
            || personas.is_empty()
            || self.accounts.contains_key(&id)
            || self
                .accounts
                .values()
                .any(|a| !a.personas.is_disjoint(&personas))
        {
            return Err(SecurityError::ActorMismatch);
        }
        for r in registry.records() {
            if !matches!(r.actor, ActorRef::Sender(p) | ActorRef::Recipient(p) if personas.contains(&p))
            {
                return Err(SecurityError::ActorMismatch);
            }
            if self.relationship.contains_key(r.reference)
                || self
                    .accounts
                    .values()
                    .any(|a| a.registry.contains_key(r.reference))
            {
                return Err(SecurityError::DuplicateKey);
            }
        }
        self.accounts
            .insert(id, AccountAuthority { personas, registry });
        Ok(())
    }
    fn registry(&self, actor: ActorRef) -> Result<&KeyRegistry, SecurityError> {
        match actor {
            ActorRef::Sender(id) | ActorRef::Recipient(id) => self
                .accounts
                .values()
                .find(|a| a.personas.contains(&id))
                .map(|a| &a.registry)
                .ok_or(SecurityError::UnknownKey),
            ActorRef::Provider(_) | ActorRef::Scheduler(_) => Ok(&self.relationship),
        }
    }
    /// # Errors
    /// Rejects stale relationship copies for user personas, revoked or unknown keys.
    pub fn active_actor(
        &self,
        reference: OperationalKeyRef,
        at: CanonicalTime,
    ) -> Result<(ActorRef, [u8; 32]), SecurityError> {
        for account in self.accounts.values() {
            if account.registry.contains_key(reference) {
                let (actor, _) = account.registry.active_actor(reference, at)?;
                return Ok((actor, self.active_verifying_key(reference, actor, at)?));
            }
        }
        let (actor, _) = self.relationship.active_actor(reference, at)?;
        Ok((
            actor,
            self.registry(actor)?
                .active_verifying_key(reference, actor, at)?,
        ))
    }
    /// # Errors
    /// Rejects invalid actor/key authority at receipt time.
    pub fn active_verifying_key(
        &self,
        reference: OperationalKeyRef,
        actor: ActorRef,
        at: CanonicalTime,
    ) -> Result<[u8; 32], SecurityError> {
        self.registry(actor)?
            .active_verifying_key(reference, actor, at)
    }
    /// # Errors
    /// Rejects invalid signatures, command context, or current authority.
    pub fn verify(
        &self,
        signed: &SignedCommandBytes,
        at: CanonicalTime,
        version: ProtocolVersion,
        scope: SigningScope,
    ) -> Result<VerifiedCommand, SecurityError> {
        let envelope = decode_command_envelope(&signed.payload)?;
        self.registry(envelope.actor)?
            .verify(signed, at, version, scope)
    }
    /// # Errors
    /// Rejects invalid content-key certificate authority.
    pub fn verify_content_key_certificate(
        &self,
        signed: &SignedContentKeyCertificate,
        at: CanonicalTime,
        version: ProtocolVersion,
        scope: SigningScope,
    ) -> Result<cs_mail_content::ContentCertificateDigest, SecurityError> {
        self.registry(ActorRef::Sender(signed.certificate.owner))?
            .verify_content_key_certificate(signed, at, version, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{ProviderRef, Version};
    use ed25519_dalek::SigningKey;
    #[test]
    fn ownership_is_consistent_and_revocation_is_authoritative() {
        let persona = ProtocolIdentity(10);
        let actor = ActorRef::Sender(persona);
        let reference = OperationalKeyRef(1);
        let key = SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes();
        let mut registry = KeyRegistry::default();
        registry
            .register(reference, actor, key, CanonicalTime(0))
            .unwrap();
        assert!(AuthoritySnapshot::new(registry.clone()).is_err());
        let mut snapshot = AuthoritySnapshot::new(KeyRegistry::default()).unwrap();
        assert!(
            snapshot
                .add_account(
                    AccountId(1),
                    [ProtocolIdentity(11)].into(),
                    registry.clone()
                )
                .is_err()
        );
        snapshot
            .add_account(AccountId(1), [persona].into(), registry.clone())
            .unwrap();
        assert!(
            snapshot
                .add_account(AccountId(2), [persona].into(), registry.clone())
                .is_err()
        );
        assert_eq!(
            snapshot.active_actor(reference, CanonicalTime(1)).unwrap(),
            (actor, key)
        );
        assert_eq!(
            snapshot
                .active_verifying_key(reference, actor, CanonicalTime(1))
                .unwrap(),
            key
        );
        registry
            .revoke(reference, Version(0), CanonicalTime(1))
            .unwrap();
        let mut revoked = AuthoritySnapshot::new(KeyRegistry::default()).unwrap();
        revoked
            .add_account(AccountId(1), [persona].into(), registry)
            .unwrap();
        assert!(revoked.active_actor(reference, CanonicalTime(2)).is_err());
        assert!(
            revoked
                .active_verifying_key(reference, actor, CanonicalTime(2))
                .is_err()
        );
        let mut provider = KeyRegistry::default();
        provider
            .register(
                OperationalKeyRef(2),
                ActorRef::Provider(ProviderRef(30)),
                key,
                CanonicalTime(0),
            )
            .unwrap();
        assert!(
            AuthoritySnapshot::new(provider)
                .unwrap()
                .active_actor(OperationalKeyRef(2), CanonicalTime(1))
                .is_ok()
        );
    }
}
