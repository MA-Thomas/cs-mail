//! Recipient-defined request classes resolve into immutable quotes; existing requests are unaffected.
use cs_mail_primitives::{Money, ProtocolIdentity, RequestClassId};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestPricingPolicy {
    pub version: u64,
    pub processing_charge: Money,
    /// Suggested amount when composing a new class; never an implicit request class.
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
    /// Checks the complete recipient publication against this operator's menu.
    pub fn permits(&self, classes: &RecipientRequestClasses) -> bool {
        self.valid()
            && classes
                .classes
                .iter()
                .all(|c| self.collateral_choices.contains(&c.collateral))
    }
    /// # Errors
    /// Rejects unknown classes or collateral outside the operator's menu.
    pub fn resolve_quote(
        &self,
        classes: &RecipientRequestClasses,
        id: RequestClassId,
    ) -> Result<ResolvedRequestPricing, crate::ProtocolError> {
        if !self.permits(classes) {
            return Err(crate::ProtocolError::PolicyInvalid);
        }
        let class = classes
            .classes
            .iter()
            .find(|c| c.id() == id)
            .ok_or(crate::ProtocolError::PolicyInvalid)?;
        Ok(ResolvedRequestPricing {
            version: cs_mail_primitives::PolicyVersion(self.version),
            processing_charge: self.processing_charge,
            collateral: class.collateral,
            selected: class.selection.clone(),
        })
    }
}

pub const MAX_REQUEST_CLASSES: usize = 8;

/// Immutable description retained in signed terms, independent of later publication edits.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(try_from = "SelectedClassInput")]
pub struct SelectedRequestClass {
    id: RequestClassId,
    description: String,
}
#[derive(Deserialize)]
struct SelectedClassInput {
    id: RequestClassId,
    description: String,
}
impl TryFrom<SelectedClassInput> for SelectedRequestClass {
    type Error = String;
    fn try_from(v: SelectedClassInput) -> Result<Self, Self::Error> {
        Self::new(v.id, v.description).map_err(|e| format!("{e:?}"))
    }
}
impl SelectedRequestClass {
    /// # Errors
    /// Rejects missing identity or description.
    pub fn new(id: RequestClassId, description: String) -> Result<Self, crate::ProtocolError> {
        if id.0 == 0 || description.trim().is_empty() {
            return Err(crate::ProtocolError::PolicyInvalid);
        }
        Ok(Self { id, description })
    }
    pub const fn id(&self) -> RequestClassId {
        self.id
    }
    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ClassInput")]
pub struct RequestClass {
    selection: SelectedRequestClass,
    collateral: Money,
}
#[derive(Deserialize)]
struct ClassInput {
    selection: SelectedRequestClass,
    collateral: Money,
}
impl TryFrom<ClassInput> for RequestClass {
    type Error = String;
    fn try_from(v: ClassInput) -> Result<Self, Self::Error> {
        Self::new(v.selection.id, v.selection.description, v.collateral)
            .map_err(|e| format!("{e:?}"))
    }
}
impl RequestClass {
    /// # Errors
    /// Rejects missing identity, description or collateral.
    pub fn new(
        id: RequestClassId,
        description: String,
        collateral: Money,
    ) -> Result<Self, crate::ProtocolError> {
        if collateral.is_zero() {
            return Err(crate::ProtocolError::PolicyInvalid);
        }
        Ok(Self {
            selection: SelectedRequestClass::new(id, description)?,
            collateral,
        })
    }
    pub const fn id(&self) -> RequestClassId {
        self.selection.id
    }
    pub fn description(&self) -> &str {
        self.selection.description()
    }
    pub const fn collateral(&self) -> Money {
        self.collateral
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ClassesInput")]
pub struct RecipientRequestClasses {
    recipient: ProtocolIdentity,
    version: u64,
    classes: Vec<RequestClass>,
}
#[derive(Deserialize)]
struct ClassesInput {
    recipient: ProtocolIdentity,
    version: u64,
    classes: Vec<RequestClass>,
}
impl TryFrom<ClassesInput> for RecipientRequestClasses {
    type Error = String;
    fn try_from(v: ClassesInput) -> Result<Self, Self::Error> {
        Self::new(v.recipient, v.version, v.classes).map_err(|e| format!("{e:?}"))
    }
}
impl RecipientRequestClasses {
    /// # Errors
    /// Rejects missing owner/version, duplicate IDs, or more than eight classes.
    pub fn new(
        recipient: ProtocolIdentity,
        version: u64,
        classes: Vec<RequestClass>,
    ) -> Result<Self, crate::ProtocolError> {
        let ids: std::collections::BTreeSet<_> = classes.iter().map(RequestClass::id).collect();
        if recipient.0 == 0
            || version == 0
            || classes.len() > MAX_REQUEST_CLASSES
            || ids.len() != classes.len()
        {
            return Err(crate::ProtocolError::PolicyInvalid);
        }
        Ok(Self {
            recipient,
            version,
            classes,
        })
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.recipient
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn classes(&self) -> &[RequestClass] {
        &self.classes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRequestPricing {
    version: cs_mail_primitives::PolicyVersion,
    processing_charge: Money,
    collateral: Money,
    selected: SelectedRequestClass,
}
impl crate::PolicySnapshot {
    pub fn apply_pricing(&mut self, pricing: &ResolvedRequestPricing) {
        self.pricing_policy_version = pricing.version;
        self.processing_charge = pricing.processing_charge;
        self.collateral = pricing.collateral;
        self.selected_class = Some(pricing.selected.clone());
    }
}
