//! Receiving-route authority. No account, payment, clock or delivery decisions.
use crate::{CapabilityError, SenderScopedLane};
use cs_mail_primitives::{AliasId, CanonicalTime, ProtocolIdentity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Explicit selected entrances. Creating another alias never expands this scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "BTreeSet<AliasId>", into = "BTreeSet<AliasId>")]
pub struct ReceivingScope(BTreeSet<AliasId>);
impl ReceivingScope {
    pub fn one(alias: AliasId) -> Self {
        Self([alias].into())
    }
    /// # Errors
    /// An empty grant has no receiving authority.
    pub fn selected(aliases: BTreeSet<AliasId>) -> Result<Self, CapabilityError> {
        if aliases.is_empty() {
            return Err(CapabilityError::InvalidGrant);
        }
        Ok(Self(aliases))
    }
    pub fn contains(&self, alias: AliasId) -> bool {
        self.0.contains(&alias)
    }
    pub fn aliases(&self) -> &BTreeSet<AliasId> {
        &self.0
    }
}
impl TryFrom<BTreeSet<AliasId>> for ReceivingScope {
    type Error = CapabilityError;
    fn try_from(value: BTreeSet<AliasId>) -> Result<Self, Self::Error> {
        Self::selected(value)
    }
}
impl From<ReceivingScope> for BTreeSet<AliasId> {
    fn from(value: ReceivingScope) -> Self {
        value.0
    }
}

/// An explicit recipient grant establishes this receiving authority, without a
/// sender, hard expiry, periodic renewal or request-settlement consequence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "AliasGrantRecord", into = "AliasGrantRecord")]
pub struct AliasScopedGrant(AliasGrantRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct AliasGrantRecord {
    alias: AliasId,
    recipient: ProtocolIdentity,
    established_at: CanonicalTime,
}
impl AliasScopedGrant {
    /// # Errors
    /// Rejects an absent controlling participant.
    pub fn establish(
        alias: AliasId,
        recipient: ProtocolIdentity,
        at: CanonicalTime,
    ) -> Result<Self, CapabilityError> {
        AliasGrantRecord {
            alias,
            recipient,
            established_at: at,
        }
        .try_into()
    }
    pub const fn alias(&self) -> AliasId {
        self.0.alias
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.0.recipient
    }
    pub const fn established_at(&self) -> CanonicalTime {
        self.0.established_at
    }
}
impl TryFrom<AliasGrantRecord> for AliasScopedGrant {
    type Error = CapabilityError;
    fn try_from(value: AliasGrantRecord) -> Result<Self, Self::Error> {
        if value.recipient.0 == 0 {
            return Err(CapabilityError::InvalidGrant);
        }
        Ok(Self(value))
    }
}
impl From<AliasScopedGrant> for AliasGrantRecord {
    fn from(value: AliasScopedGrant) -> Self {
        value.0
    }
}

/// Sender authority and the explicitly authorized receiving entrances. The
/// application must authenticate a recipient's signature over BOTH parts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SenderScopedPermission {
    lane: SenderScopedLane,
    receiving: ReceivingScope,
}
impl SenderScopedPermission {
    pub fn new(lane: SenderScopedLane, receiving: ReceivingScope) -> Self {
        Self { lane, receiving }
    }
    pub const fn lane(&self) -> &SenderScopedLane {
        &self.lane
    }
    pub const fn receiving(&self) -> &ReceivingScope {
        &self.receiving
    }
    pub fn into_parts(self) -> (SenderScopedLane, ReceivingScope) {
        (self.lane, self.receiving)
    }
}

/// Shared inspection of the two distinct authorities, without shared lifecycle
/// transitions. Alias permission cannot be passed to sender-lane settlement.
#[derive(Clone, Copy, Debug)]
pub enum ExpressLanePermission<'a> {
    Sender(&'a SenderScopedPermission),
    Alias(&'a AliasScopedGrant),
}
