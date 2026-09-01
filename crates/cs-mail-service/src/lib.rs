//! A small authenticated ingress facade around the durable protocol engine.

use core::fmt;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cs_mail_adapters::{DmarcError, DmarcVerifier, LegacyDmarcEvidence, SmtpAuthenticationRequest};
use cs_mail_capabilities::{
    AdmissionAuthentication, BondFreeAdmission, LaneEvidence, LaneSubject, LegacyBondFreeAdmission,
    SignedBondFreeAdmission, SignedLaneControl, SignedLaneGrant,
};
use cs_mail_content::{EncryptedContentRecord, content_key_certificate_digest};
use cs_mail_primitives::{
    CanonicalTime, DeclarationAuthority, Duration, ExtensionCriticality, IdempotencyKey,
    MessageDeclarations, MessageValidityUntil, NamespacedIdentifier, OperationalKeyRef,
    OriginDeclaration, OriginMode, ProtocolVersion, ReceiptRef, Version, WireVersion,
};
use cs_mail_protocol::{
    ActorRef, PolicySnapshot, ProtocolEventKind, SettlementSnapshot, TermsOutcome,
};
use cs_mail_security::{
    CommandDigest, KeyRegistry, OutcomeDigest, ProviderSigner, ReceiptKind, ReceiptPayload,
    SecurityError, SignedCommandBytes, SignedContactTerms, SignedContentKeyCertificate,
    SignedReceipt, SigningScope,
};
use cs_mail_storage_postgres::{
    BondFreeAdmissionOutcome, DurableExecutionOutcome, LaneOperationOutcome, PostgresEngine,
    StorageError,
};
use sha2::{Digest as _, Sha256};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionPolicyReason {
    UnsupportedCriticalExtension,
}

pub struct AdmissionPolicyInput<'a> {
    pub declarations: &'a MessageDeclarations,
    pub message_valid_until: MessageValidityUntil,
    pub capability: Option<cs_mail_primitives::LaneId>,
    pub authentication: AdmissionAuthentication,
}

