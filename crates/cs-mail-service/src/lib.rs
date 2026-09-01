//! A small authenticated ingress facade around the durable protocol engine.

use core::fmt;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use cs_mail_adapters::{DmarcError, DmarcVerifier, LegacyDmarcEvidence, SmtpAuthenticationRequest};
use cs_mail_capabilities::{
    AdmissionAuthentication, BondFreeAdmission, LaneEvidence, LaneSubject, LegacyBondFreeAdmission,
    SignedBondFreeAdmission, SignedLaneControl, SignedLaneGrant,
};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_primitives::{
    CanonicalTime, Duration, IdempotencyKey, OperationalKeyRef, ProtocolVersion, Version,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot, SettlementSnapshot};
use cs_mail_security::{KeyRegistry, SecurityError, SignedCommandBytes};
use cs_mail_storage_postgres::{
    BondFreeAdmissionOutcome, DurableExecutionOutcome, LaneOperationOutcome, PostgresEngine,
    StorageError,
};

pub trait CanonicalClock: Send + Sync {
    fn now(&self) -> CanonicalTime;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnixMillisecondClock;

impl CanonicalClock for UnixMillisecondClock {
    fn now(&self) -> CanonicalTime {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        CanonicalTime(u64::try_from(millis).unwrap_or(u64::MAX))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePolicy {
    pub protocol: PolicySnapshot,
    pub deployment_domain: [u8; 32],
    pub max_content_lifetime: Duration,
    pub legacy_identity_mapping_version: u16,
}

#[derive(Debug)]
pub enum ServiceError {
    Security(SecurityError),
    Storage(StorageError),
    RegistryLock,
    ContentTimeInvalid,
    SigningScopeMismatch,
    Dmarc(DmarcError),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Security(error) => write!(formatter, "authentication failed: {error}"),
            Self::Storage(error) => write!(formatter, "durable operation failed: {error}"),
            Self::RegistryLock => formatter.write_str("key registry lock poisoned"),
            Self::ContentTimeInvalid => formatter.write_str("content lifetime is outside policy"),
            Self::SigningScopeMismatch => {
                formatter.write_str("capability command signing scope mismatch")
            }
            Self::Dmarc(error) => write!(formatter, "DMARC verification failed: {error}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<SecurityError> for ServiceError {
    fn from(value: SecurityError) -> Self {
        Self::Security(value)
    }
}

impl From<StorageError> for ServiceError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct IngressService<C> {
    engine: PostgresEngine,
    registry: RwLock<KeyRegistry>,
    clock: C,
    policy: ServicePolicy,
}

impl<C: CanonicalClock> IngressService<C> {
    pub fn new(
        engine: PostgresEngine,
        registry: KeyRegistry,
        clock: C,
        policy: ServicePolicy,
    ) -> Self {
        Self {
            engine,
            registry: RwLock::new(registry),
            clock,
            policy,
        }
    }

    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.policy.protocol.protocol_version
    }

    /// Verifies at provider receipt time and atomically executes a signed command.
    ///
    /// # Errors
    ///
    /// Returns an authentication, registry, protocol, ledger, or storage error.
    pub fn submit(
        &self,
        command: &SignedCommandBytes,
    ) -> Result<DurableExecutionOutcome, ServiceError> {
        let now = self.clock.now();
        let registry = self
            .registry
            .read()
            .map_err(|_| ServiceError::RegistryLock)?;
        Ok(self.engine.execute_signed(
            &registry,
            command,
            self.policy.deployment_domain,
            now,
            self.policy.protocol.clone(),
        )?)
    }

    /// Verifies a recipient-signed grant and stores it under the relationship lock.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid recipient key, signature, scope, block,
    /// replay conflict, or storage failure.
    pub fn grant_lane(
        &self,
        signed: &SignedLaneGrant,
        idempotency_key: IdempotencyKey,
    ) -> Result<LaneOperationOutcome, ServiceError> {
        let now = self.clock.now();
        if signed.grant.protocol_version != self.policy.protocol.protocol_version
            || signed.grant.deployment_domain != self.policy.deployment_domain
            || signed.grant.intended_provider != self.policy.protocol.recipient_provider
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        if let LaneSubject::LegacyDomain(domain) = &signed.grant.subject
            && domain.synthetic_protocol_identity(
                &self.policy.deployment_domain,
                self.policy.legacy_identity_mapping_version,
            ) != signed.grant.sender
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let registry = self
            .registry
            .read()
            .map_err(|_| ServiceError::RegistryLock)?;
        let key = registry.active_verifying_key(
            signed.grant.recipient_operational_key,
            ActorRef::Recipient(signed.grant.recipient),
            now,
        )?;
        Ok(self.engine.grant_lane(signed, &key, idempotency_key, now)?)
    }

    /// Verifies and applies an explicit recipient revocation or reconfirmation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid signature/scope, stale version, block,
    /// conflicting replay, or storage failure.
    pub fn control_lane(
        &self,
        signed: &SignedLaneControl,
    ) -> Result<LaneOperationOutcome, ServiceError> {
        let now = self.clock.now();
        let control = &signed.control;
        if control.protocol_version != self.policy.protocol.protocol_version
            || control.deployment_domain != self.policy.deployment_domain
            || control.intended_provider != self.policy.protocol.recipient_provider
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let registry = self
            .registry
            .read()
            .map_err(|_| ServiceError::RegistryLock)?;
        let key = registry.active_verifying_key(
            control.recipient_operational_key,
            ActorRef::Recipient(control.recipient),
            now,
        )?;
        signed
            .verify(&key)
            .map_err(StorageError::from)
            .map_err(ServiceError::from)?;
        Ok(self.engine.control_lane(control, now)?)
    }

    /// Authenticates and durably admits one accepted-relationship or lane message.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid sender signature, wrong provider scope,
    /// blocked contact, missing authority, rate exhaustion, or storage failure.
    pub fn admit_native_bond_free(
        &self,
        signed: &SignedBondFreeAdmission,
    ) -> Result<BondFreeAdmissionOutcome, ServiceError> {
        let now = self.clock.now();
        let admission = &signed.admission;
        if admission.protocol_version != self.policy.protocol.protocol_version
            || admission.deployment_domain != self.policy.deployment_domain
            || admission.intended_provider != self.policy.protocol.recipient_provider
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let AdmissionAuthentication::NativeKey(operational_key) = admission.authentication else {
            return Err(ServiceError::SigningScopeMismatch);
        };
        if matches!(admission.evidence, Some(LaneEvidence::Legacy(_))) {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let registry = self
            .registry
            .read()
            .map_err(|_| ServiceError::RegistryLock)?;
        let key = registry.active_verifying_key(
            operational_key,
            ActorRef::Sender(admission.sender),
            now,
        )?;
        signed
            .verify(&key)
            .map_err(StorageError::from)
            .map_err(ServiceError::from)?;
        Ok(self.engine.admit_bond_free(admission, now)?)
    }

    /// Runs DMARC at the trusted edge and admits a legacy message using the
    /// resulting evidence rather than a sender assertion.
    ///
    /// # Errors
    ///
    /// Returns an error for DMARC failure, identity/scope mismatch, blocked
    /// contact, missing authority, rate exhaustion, or storage failure.
    pub async fn admit_legacy_bond_free<V: DmarcVerifier>(
        &self,
        verifier: &V,
        authentication: SmtpAuthenticationRequest<'_>,
        request: &LegacyBondFreeAdmission,
    ) -> Result<BondFreeAdmissionOutcome, ServiceError> {
        let now = self.clock.now();
        if request.protocol_version != self.policy.protocol.protocol_version
            || request.deployment_domain != self.policy.deployment_domain
            || request.intended_provider != self.policy.protocol.recipient_provider
            || authentication.received_at != now
            || request.lane_domain.synthetic_protocol_identity(
                &self.policy.deployment_domain,
                self.policy.legacy_identity_mapping_version,
            ) != request.sender
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let evidence = verifier
            .verify(authentication)
            .await
            .map_err(ServiceError::Dmarc)?;
        if evidence.evaluated_at != now {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let evidence = LegacyDmarcEvidence::new(request.lane_domain.clone(), evidence)
            .map_err(|_| ServiceError::SigningScopeMismatch)?;
        let admission = BondFreeAdmission {
            sender: request.sender,
            recipient: request.recipient,
            message_id: request.message_id,
            content_ref: request.content_ref,
            delivery_intent_ref: request.delivery_intent_ref,
            evidence: Some(LaneEvidence::Legacy(evidence)),
            idempotency_key: request.idempotency_key,
            protocol_version: request.protocol_version,
            deployment_domain: request.deployment_domain,
            intended_provider: request.intended_provider,
            authentication: AdmissionAuthentication::LegacyDmarc,
        };
        Ok(self.engine.admit_bond_free(&admission, now)?)
    }

    /// Stores ciphertext only when its client-declared lifetime is bounded by service policy.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid time bounds or storage failure.
    pub fn upload_content(&self, record: &EncryptedContentRecord) -> Result<(), ServiceError> {
        let now = self.clock.now();
        let latest = now
            .checked_add(self.policy.max_content_lifetime)
            .ok_or(ServiceError::ContentTimeInvalid)?;
        if record.created_at > now || record.expires_at <= now || record.expires_at > latest {
            return Err(ServiceError::ContentTimeInvalid);
        }
        self.engine.store_content(record)?;
        Ok(())
    }

    /// Registers an actor-scoped operational key through the administrative boundary.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid keys or a poisoned registry lock.
    pub fn register_operational_key(
        &self,
        reference: OperationalKeyRef,
        actor: ActorRef,
        verifying_key: [u8; 32],
    ) -> Result<(), ServiceError> {
        let now = self.clock.now();
        self.registry
            .write()
            .map_err(|_| ServiceError::RegistryLock)?
            .register(reference, actor, verifying_key, now)?;
        Ok(())
    }

    /// Revokes an operational key at trusted receipt time.
    ///
    /// # Errors
    ///
    /// Returns an error for stale versions, missing keys, or a poisoned registry lock.
    pub fn revoke_operational_key(
        &self,
        reference: OperationalKeyRef,
        expected_version: Version,
    ) -> Result<(), ServiceError> {
        let now = self.clock.now();
        self.registry
            .write()
            .map_err(|_| ServiceError::RegistryLock)?
            .revoke(reference, expected_version, now)?;
        Ok(())
    }

    /// Returns the current protocol and ledger projection.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the snapshot cannot be read.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, ServiceError> {
        Ok(self.engine.snapshot()?)
    }

    pub const fn engine(&self) -> &PostgresEngine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct FixedClock(CanonicalTime);

    impl CanonicalClock for FixedClock {
        fn now(&self) -> CanonicalTime {
            self.0
        }
    }

    #[test]
    fn fixed_clock_is_suitable_for_canonical_boundary_tests() {
        assert_eq!(FixedClock(CanonicalTime(7)).now(), CanonicalTime(7));
    }
}
