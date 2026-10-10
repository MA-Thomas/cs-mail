//! Operator commands for canonical deployment configuration.
use crate::CommandFamily;
use cs_mail_billing::ServiceOffer;
use cs_mail_primitives::PolicyVersion;
use cs_mail_protocol::pricing::RequestPricingPolicy;
use serde::{Deserialize, Serialize};

/// Commands that change canonical, published configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperatorCommand {
    /// Publish an immutable annual service offer version.
    PublishServiceOffer(ServiceOffer),
    /// Publish a new request pricing policy version and make it current.
    PublishRequestPricing(RequestPricingPolicy),
}
impl CommandFamily for OperatorCommand {
    const SIGNING_DOMAIN: &'static str = "cs-mail/operator-command/v1";
    const NAME: &'static str = "configuration";
}

/// The recorded result of an operator command, returned again on an exact retry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperatorOutcome {
    ServiceOfferPublished {
        version: PolicyVersion,
        changed: bool,
    },
    RequestPricingPublished {
        version: PolicyVersion,
        changed: bool,
        addresses_with_unquotable_classes: usize,
    },
}
