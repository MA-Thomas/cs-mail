//! CSQD's request pricing policy and each address's request-class publication.
//!
//! Both are deployment-level facts, authorized and validated against protected current
//! context; neither depends on any relationship. Quote resolution itself is the pure
//! [`RequestPricingPolicy::quote`].
use cs_mail_primitives::{CanonicalTime, PolicyVersion};
use cs_mail_protocol::ActorRef;
use cs_mail_protocol::{
    ProtocolError,
    pricing::{RecipientRequestClasses, RequestPricingPolicy},
};
use cs_mail_security::{
    AuthoritySnapshot, RecipientSigningScope, SecurityError, SignedRequestClasses,
};

/// Protected context for publishing an operator policy version.
pub struct PolicyPublicationContext {
    /// The current policy, if any.
    pub current: Option<RequestPricingPolicy>,
    /// An already stored policy with the same version, if any.
    pub same_version: Option<RequestPricingPolicy>,
    /// Every address's current publication, to report the effect of new bounds.
    pub publications: Vec<RecipientRequestClasses>,
}
/// Outcome of publishing an operator policy version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyPublicationReport {
    pub version: PolicyVersion,
    /// False for an exact replay of the current version.
    pub changed: bool,
    /// Addresses with at least one class outside the new bounds. Those classes cannot be
    /// quoted until the address republishes.
    pub addresses_with_unquotable_classes: usize,
}
pub struct PolicyPublicationDecision {
    next: Option<(RequestPricingPolicy, CanonicalTime)>,
    report: PolicyPublicationReport,
}
impl PolicyPublicationDecision {
    pub fn into_parts(
        self,
    ) -> (
        Option<(RequestPricingPolicy, CanonicalTime)>,
        PolicyPublicationReport,
    ) {
        (self.next, self.report)
    }
}

/// Protected context for one address's publication.
pub struct ClassesPublicationContext {
    /// Authority of the account owning the address, present only if that account may use
    /// the service.
    pub authority: AuthoritySnapshot,
    pub policy: Option<RequestPricingPolicy>,
    pub current: Option<RecipientRequestClasses>,
}
pub struct ClassesPublicationDecision {
    next: Option<(RecipientRequestClasses, CanonicalTime)>,
}
impl ClassesPublicationDecision {
    pub fn into_effects(self) -> Option<(RecipientRequestClasses, CanonicalTime)> {
        self.next
    }
}

/// Atomic persistence of pricing publications. Each method invokes its callback once,
/// after loading all protected context, and commits its effects or nothing.
pub trait RequestPricingStore {
    type Error: From<ProtocolError> + From<SecurityError>;
    /// # Errors
    /// Rolls back on a refused decision or a persistence failure.
    fn transact_pricing_policy(
        &self,
        policy: &RequestPricingPolicy,
        decide: impl FnOnce(PolicyPublicationContext) -> Result<PolicyPublicationDecision, Self::Error>,
    ) -> Result<PolicyPublicationReport, Self::Error>;
    /// # Errors
    /// Rolls back on a refused decision or a persistence failure.
    fn transact_request_classes(
        &self,
        signed: &SignedRequestClasses,
        decide: impl FnOnce(
            ClassesPublicationContext,
        ) -> Result<ClassesPublicationDecision, Self::Error>,
    ) -> Result<(), Self::Error>;
}

pub struct RequestPricingService<'a, R> {
    repository: &'a R,
}
impl<'a, R: RequestPricingStore> RequestPricingService<'a, R> {
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    /// Publishes a new operator policy version and makes it current. An exact replay of the
    /// current version succeeds without change.
    /// # Errors
    /// `DuplicateConflict` for a stored version with different content, `VersionConflict`
    /// for a version not newer than the current one, and `PolicyInvalid` for a change of
    /// settlement unit.
    pub fn publish_policy(
        &self,
        policy: &RequestPricingPolicy,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<PolicyPublicationReport, R::Error> {
        self.repository.transact_pricing_policy(policy, |context| {
            let at = clock();
            if context.same_version.as_ref().is_some_and(|p| p != policy) {
                return Err(ProtocolError::DuplicateConflict.into());
            }
            let affected = context
                .publications
                .iter()
                .filter(|p| !policy.permits(p))
                .count();
            if let Some(current) = &context.current {
                if current == policy {
                    return Ok(PolicyPublicationDecision {
                        next: None,
                        report: PolicyPublicationReport {
                            version: policy.version(),
                            changed: false,
                            addresses_with_unquotable_classes: affected,
                        },
                    });
                }
                if policy.version() <= current.version() {
                    return Err(ProtocolError::VersionConflict.into());
                }
                if policy.unit() != current.unit() {
                    return Err(ProtocolError::PolicyInvalid.into());
                }
            }
            Ok(PolicyPublicationDecision {
                next: Some((policy.clone(), at)),
                report: PolicyPublicationReport {
                    version: policy.version(),
                    changed: true,
                    addresses_with_unquotable_classes: affected,
                },
            })
        })
    }
    /// Publishes one address's classes. The signature must come from an active key of that
    /// address, and every class must lie within the current policy's bounds. Versions
    /// strictly increase; an identical replay succeeds without change.
    /// # Errors
    /// Rejects a foreign scope or key, a missing policy, out-of-bounds collateral, an
    /// altered replay or a stale version.
    pub fn publish_classes(
        &self,
        signed: &SignedRequestClasses,
        expected: RecipientSigningScope,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<(), R::Error> {
        self.repository.transact_request_classes(signed, |context| {
            let now = clock();
            if signed.scope != expected {
                return Err(SecurityError::SigningScopeMismatch.into());
            }
            let key = context.authority.active_verifying_key(
                signed.operational_key,
                ActorRef::Recipient(signed.classes.recipient()),
                now,
            )?;
            signed.verify(&key)?;
            let policy = context.policy.ok_or(ProtocolError::PolicyInvalid)?;
            if !policy.permits(&signed.classes) {
                return Err(ProtocolError::RequestClassOutsidePolicy.into());
            }
            if let Some(old) = context.current {
                if old == signed.classes {
                    return Ok(ClassesPublicationDecision { next: None });
                }
                if signed.classes.version() <= old.version() {
                    return Err(ProtocolError::VersionConflict.into());
                }
            }
            Ok(ClassesPublicationDecision {
                next: Some((signed.classes.clone(), now)),
            })
        })
    }
}
