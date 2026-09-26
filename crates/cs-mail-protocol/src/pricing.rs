//! CSQD's request pricing policy and recipient-defined request classes. Quotes resolve one
//! class against the current policy into an immutable [`RequestPrice`]; issued terms never
//! change afterwards.
use cs_mail_primitives::{Money, PolicyVersion, ProtocolIdentity, RequestClassId, SettlementUnit};
use serde::{Deserialize, Serialize};

/// The operator's single cs-mail-wide pricing policy: processing component `C` and the
/// bounds within which every request class's collateral `S` must lie. A published
/// version is immutable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PolicyInput")]
pub struct RequestPricingPolicy {
    version: PolicyVersion,
    unit: SettlementUnit,
    processing_charge: Money,
    collateral_min: Money,
    collateral_max: Money,
}
#[derive(Deserialize)]
struct PolicyInput {
    version: PolicyVersion,
    unit: SettlementUnit,
    processing_charge: Money,
    collateral_min: Money,
    collateral_max: Money,
}
impl TryFrom<PolicyInput> for RequestPricingPolicy {
    type Error = String;
    fn try_from(v: PolicyInput) -> Result<Self, Self::Error> {
        Self::new(
            v.version,
            v.unit,
            v.processing_charge,
            v.collateral_min,
            v.collateral_max,
        )
        .map_err(|e| format!("{e:?}"))
    }
}
impl RequestPricingPolicy {
    /// # Errors
    /// Rejects a zero version or charge, empty or inverted bounds, and a maximum charge
    /// `C + collateral_max` that cannot be represented.
    pub fn new(
        version: PolicyVersion,
        unit: SettlementUnit,
        processing_charge: Money,
        collateral_min: Money,
        collateral_max: Money,
    ) -> Result<Self, crate::ProtocolError> {
        if version.0 == 0
            || processing_charge.is_zero()
            || collateral_min.is_zero()
            || collateral_min > collateral_max
            || processing_charge.checked_add(collateral_max).is_none()
        {
            return Err(crate::ProtocolError::PolicyInvalid);
        }
        Ok(Self {
            version,
            unit,
            processing_charge,
            collateral_min,
            collateral_max,
        })
    }
    pub const fn version(&self) -> PolicyVersion {
        self.version
    }
    pub const fn unit(&self) -> SettlementUnit {
        self.unit
    }
    pub const fn processing_charge(&self) -> Money {
        self.processing_charge
    }
    pub const fn collateral_min(&self) -> Money {
        self.collateral_min
    }
    pub const fn collateral_max(&self) -> Money {
        self.collateral_max
    }
    /// Whether one class's collateral lies within the current bounds.
    pub fn quotable(&self, class: &RequestClass) -> bool {
        self.collateral_min <= class.collateral && class.collateral <= self.collateral_max
    }
    /// Whether every class of a publication lies within the current bounds.
    pub fn permits(&self, classes: &RecipientRequestClasses) -> bool {
        classes.classes.iter().all(|c| self.quotable(c))
    }
    /// What a sender may choose from now: `C` and the currently quotable classes.
    pub fn sender_offer(&self, classes: &RecipientRequestClasses) -> SenderOffer {
        SenderOffer {
            policy_version: self.version,
            processing_charge: self.processing_charge,
            classes: classes
                .classes
                .iter()
                .filter(|c| self.quotable(c))
                .cloned()
                .collect(),
        }
    }
    /// The recipient's own view: every published class and whether it is quotable now.
    pub fn class_status(&self, classes: &RecipientRequestClasses) -> Vec<(RequestClass, bool)> {
        classes
            .classes
            .iter()
            .map(|c| (c.clone(), self.quotable(c)))
            .collect()
    }
    /// Prices one class of a recipient's current publication.
    /// # Errors
    /// `ClassUnknown` when the publication has no such class, and `ClassOutsidePolicy`
    /// when its collateral lies outside the current bounds.
    pub fn quote(
        &self,
        classes: &RecipientRequestClasses,
        id: RequestClassId,
    ) -> Result<RequestPrice, PricingRefusal> {
        let class = classes
            .classes
            .iter()
            .find(|c| c.id() == id)
            .ok_or(PricingRefusal::ClassUnknown)?;
        if !self.quotable(class) {
            return Err(PricingRefusal::ClassOutsidePolicy);
        }
        Ok(RequestPrice {
            policy_version: self.version,
            unit: self.unit,
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

/// Why a requested class cannot be priced.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PricingRefusal {
    /// No operator pricing policy has been published.
    NoPolicy,
    /// The recipient has not published the requested class.
    ClassUnknown,
    /// The class's collateral lies outside the current bounds; it cannot be quoted until
    /// the recipient republishes.
    ClassOutsidePolicy,
}
impl From<PricingRefusal> for crate::ProtocolError {
    fn from(refusal: PricingRefusal) -> Self {
        match refusal {
            PricingRefusal::NoPolicy => Self::PolicyInvalid,
            PricingRefusal::ClassUnknown => Self::RequestClassUnknown,
            PricingRefusal::ClassOutsidePolicy => Self::RequestClassOutsidePolicy,
        }
    }
}

/// The host's pricing input to one transition. A refusal is surfaced only when the
/// request actually needs a charge; an accepted relationship or a live lane needs none.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum QuotePricing {
    /// The command does not request terms.
    NotRequested,
    /// The requested class resolved to a price under the current policy.
    Resolved(RequestPrice),
    /// The requested class cannot be priced.
    Refused(PricingRefusal),
}
impl QuotePricing {
    /// The single rule for pricing a requested class from the current operator policy and
    /// the recipient's current publication, used by every storage adapter.
    pub fn resolve(
        policy: Option<&RequestPricingPolicy>,
        classes: Option<&RecipientRequestClasses>,
        class: RequestClassId,
    ) -> Self {
        let Some(policy) = policy else {
            return Self::Refused(PricingRefusal::NoPolicy);
        };
        let Some(classes) = classes else {
            return Self::Refused(PricingRefusal::ClassUnknown);
        };
        match policy.quote(classes, class) {
            Ok(price) => Self::Resolved(price),
            Err(refusal) => Self::Refused(refusal),
        }
    }
}

/// The classes a sender can currently choose, with the processing component they add.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderOffer {
    pub policy_version: PolicyVersion,
    pub processing_charge: Money,
    pub classes: Vec<RequestClass>,
}

/// The price of one request class under one policy version, fixed when terms are issued.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PriceInput")]
pub struct RequestPrice {
    policy_version: PolicyVersion,
    unit: SettlementUnit,
    processing_charge: Money,
    collateral: Money,
    selected: SelectedRequestClass,
}
#[derive(Deserialize)]
struct PriceInput {
    policy_version: PolicyVersion,
    unit: SettlementUnit,
    processing_charge: Money,
    collateral: Money,
    selected: SelectedRequestClass,
}
impl TryFrom<PriceInput> for RequestPrice {
    type Error = String;
    fn try_from(v: PriceInput) -> Result<Self, Self::Error> {
        if v.policy_version.0 == 0
            || v.processing_charge.is_zero()
            || v.collateral.is_zero()
            || v.processing_charge.checked_add(v.collateral).is_none()
        {
            return Err("invalid request price".into());
        }
        Ok(Self {
            policy_version: v.policy_version,
            unit: v.unit,
            processing_charge: v.processing_charge,
            collateral: v.collateral,
            selected: v.selected,
        })
    }
}
impl RequestPrice {
    pub const fn policy_version(&self) -> PolicyVersion {
        self.policy_version
    }
    pub const fn unit(&self) -> SettlementUnit {
        self.unit
    }
    pub const fn processing_charge(&self) -> Money {
        self.processing_charge
    }
    pub const fn collateral(&self) -> Money {
        self.collateral
    }
    pub const fn selected(&self) -> &SelectedRequestClass {
        &self.selected
    }
}
