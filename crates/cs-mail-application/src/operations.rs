//! Deployment bootstrap and authenticated operator commands.
//!
//! Bootstrap creates the deployment's identity and trust roots exactly once. Operator
//! commands are verified against the operator keys installed at bootstrap, are idempotent
//! under their identity, and commit their effect together with their audit record.
use crate::request_pricing::{self, PolicyPublicationContext};
use cs_mail_billing::{BillingError, ServiceOffer};
use cs_mail_operations::{
    BootstrapOutcome, CommandReplay, DeploymentBootstrap, OperationsError, OperatorAuthority,
    OperatorCommand, OperatorOutcome, SignedOperatorCommand, VerifiedOperatorCommand, replay,
};
use cs_mail_primitives::CanonicalTime;
use cs_mail_protocol::ProtocolError;
use cs_mail_protocol::policy::ProtocolPolicy;
use cs_mail_protocol::pricing::RequestPricingPolicy;

/// The bootstrap decision and the trusted instant it was made.
pub struct BootstrapDecision {
    pub outcome: BootstrapOutcome,
    pub at: CanonicalTime,
}

/// Atomic creation of the deployment and all its trust roots.
pub trait BootstrapStore {
    type Error: From<OperationsError>;
    /// Loads the stored deployment under protection, invokes the callback once, and on
    /// `Created` writes the deployment record, payment trust and identity trust together.
    /// # Errors
    /// Rolls back everything on a refused decision or any persistence failure.
    fn transact_bootstrap(
        &self,
        bootstrap: &DeploymentBootstrap,
        decide: impl FnOnce(Option<DeploymentBootstrap>) -> Result<BootstrapDecision, Self::Error>,
    ) -> Result<BootstrapOutcome, Self::Error>;
}

/// Creates the deployment, or confirms an identical one. Trusted time is sampled after the
/// stored deployment has been loaded under protection.
/// # Errors
/// `BootstrapConflict` when a different deployment exists; persistence failures.
pub fn bootstrap<R: BootstrapStore>(
    repository: &R,
    bootstrap: &DeploymentBootstrap,
    clock: impl FnOnce() -> CanonicalTime,
) -> Result<BootstrapOutcome, R::Error> {
    repository.transact_bootstrap(bootstrap, |stored| {
        let outcome = bootstrap.reconcile(stored.as_ref())?;
        Ok(BootstrapDecision {
            outcome,
            at: clock(),
        })
    })
}

/// What the adapter loads for the command's target, under protection.
pub enum OperatorTarget {
    ServiceOffer {
        /// A stored offer with the same version, if any.
        same_version: Option<ServiceOffer>,
    },
    RequestPricing(PolicyPublicationContext),
    ProtocolPolicy {
        /// The current protocol policy, if any.
        current: Option<Box<ProtocolPolicy>>,
        /// A stored protocol policy with the same version, if any.
        same_version: Option<Box<ProtocolPolicy>>,
    },
}
pub struct OperatorContext {
    pub authority: OperatorAuthority,
    /// The digest and outcome recorded under this command's identity, if any.
    pub prior: Option<([u8; 32], OperatorOutcome)>,
    pub target: OperatorTarget,
}
/// The canonical change an operator command makes.
pub enum OperatorEffect {
    PublishServiceOffer(ServiceOffer),
    PublishRequestPricing(RequestPricingPolicy, CanonicalTime),
    PublishProtocolPolicy(ProtocolPolicy, CanonicalTime),
}
/// The audit record committed with the effect.
pub struct OperatorRecord {
    pub command: VerifiedOperatorCommand<OperatorCommand>,
    pub outcome: OperatorOutcome,
    pub authorized_at: CanonicalTime,
}
pub struct OperatorDecision {
    record: Option<OperatorRecord>,
    effect: Option<OperatorEffect>,
    outcome: OperatorOutcome,
}
impl OperatorDecision {
    pub fn into_parts(
        self,
    ) -> (
        Option<OperatorRecord>,
        Option<OperatorEffect>,
        OperatorOutcome,
    ) {
        (self.record, self.effect, self.outcome)
    }
}

