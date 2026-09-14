//! Pairwise identifiers, minimized audit records, safe telemetry, and executable retention.

use std::collections::BTreeMap;

use cs_mail_primitives::{
    AuditRef, CanonicalTime, ContentScopeRef, Duration, PrincipalRef, ProtocolIdentity,
    RelationshipRef, RequestHistoryRef, RetentionClassId,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ScopedHandle(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PrincipalAssertion(pub [u8; 32]);

/// Holds purpose-specific derivation material and deliberately omits `Debug` and serialization.
pub struct PrivacyDeriver {
    relationship_secret: [u8; 32],
    principal_secret: [u8; 32],
}

impl PrivacyDeriver {
    pub const fn new(relationship_secret: [u8; 32], principal_secret: [u8; 32]) -> Self {
        Self {
            relationship_secret,
            principal_secret,
        }
    }

    pub fn relationship_handle(
        &self,
        service_domain: &[u8],
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> ScopedHandle {
        ScopedHandle(derive(
            &self.relationship_secret,
            b"cs-mail/relationship-handle/v1",
            service_domain,
            sender.0,
            recipient.0,
        ))
    }

    pub fn relationship_ref(
        &self,
        derivation_version: u16,
        service_domain: &[u8],
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> RelationshipRef {
        RelationshipRef::new(
            derivation_version,
            derive(
                &self.relationship_secret,
                b"cs-mail/relationship-ref/v1",
                service_domain,
                sender.0,
                recipient.0,
            ),
        )
    }

    pub fn principal_assertion(
        &self,
        service_domain: &[u8],
        principal: PrincipalRef,
        recipient: ProtocolIdentity,
    ) -> PrincipalAssertion {
        PrincipalAssertion(derive(
            &self.principal_secret,
            b"cs-mail/principal-assertion/v1",
            service_domain,
            principal.0,
            recipient.0,
        ))
    }

    pub fn request_history_ref(
        &self,
        derivation_version: u16,
        service_domain: &[u8],
        principal: PrincipalRef,
        recipient: ProtocolIdentity,
    ) -> RequestHistoryRef {
        RequestHistoryRef::new(
            derivation_version,
            derive(
                &self.principal_secret,
                b"cs-mail/request-history/v1",
                service_domain,
                principal.0,
                recipient.0,
            ),
        )
    }

    pub fn content_scope_ref(
        &self,
        derivation_version: u16,
        service_domain: &[u8],
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> ContentScopeRef {
        ContentScopeRef::new(
            derivation_version,
            derive(
                &self.relationship_secret,
                b"cs-mail/content-scope/v1",
                service_domain,
                sender.0,
                recipient.0,
            ),
        )
    }
}

fn derive(secret: &[u8; 32], label: &[u8], domain: &[u8], left: u128, right: u128) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
    mac.update(label);
    mac.update(&(domain.len() as u64).to_be_bytes());
    mac.update(domain);
    mac.update(&left.to_be_bytes());
    mac.update(&right.to_be_bytes());
    mac.finalize().into_bytes().into()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TranslationPurpose {
    RouteDelivery,
    EnforceRepeatedAttempt,
    PresentRelationshipToOwner,
    ApplySettlement,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccessDecision {
    Allowed,
    Denied,
}

/// Records that a scoped translation occurred without reproducing either identifier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranslationAuditEvent {
    pub reference: AuditRef,
    pub purpose: TranslationPurpose,
    pub decision: AccessDecision,
    pub occurred_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SafeMetric {
    CommandAccepted,
    CommandRejected,
    DeliverySucceeded,
    DeliveryRetried,
    RetentionDeleted,
}

/// Intentionally contains no user, relationship, message, content, or balance identifier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SafeTelemetryEvent {
    pub metric: SafeMetric,
    pub coarse_time_bucket: u64,
}

impl SafeTelemetryEvent {
    pub const fn hourly(metric: SafeMetric, now: CanonicalTime) -> Self {
        Self {
            metric,
            coarse_time_bucket: now.0 / 3_600_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RetentionPurpose {
    PendingProtocolOperation,
    DeliveryRetry,
    UserContent,
    SettlementAudit,
    SecurityInvestigation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccessDomain {
    Relationship,
    Content,
    Ledger,
    Telemetry,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeletionMethod {
    PhysicalDelete,
    DestroyRecordKeyThenExpireBackups,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetentionClass {
    pub id: RetentionClassId,
    pub purpose: RetentionPurpose,
    pub access_domain: AccessDomain,
    pub operational_lifetime: Duration,
    pub dispute_lifetime: Option<Duration>,
    pub required_lifetime: Option<Duration>,
    pub deletion: DeletionMethod,
}

impl RetentionClass {
    pub fn delete_after(&self, created_at: CanonicalTime) -> Option<CanonicalTime> {
        let lifetime = [
            Some(self.operational_lifetime),
            self.dispute_lifetime,
            self.required_lifetime,
        ]
        .into_iter()
        .flatten()
        .max_by_key(|duration| duration.0)?;
        created_at.checked_add(lifetime)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetentionCatalog {
    classes: BTreeMap<RetentionClassId, RetentionClass>,
}

impl RetentionCatalog {
    pub fn register(&mut self, class: RetentionClass) -> Option<RetentionClass> {
        self.classes.insert(class.id, class)
    }

    pub fn class(&self, id: RetentionClassId) -> Option<&RetentionClass> {
        self.classes.get(&id)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeletionManifest {
    pub audit_ref: AuditRef,
    pub class: RetentionClassId,
    pub deleted_at: CanonicalTime,
    pub method: DeletionMethod,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_directional_recipient_scoped_and_domain_separated() {
        let deriver = PrivacyDeriver::new([1; 32], [2; 32]);
        let forward = deriver.relationship_handle(
            b"relationship-store",
            ProtocolIdentity(1),
            ProtocolIdentity(2),
        );
        assert_ne!(
            forward,
            deriver.relationship_handle(
                b"relationship-store",
                ProtocolIdentity(2),
                ProtocolIdentity(1)
            )
        );
        assert_ne!(
            forward,
            deriver.relationship_handle(b"telemetry", ProtocolIdentity(1), ProtocolIdentity(2))
        );
        assert_ne!(
            deriver.principal_assertion(b"attempts", PrincipalRef(1), ProtocolIdentity(2)),
            deriver.principal_assertion(b"attempts", PrincipalRef(1), ProtocolIdentity(3))
        );
    }

    #[test]
    fn longest_documented_retention_rule_controls_deletion() {
        let class = RetentionClass {
            id: RetentionClassId(1),
            purpose: RetentionPurpose::SettlementAudit,
            access_domain: AccessDomain::Ledger,
            operational_lifetime: Duration(10),
            dispute_lifetime: Some(Duration(20)),
            required_lifetime: Some(Duration(15)),
            deletion: DeletionMethod::DestroyRecordKeyThenExpireBackups,
        };
        assert_eq!(
            class.delete_after(CanonicalTime(5)),
            Some(CanonicalTime(25))
        );
    }

    #[test]
    fn safe_telemetry_has_only_metric_and_coarse_time() {
        assert_eq!(
            SafeTelemetryEvent::hourly(SafeMetric::DeliverySucceeded, CanonicalTime(7_200_001)),
            SafeTelemetryEvent {
                metric: SafeMetric::DeliverySucceeded,
                coarse_time_bucket: 2,
            }
        );
    }
}