pub trait AdmissionPolicy: Send + Sync {
    /// Decides whether envelope-visible declarations are acceptable for admission.
    ///
    /// # Errors
    ///
    /// Returns a refusal reason without mutating protocol or ledger state.
    fn evaluate(&self, input: &AdmissionPolicyInput<'_>) -> Result<(), AdmissionPolicyReason>;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultAdmissionPolicy {
    supported_critical_schemas: BTreeSet<NamespacedIdentifier>,
}

impl DefaultAdmissionPolicy {
    pub fn with_supported_critical_schemas(
        supported_critical_schemas: impl IntoIterator<Item = NamespacedIdentifier>,
    ) -> Self {
        Self {
            supported_critical_schemas: supported_critical_schemas.into_iter().collect(),
        }
    }
}

impl AdmissionPolicy for DefaultAdmissionPolicy {
    fn evaluate(&self, input: &AdmissionPolicyInput<'_>) -> Result<(), AdmissionPolicyReason> {
        if let Some(schema) = &input.declarations.payload_schema
            && schema.criticality == ExtensionCriticality::Critical
            && !self.supported_critical_schemas.contains(&schema.id)
        {
            return Err(AdmissionPolicyReason::UnsupportedCriticalExtension);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ServiceError {
    Security(SecurityError),
    Storage(StorageError),
    ContentTimeInvalid,
    SigningScopeMismatch,
    ReceiptUnavailable,
    Dmarc(DmarcError),
    MessageValidityClosed,
    AdmissionPolicyRefused(AdmissionPolicyReason),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Security(error) => write!(formatter, "authentication failed: {error}"),
            Self::Storage(error) => write!(formatter, "durable operation failed: {error}"),
            Self::ContentTimeInvalid => formatter.write_str("content lifetime is outside policy"),
            Self::SigningScopeMismatch => {
                formatter.write_str("capability command signing scope mismatch")
            }
            Self::Dmarc(error) => write!(formatter, "DMARC verification failed: {error}"),
            Self::MessageValidityClosed => {
                formatter.write_str("message validity closed before admission")
            }
            Self::AdmissionPolicyRefused(reason) => {
                write!(
                    formatter,
                    "admission policy refused the envelope: {reason:?}"
                )
            }
            Self::ReceiptUnavailable => {
                formatter.write_str("committed operation has no journal evidence")
            }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceExecutionOutcome {
    pub execution: DurableExecutionOutcome,
    pub signed_terms: Option<SignedContactTerms>,
    pub receipt: SignedReceipt,
}

pub struct IngressService<C> {
    engine: PostgresEngine,
    provider_signer: ProviderSigner,
    clock: C,
    policy: ServicePolicy,
    admission_policy: Arc<dyn AdmissionPolicy>,
}

impl<C: CanonicalClock> IngressService<C> {
    /// Creates an ingress boundary backed by durable key authority.
    ///
    /// # Errors
    ///
    /// Returns an error for provider/key mismatch or registry persistence failure.
    pub fn new(
        engine: PostgresEngine,
        registry: KeyRegistry,
        provider_signer: ProviderSigner,
        clock: C,
        policy: ServicePolicy,
    ) -> Result<Self, ServiceError> {
        Self::new_with_admission_policy(
            engine,
            registry,
            provider_signer,
            clock,
            policy,
            Arc::new(DefaultAdmissionPolicy::default()),
        )
    }

    /// Creates ingress with an explicitly injected provider admission policy.
    ///
    /// # Errors
    ///
    /// Returns an error for provider/key mismatch or registry persistence failure.
    pub fn new_with_admission_policy(
        engine: PostgresEngine,
        mut registry: KeyRegistry,
        provider_signer: ProviderSigner,
        clock: C,
        policy: ServicePolicy,
        admission_policy: Arc<dyn AdmissionPolicy>,
    ) -> Result<Self, ServiceError> {
        if provider_signer.provider() != policy.protocol.recipient_provider {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let now = clock.now();
        match registry.active_verifying_key(
            provider_signer.reference(),
            ActorRef::Provider(provider_signer.provider()),
            now,
        ) {
            Ok(key) if key == provider_signer.verifying_key_bytes() => {}
            Ok(_) => return Err(ServiceError::SigningScopeMismatch),
            Err(SecurityError::UnknownKey) => registry.register(
                provider_signer.reference(),
                ActorRef::Provider(provider_signer.provider()),
                provider_signer.verifying_key_bytes(),
                now,
            )?,
            Err(error) => return Err(ServiceError::Security(error)),
        }
        engine.initialize_key_registry(&registry, now)?;
        let durable_registry = engine.key_registry()?;
        match durable_registry.active_verifying_key(
            provider_signer.reference(),
            ActorRef::Provider(provider_signer.provider()),
            now,
        ) {
            Ok(key) if key == provider_signer.verifying_key_bytes() => {}
            Ok(_) => return Err(ServiceError::SigningScopeMismatch),
            Err(SecurityError::UnknownKey) => engine.register_operational_key(
                provider_signer.reference(),
                ActorRef::Provider(provider_signer.provider()),
                provider_signer.verifying_key_bytes(),
                now,
            )?,
            Err(error) => return Err(ServiceError::Security(error)),
        }
        Ok(Self {
            engine,
            provider_signer,
            clock,
            policy,
            admission_policy,
        })
    }

    fn evaluate_admission_policy(
        &self,
        declarations: &MessageDeclarations,
        message_valid_until: MessageValidityUntil,
        capability: Option<cs_mail_primitives::LaneId>,
        authentication: AdmissionAuthentication,
        now: CanonicalTime,
    ) -> Result<(), ServiceError> {
        if message_valid_until.0 < now {
            return Err(ServiceError::MessageValidityClosed);
        }
        self.admission_policy
            .evaluate(&AdmissionPolicyInput {
                declarations,
                message_valid_until,
                capability,
                authentication,
            })
            .map_err(ServiceError::AdmissionPolicyRefused)
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
        Ok(self.submit_with_artifacts(command)?.execution)
    }

    /// Executes a command and returns portable provider-signed terms and receipt evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for authentication, transition, persistence, or signing failures.
    pub fn submit_with_artifacts(
        &self,
        command: &SignedCommandBytes,
    ) -> Result<ServiceExecutionOutcome, ServiceError> {
        let now = self.clock.now();
        let registry = self.engine.key_registry()?;
        let execution = self.engine.execute_signed(
            &registry,
            command,
            self.policy.deployment_domain,
            now,
            self.policy.protocol.clone(),
        )?;
        let signed_terms = match execution.manifest.terms_outcome.clone() {
            Some(TermsOutcome::BondRequired(terms)) => {
                let signed = self.provider_signer.sign_contact_terms(*terms)?;
                self.engine.attach_signed_quote(&signed)?;
                Some(signed)
            }
            Some(TermsOutcome::NoBondRequired) | None => None,
        };
        let event = execution
            .manifest
            .protocol_events
            .first()
            .ok_or(ServiceError::ReceiptUnavailable)?;
        let command_digest = CommandDigest(Sha256::digest(&command.payload).into());
        let outcome_bytes = serde_json::to_vec(&execution.manifest)
            .map_err(StorageError::from)
            .map_err(ServiceError::from)?;
        let outcome_digest = OutcomeDigest(Sha256::digest(outcome_bytes).into());
        let receipt_id = receipt_ref(command_digest, event.reference.0, outcome_digest);
        let receipt = self.provider_signer.sign_receipt(ReceiptPayload {
            receipt_id,
            kind: receipt_kind(event.kind),
            relationship: execution.manifest.next_state.relationship.key.reference,
            command_digest,
            journal_position: event.reference.0,
            received_at: event.at,
            outcome_digest,
            provider: self.policy.protocol.recipient_provider,
            protocol_version: self.policy.protocol.protocol_version,
        })?;
        self.engine.store_receipt(&receipt)?;
        Ok(ServiceExecutionOutcome {
            execution,
            signed_terms,
            receipt,
        })
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
        let registry = self.engine.key_registry()?;
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
        let registry = self.engine.key_registry()?;
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
        let registry = self.engine.key_registry()?;
        let key = registry.active_verifying_key(
            operational_key,
            ActorRef::Sender(admission.sender),
            now,
        )?;
        signed
            .verify(&key)
            .map_err(StorageError::from)
            .map_err(ServiceError::from)?;
        self.evaluate_admission_policy(
            &admission.declarations,
            admission.message_valid_until,
            admission.capability,
            admission.authentication,
            now,
        )?;
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
        let declarations = MessageDeclarations {
            purpose: request.purpose.clone(),
            origin: OriginDeclaration {
                mode: OriginMode::LegacyOrUnspecified,
                authority: DeclarationAuthority::LegacyGateway(
                    self.policy.protocol.recipient_provider,
                ),
            },
            payload_schema: request.payload_schema.clone(),
        };
        let admission = BondFreeAdmission {
            wire_version: WireVersion(1),
            sender: request.sender,
            recipient: request.recipient,
            message_id: request.message_id,
            content_ref: request.content_ref,
            delivery_intent_ref: request.delivery_intent_ref,
            declarations,
            message_valid_until: request.message_valid_until,
            capability: Some(request.capability),
            evidence: Some(LaneEvidence::Legacy(evidence)),
            idempotency_key: request.idempotency_key,
            protocol_version: request.protocol_version,
            deployment_domain: request.deployment_domain,
            intended_provider: request.intended_provider,
            authentication: AdmissionAuthentication::LegacyDmarc,
        };
        self.evaluate_admission_policy(
            &admission.declarations,
            admission.message_valid_until,
            admission.capability,
            admission.authentication,
            now,
        )?;
        Ok(self.engine.admit_bond_free(&admission, now)?)
    }

    /// Stores ciphertext only when its client-declared lifetime is bounded by service policy.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid time bounds or storage failure.
    pub fn upload_content(
        &self,
        record: &EncryptedContentRecord,
        certificate: &SignedContentKeyCertificate,
    ) -> Result<(), ServiceError> {
        let now = self.clock.now();
        let latest = now
            .checked_add(self.policy.max_content_lifetime)
            .ok_or(ServiceError::ContentTimeInvalid)?;
        if record.created_at > now || record.expires_at <= now || record.expires_at > latest {
            return Err(ServiceError::ContentTimeInvalid);
        }
        self.evaluate_admission_policy(
            &record.binding.declarations,
            record.binding.message_valid_until,
            record.binding.capability,
            match record.binding.declarations.origin.authority {
                DeclarationAuthority::NativeSender(key) => AdmissionAuthentication::NativeKey(key),
                DeclarationAuthority::LegacyGateway(_) => AdmissionAuthentication::LegacyDmarc,
            },
            now,
        )?;
        let relationship = self.engine.snapshot()?.state.relationship.key.reference;
        let registry = self.engine.key_registry()?;
        let digest = registry.verify_content_key_certificate(
            certificate,
            now,
            self.policy.protocol.protocol_version,
            SigningScope {
                deployment_domain: self.policy.deployment_domain,
                intended_provider: self.policy.protocol.recipient_provider,
                relationship,
            },
        )?;
        if certificate.certificate.key.reference != record.envelope.sender_key
            || certificate.certificate.owner != record.binding.sender
            || record.binding.relationship != relationship
            || record.binding.sender_certificate != digest
            || content_key_certificate_digest(&certificate.certificate) != digest
            || match record.binding.declarations.origin.authority {
                DeclarationAuthority::NativeSender(key) => {
                    key != certificate.certificate.operational_key
                }
                DeclarationAuthority::LegacyGateway(provider) => {
                    provider != self.policy.protocol.recipient_provider
                }
            }
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        self.engine
            .store_content_with_retention(record, self.policy.protocol.retention_policy_version)?;
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
        self.engine
            .register_operational_key(reference, actor, verifying_key, now)?;
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
        self.engine
            .revoke_operational_key(reference, expected_version, now)?;
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

fn receipt_kind(event: ProtocolEventKind) -> ReceiptKind {
    match event {
        ProtocolEventKind::TermsIssued => ReceiptKind::ContactTermsIssued,
        ProtocolEventKind::AttemptReserved(_) => ReceiptKind::ReservationCommitted,
        ProtocolEventKind::AttemptAdmitted(_) => ReceiptKind::AdmissionCommitted,
        ProtocolEventKind::RelationshipAccepted
        | ProtocolEventKind::RelationshipRejected
        | ProtocolEventKind::RelationshipBlocked
        | ProtocolEventKind::RelationshipUnblocked
        | ProtocolEventKind::RelationshipRevoked => ReceiptKind::RelationshipDecisionCommitted,
        ProtocolEventKind::ReservedAttemptCancelled(_)
        | ProtocolEventKind::MessageValidityClosed(_)
        | ProtocolEventKind::DeclarationMismatch(_)
        | ProtocolEventKind::BondExpired(_)
        | ProtocolEventKind::PersistenceReleased(_) => ReceiptKind::SettlementCommitted,
    }
}

fn receipt_ref(
    command: CommandDigest,
    position: cs_mail_primitives::JournalPosition,
    outcome: OutcomeDigest,
) -> ReceiptRef {
    let mut hasher = Sha256::new();
    hasher.update(b"cs-mail/receipt-reference/v1");
    hasher.update(command.0);
    hasher.update(position.0.to_be_bytes());
    hasher.update(outcome.0);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    ReceiptRef(u128::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{DeclaredPurpose, KnownPurpose, PayloadSchema};

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

    fn declarations_with_schema(criticality: ExtensionCriticality) -> MessageDeclarations {
        MessageDeclarations {
            purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
            origin: OriginDeclaration {
                mode: OriginMode::AutomatedSystem,
                authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
            },
            payload_schema: Some(PayloadSchema {
                id: NamespacedIdentifier::new("com.example", "invoice", 1).unwrap(),
                criticality,
            }),
        }
    }

    #[test]
    fn admission_policy_rejects_only_unsupported_critical_schemas() {
        let policy = DefaultAdmissionPolicy::default();
        let noncritical = declarations_with_schema(ExtensionCriticality::NonCritical);
        let critical = declarations_with_schema(ExtensionCriticality::Critical);
        let input = |declarations| AdmissionPolicyInput {
            declarations,
            message_valid_until: MessageValidityUntil(CanonicalTime(10)),
            capability: None,
            authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(1)),
        };

        assert_eq!(policy.evaluate(&input(&noncritical)), Ok(()));
        assert_eq!(
            policy.evaluate(&input(&critical)),
            Err(AdmissionPolicyReason::UnsupportedCriticalExtension)
        );

        let supported =
            DefaultAdmissionPolicy::with_supported_critical_schemas([NamespacedIdentifier::new(
                "com.example",
                "invoice",
                1,
            )
            .unwrap()]);
        assert_eq!(supported.evaluate(&input(&critical)), Ok(()));
    }
}
