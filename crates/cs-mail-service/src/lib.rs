//! A small authenticated ingress facade around the durable protocol engine.

use core::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use cs_mail_adapters::{DmarcError, DmarcVerifier, SmtpAuthenticationRequest};
use cs_mail_capabilities::{
    AdmissionAuthentication, LaneSubject, LegacyBondFreeAdmission, SignedBondFreeAdmission,
    SignedLaneControl, SignedLaneGrant,
};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_primitives::{
    CanonicalTime, DeclarationAuthority, Duration, IdempotencyKey, MessageDeclarations,
    MessageValidityUntil, OperationalKeyRef, ProtocolVersion, Version,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot, SettlementSnapshot, TermsOutcome};
use cs_mail_security::{
    KeyRegistry, ProviderSigner, SecurityError, SignedCommandBytes, SignedContactTerms,
    SignedContentKeyCertificate, SignedReceipt,
};
use cs_mail_storage_postgres::{PostgresEngine, StorageError};

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

pub use cs_mail_protocol::admission::{AdmissionFailure, AdmissionPolicy};
#[derive(Debug)]
pub enum ServiceError {
    Security(SecurityError),
    Storage(StorageError),
    ContentTimeInvalid,
    SigningScopeMismatch,
    ReceiptUnavailable,
    Dmarc(DmarcError),
    MessageValidityClosed,
    AdmissionPolicyRefused(AdmissionFailure),
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

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(e) => Some(e),
            Self::Security(e) => Some(e),
            _ => None,
        }
    }
}

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
pub struct ServiceOutcome {
    pub outcome: cs_mail_storage_postgres::ReceivedOutcome,
    pub signed_terms: Option<SignedContactTerms>,
    pub receipt: SignedReceipt,
}

pub struct BillingOutcome {
    pub account: cs_mail_billing::BillingAccount,
    pub operations: Vec<cs_mail_finance::PaymentOperation>,
    pub allocations: Vec<cs_mail_finance::MemberStatementEntry>,
}

pub struct IngressService<C> {
    engine: PostgresEngine,
    provider_signer: ProviderSigner,
    clock: C,
    policy: ServicePolicy,
    admission_policy: AdmissionPolicy,
}

