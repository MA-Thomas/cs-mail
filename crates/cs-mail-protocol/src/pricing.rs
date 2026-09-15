//! Recipient request preferences resolve into immutable quotes; existing requests are unaffected.
use cs_mail_primitives::{Money, ProtocolIdentity};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestPricingPolicy {
    pub version: u64,
    pub processing_charge: Money,
    pub default_collateral: Money,
    pub collateral_choices: Vec<Money>,
}
impl Default for RequestPricingPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            processing_charge: Money::from_minor_units(50),
            default_collateral: Money::from_minor_units(500),
            collateral_choices: [100, 250, 500, 1000, 2500]
                .map(Money::from_minor_units)
                .to_vec(),
        }
    }
}
impl RequestPricingPolicy {
    pub fn valid(&self) -> bool {
        self.version != 0
            && !self.processing_charge.is_zero()
            && self.collateral_choices.contains(&self.default_collateral)
            && self.collateral_choices.windows(2).all(|p| p[0] < p[1])
            && self
                .collateral_choices
                .iter()
                .all(|s| !s.is_zero() && self.processing_charge.checked_add(*s).is_some())
    }
    pub fn resolve(&self, preference: Option<&RecipientCollateralPreference>) -> Option<Money> {
        if !self.valid() {
            return None;
        }
        let amount = preference.map_or(self.default_collateral, |p| p.amount);
        self.collateral_choices.contains(&amount).then_some(amount)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecipientCollateralPreference {
    pub recipient: ProtocolIdentity,
    pub version: u64,
    pub amount: Money,
}

/// Validated quote pricing. A preference can select collateral but cannot set the operator's fee.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRequestPricing {
    version: cs_mail_primitives::PolicyVersion,
    processing_charge: Money,
    collateral: Money,
}
impl RequestPricingPolicy {
    /// # Errors
    /// Rejects invalid policy or a preference outside the published collateral menu.
    pub fn resolve_quote(
        &self,
        preference: Option<&RecipientCollateralPreference>,
    ) -> Result<ResolvedRequestPricing, crate::ProtocolError> {
        let collateral = self
            .resolve(preference)
            .ok_or(crate::ProtocolError::PolicyInvalid)?;
        Ok(ResolvedRequestPricing {
            version: cs_mail_primitives::PolicyVersion(self.version),
            processing_charge: self.processing_charge,
            collateral,
        })
    }
}
impl crate::PolicySnapshot {
    pub fn apply_pricing(&mut self, pricing: &ResolvedRequestPricing) {
        self.pricing_policy_version = pricing.version;
        self.processing_charge = pricing.processing_charge;
        self.collateral = pricing.collateral;
    }
}
