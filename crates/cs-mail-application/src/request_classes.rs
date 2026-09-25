//! Recipient-owned publication, authorized against protected current context.
use cs_mail_primitives::CanonicalTime;
use cs_mail_protocol::ActorRef;
use cs_mail_protocol::{
    PolicySnapshot, ProtocolError,
    pricing::{RecipientRequestClasses, RequestPricingPolicy},
};
use cs_mail_security::{AuthoritySnapshot, SecurityError, SignedRequestClasses, SigningScope};

pub struct PublicationContext {
    pub scope: SigningScope,
    pub recipient: cs_mail_primitives::ProtocolIdentity,
    pub registry: AuthoritySnapshot,
    pub pricing: RequestPricingPolicy,
    pub current: Option<RecipientRequestClasses>,
}
pub struct PublicationDecision {
    next: Option<RecipientRequestClasses>,
}
impl PublicationDecision {
    pub fn into_effects(self) -> Option<RecipientRequestClasses> {
        self.next
    }
}
/// Protect recipient publication and signing authority through atomic commit.
/// Invoke the local callback once, after loading all protected context.
pub trait RequestClassesStore {
    type Error: From<ProtocolError> + From<SecurityError>;
    /// # Errors
    /// Rolls back the publication on failed authorization, validation or persistence.
    fn transact_request_classes(
        &self,
        signed: &SignedRequestClasses,
        policy: &PolicySnapshot,
        decide: impl FnOnce(PublicationContext) -> Result<PublicationDecision, Self::Error>,
    ) -> Result<(), Self::Error>;
}
pub struct RequestClassesService<'a, R> {
    repository: &'a R,
}
impl<'a, R: RequestClassesStore> RequestClassesService<'a, R> {
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    /// # Errors
    /// Rejects foreign authority, invalid menu choices, altered replay or stale versions.
    pub fn publish(
        &self,
        signed: &SignedRequestClasses,
        policy: &PolicySnapshot,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<(), R::Error> {
        self.repository
            .transact_request_classes(signed, policy, |context| {
                let now = clock();
                if context.scope != signed.scope || context.recipient != signed.classes.recipient()
                {
                    return Err(SecurityError::SigningScopeMismatch.into());
                }
                let key = context.registry.active_verifying_key(
                    signed.operational_key,
                    ActorRef::Recipient(context.recipient),
                    now,
                )?;
                signed.verify(&key)?;
                if !context.pricing.permits(&signed.classes) {
                    return Err(ProtocolError::PolicyInvalid.into());
                }
                if let Some(old) = context.current {
                    if old == signed.classes {
                        return Ok(PublicationDecision { next: None });
                    }
                    if signed.classes.version() <= old.version() {
                        return Err(ProtocolError::VersionConflict.into());
                    }
                }
                Ok(PublicationDecision {
                    next: Some(signed.classes.clone()),
                })
            })
    }
}