/// Atomic execution of one operator command with its audit record.
pub trait OperatorStore {
    type Error: From<OperationsError> + From<ProtocolError> + From<BillingError>;
    /// # Errors
    /// Rolls back the effect and the record together on refusal or persistence failure.
    fn transact_operator_command(
        &self,
        signed: &SignedOperatorCommand<OperatorCommand>,
        decide: impl FnOnce(OperatorContext) -> Result<OperatorDecision, Self::Error>,
    ) -> Result<OperatorOutcome, Self::Error>;
}

pub struct OperatorService<'a, R> {
    repository: &'a R,
}
impl<'a, R: OperatorStore> OperatorService<'a, R> {
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    /// Verifies and executes one operator command. An exact retry returns the recorded
    /// outcome without a new effect.
    /// # Errors
    /// `Unauthorized` for a foreign deployment or key or a bad signature,
    /// `DuplicateConflict` when the identity was used for another command, and the
    /// target's own refusals (a changed offer version, a stale pricing version).
    pub fn execute(
        &self,
        signed: &SignedOperatorCommand<OperatorCommand>,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<OperatorOutcome, R::Error> {
        self.repository
            .transact_operator_command(signed, |context| {
                let verified = context.authority.verify(signed)?;
                if let CommandReplay::Replay(outcome) =
                    replay(&verified, context.prior.as_ref().map(|(d, o)| (d, o)))?
                {
                    return Ok(OperatorDecision {
                        record: None,
                        effect: None,
                        outcome,
                    });
                }
                let at = clock();
                let (effect, outcome) = match (verified.command(), context.target) {
                    (
                        OperatorCommand::PublishServiceOffer(offer),
                        OperatorTarget::ServiceOffer { same_version },
                    ) => {
                        let changed = match same_version {
                            None => true,
                            Some(existing) if existing == *offer => false,
                            Some(_) => return Err(BillingError::Conflict.into()),
                        };
                        (
                            changed.then(|| OperatorEffect::PublishServiceOffer(offer.clone())),
                            OperatorOutcome::ServiceOfferPublished {
                                version: offer.version(),
                                changed,
                            },
                        )
                    }
                    (
                        OperatorCommand::PublishRequestPricing(policy),
                        OperatorTarget::RequestPricing(pricing),
                    ) => {
                        let (next, report) =
                            request_pricing::decide_policy(policy, &pricing, at)?.into_parts();
                        (
                            next.map(|(policy, at)| {
                                OperatorEffect::PublishRequestPricing(policy, at)
                            }),
                            OperatorOutcome::RequestPricingPublished {
                                version: report.version,
                                changed: report.changed,
                                addresses_with_unquotable_classes: report
                                    .addresses_with_unquotable_classes,
                            },
                        )
                    }
                    (
                        OperatorCommand::PublishProtocolPolicy(policy),
                        OperatorTarget::ProtocolPolicy {
                            current,
                            same_version,
                        },
                    ) => {
                        let changed = decide_protocol_policy(
                            policy,
                            current.as_deref(),
                            same_version.as_deref(),
                        )?;
                        (
                            changed
                                .then(|| OperatorEffect::PublishProtocolPolicy(policy.clone(), at)),
                            OperatorOutcome::ProtocolPolicyPublished {
                                version: policy.version(),
                                changed,
                            },
                        )
                    }
                    _ => return Err(OperationsError::Invalid.into()),
                };
                Ok(OperatorDecision {
                    record: Some(OperatorRecord {
                        command: verified,
                        outcome,
                        authorized_at: at,
                    }),
                    effect,
                    outcome,
                })
            })
    }
}

/// Decides publishing a protocol policy version. Returns whether it changes the current
/// policy; an exact replay of the current version changes nothing. Issued request terms
/// keep the windows they were issued under.
/// # Errors
/// `DuplicateConflict` for a stored version with different content and `VersionConflict`
/// for a version not newer than the current one.
pub fn decide_protocol_policy(
    policy: &ProtocolPolicy,
    current: Option<&ProtocolPolicy>,
    same_version: Option<&ProtocolPolicy>,
) -> Result<bool, ProtocolError> {
    if same_version.is_some_and(|stored| stored != policy) {
        return Err(ProtocolError::DuplicateConflict);
    }
    match current {
        Some(current) if current == policy => Ok(false),
        Some(current) if policy.version() <= current.version() => {
            Err(ProtocolError::VersionConflict)
        }
        _ => Ok(true),
    }
}