impl<C: CanonicalClock> IngressService<C> {
    /// # Errors
    /// Rejects invalid recipient authority or an amount outside the configured menu.
    pub fn set_request_classes(
        &self,
        signed: &cs_mail_security::SignedRequestClasses,
    ) -> Result<(), ServiceError> {
        cs_mail_application::request_classes::RequestClassesService::new(&self.engine).publish(
            signed,
            &self.policy.protocol,
            || self.clock.now(),
        )?;
        Ok(())
    }
    /// # Errors
    /// Returns storage or invalid-publication errors.
    pub fn request_classes(
        &self,
    ) -> Result<Option<cs_mail_protocol::pricing::RecipientRequestClasses>, ServiceError> {
        Ok(self.engine.request_classes()?)
    }
    /// Authenticates the billing command independently of service coverage. Expired service
    /// never prevents a member from inspecting or collecting an existing allocation.
    /// # Errors
    /// Rejects invalid billing authority, scope, versions and financial transitions.
    pub fn submit_billing(
        &self,
        command: &cs_mail_billing::SignedBillingCommand,
    ) -> Result<BillingOutcome, ServiceError> {
        if command.scope != self.policy.protocol.financial.scope {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let operations =
            cs_mail_application::billing::operations::BillingService::new(&self.engine, &|| {
                self.clock.now()
            })
            .execute_command(command)?;
        let account = self.engine.billing_account(command.account)?;
        let allocations = self
            .engine
            .financial_program(account.unit())?
            .member_statement(account.member());
        Ok(BillingOutcome {
            account,
            operations,
            allocations,
        })
    }
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
            AdmissionPolicy::default(),
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
        admission_policy: AdmissionPolicy,
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
        engine.configure_ingress(policy.deployment_domain, &policy.protocol)?;
        engine.configure_admission_policy(&admission_policy)?;
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
        _capability: Option<cs_mail_primitives::LaneId>,
        _authentication: AdmissionAuthentication,
        now: CanonicalTime,
    ) -> Result<(), ServiceError> {
        if message_valid_until.0 < now {
            return Err(ServiceError::MessageValidityClosed);
        }
        self.admission_policy
            .evaluate(declarations)
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
    pub fn submit(&self, command: &SignedCommandBytes) -> Result<ServiceOutcome, ServiceError> {
        let handle = self.engine.receive_signed(
            command,
            self.policy.deployment_domain,
            || self.clock.now(),
            self.policy.protocol.clone(),
        )?;
        self.complete_receipt(&handle)
    }

    fn complete_receipt(
        &self,
        handle: &cs_mail_storage_postgres::ReceivedCommand,
    ) -> Result<ServiceOutcome, ServiceError> {
        self.engine.sign_artifacts_batch(
            &self.provider_signer,
            self.clock.now(),
            cs_mail_primitives::Duration(30_000),
            100,
        )?;
        let result = self.engine.process_received(handle);
        self.engine.sign_artifacts_batch(
            &self.provider_signer,
            self.clock.now(),
            cs_mail_primitives::Duration(30_000),
            100,
        )?;
        let outcome = result?;
        let signed_terms = match &outcome {
            cs_mail_storage_postgres::ReceivedOutcome::Protocol(o) => {
                match &o.transition.terms_outcome {
                    Some(TermsOutcome::ChargeRequired(t)) => {
                        Some(self.engine.signed_quote(t.quote_id)?)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        let receipt = self
            .engine
            .receipt(handle)?
            .ok_or(ServiceError::ReceiptUnavailable)?;
        Ok(ServiceOutcome {
            outcome,
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
    ) -> Result<ServiceOutcome, ServiceError> {
        if let LaneSubject::LegacyDomain(domain) = &signed.grant.subject
            && domain.synthetic_protocol_identity(
                &self.policy.deployment_domain,
                self.policy.legacy_identity_mapping_version,
            ) != signed.grant.sender
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        let handle = self.engine.receive_grant(
            signed,
            idempotency_key,
            self.policy.deployment_domain,
            || self.clock.now(),
            self.policy.protocol.clone(),
        )?;
        self.complete_receipt(&handle)
    }

    /// Verifies and applies an explicit recipient revocation or reconfirmation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid signature/scope, stale version, block,
    /// conflicting replay, or storage failure.
    pub fn control_lane(&self, signed: &SignedLaneControl) -> Result<ServiceOutcome, ServiceError> {
        let handle = self.engine.receive_control(
            signed,
            self.policy.deployment_domain,
            || self.clock.now(),
            self.policy.protocol.clone(),
        )?;
        self.complete_receipt(&handle)
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
    ) -> Result<ServiceOutcome, ServiceError> {
        let handle = self.engine.receive_message(
            signed,
            self.policy.deployment_domain,
            || self.clock.now(),
            self.policy.protocol.clone(),
        )?;
        self.complete_receipt(&handle)
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
    ) -> Result<ServiceOutcome, ServiceError> {
        let proof = cs_mail_capabilities::verify_legacy_admission(
            verifier,
            authentication,
            request,
            self.policy.legacy_identity_mapping_version,
        )
        .await
        .map_err(StorageError::from)?;
        let handle = self.engine.receive_legacy(
            &proof,
            self.policy.deployment_domain,
            || self.clock.now(),
            self.policy.protocol.clone(),
        )?;
        self.complete_receipt(&handle)
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
        self.engine.store_authenticated_content(
            record,
            certificate,
            || self.clock.now(),
            &self.policy.protocol,
            self.policy.deployment_domain,
        )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{
        DeclaredPurpose, ExtensionCriticality, KnownPurpose, NamespacedIdentifier,
        OriginDeclaration, OriginMode, PayloadSchema,
    };

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
        let policy = AdmissionPolicy::default();
        let noncritical = declarations_with_schema(ExtensionCriticality::NonCritical);
        let critical = declarations_with_schema(ExtensionCriticality::Critical);

        assert_eq!(policy.evaluate(&noncritical), Ok(()));
        assert_eq!(
            policy.evaluate(&critical),
            Err(AdmissionFailure::UnsupportedCriticalExtension)
        );

        let supported = AdmissionPolicy {
            version: Version(1),
            supported_critical_schemas: std::collections::BTreeSet::from([
                NamespacedIdentifier::new("com.example", "invoice", 1).unwrap(),
            ]),
        };
        assert_eq!(supported.evaluate(&critical), Ok(()));
    }
}
