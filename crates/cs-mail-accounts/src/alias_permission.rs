//! Receiving permission has its own revision and lifecycle, independent of its address.
use crate::aliases::{AdmittedConversation, AliasError};
use cs_mail_capabilities::{SenderScopedLane, routes::AliasScopedGrant};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{AliasId, CanonicalTime, ProtocolIdentity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Permission revisions cannot be substituted for address lifecycle revisions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PermissionRevision(u64);
impl From<u64> for PermissionRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
impl PermissionRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
    pub fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum PermissionState {
    None,
    Broad(AliasScopedGrant),
    Restricted {
        grant: AliasScopedGrant,
        conversations: BTreeMap<ConversationId, AdmittedConversation>,
    },
    Senders(Vec<SenderScopedLane>),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PermissionRecord", into = "PermissionRecord")]
pub struct ReceivingPermission(PermissionRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PermissionRecord {
    alias: AliasId,
    owner: ProtocolIdentity,
    revision: PermissionRevision,
    changed_at: CanonicalTime,
    state: PermissionState,
}
impl TryFrom<PermissionRecord> for ReceivingPermission {
    type Error = AliasError;
    fn try_from(r: PermissionRecord) -> Result<Self, AliasError> {
        if r.owner.0 == 0 || (r.revision.value() == 0 && r.state != PermissionState::None) {
            return Err(AliasError::Invalid);
        }
        let grant = match &r.state {
            PermissionState::Broad(g) | PermissionState::Restricted { grant: g, .. } => Some(g),
            _ => None,
        };
        if grant.is_some_and(|g| {
            g.alias() != r.alias || g.recipient() != r.owner || g.established_at() > r.changed_at
        }) {
            return Err(AliasError::Invalid);
        }
        match &r.state {
            PermissionState::Restricted { conversations, .. }
                if conversations.iter().any(|(id, c)| {
                    *id != c.conversation()
                        || c.alias() != r.alias
                        || c.sender() == r.owner
                        || c.admitted_at() > r.changed_at
                }) =>
            {
                return Err(AliasError::Invalid);
            }
            PermissionState::Senders(lanes) => {
                let ids: std::collections::BTreeSet<_> =
                    lanes.iter().map(|l| l.grant().id).collect();
                if lanes.is_empty()
                    || lanes.len() > 32
                    || ids.len() != lanes.len()
                    || lanes.iter().enumerate().any(|(i, l)| {
                        lanes[..i]
                            .iter()
                            .any(|prior| prior.grant().subject == l.grant().subject)
                    })
                    || lanes.iter().any(|l| {
                        l.grant().recipient != r.owner || l.grant().issued_at > r.changed_at
                    })
                {
                    return Err(AliasError::Invalid);
                }
            }
            _ => (),
        }
        Ok(Self(r))
    }
}
impl From<ReceivingPermission> for PermissionRecord {
    fn from(p: ReceivingPermission) -> Self {
        p.0
    }
}
impl ReceivingPermission {
    /// # Errors
    /// Rejects an absent controlling participant.
    pub fn unpaired(
        alias: AliasId,
        owner: ProtocolIdentity,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        PermissionRecord {
            alias,
            owner,
            revision: PermissionRevision(0),
            changed_at: at,
            state: PermissionState::None,
        }
        .try_into()
    }
    pub const fn alias(&self) -> AliasId {
        self.0.alias
    }
    pub const fn owner(&self) -> ProtocolIdentity {
        self.0.owner
    }
    pub const fn revision(&self) -> PermissionRevision {
        self.0.revision
    }
    pub const fn changed_at(&self) -> CanonicalTime {
        self.0.changed_at
    }
    pub const fn state(&self) -> &PermissionState {
        &self.0.state
    }
    fn change(
        &self,
        expected: PermissionRevision,
        state: PermissionState,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        if self.revision() != expected {
            return Err(AliasError::Conflict);
        }
        if at < self.changed_at() {
            return Err(AliasError::Invalid);
        }
        PermissionRecord {
            state,
            revision: expected.checked_next().ok_or(AliasError::Invalid)?,
            changed_at: at,
            ..self.0.clone()
        }
        .try_into()
    }
    /// # Errors
    /// Rejects stale permission revisions, backward time and grant construction failure.
    pub fn open(
        &self,
        expected: PermissionRevision,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        self.change(
            expected,
            PermissionState::Broad(
                AliasScopedGrant::establish(self.alias(), self.owner(), at)
                    .map_err(|_| AliasError::Invalid)?,
            ),
            at,
        )
    }
    /// # Errors
    /// Rejects stale permission revisions and backward time.
    pub fn remove(
        &self,
        expected: PermissionRevision,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        self.change(expected, PermissionState::None, at)
    }
    /// # Errors
    /// Rejects stale revisions, a non-broad lane or invalid admitted-conversation evidence.
    pub fn restrict(
        &self,
        expected: PermissionRevision,
        existing: Vec<AdmittedConversation>,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        let PermissionState::Broad(grant) = self.state() else {
            return Err(AliasError::Restricted);
        };
        let count = existing.len();
        let conversations: BTreeMap<_, _> = existing
            .into_iter()
            .map(|c| (c.conversation(), c))
            .collect();
        if conversations.len() != count {
            return Err(AliasError::Invalid);
        }
        self.change(
            expected,
            PermissionState::Restricted {
                grant: grant.clone(),
                conversations,
            },
            at,
        )
    }
    /// # Errors
    /// Rejects stale revisions, backward time or an empty/invalid sender grant set.
    pub fn senders(
        &self,
        expected: PermissionRevision,
        lanes: Vec<SenderScopedLane>,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        self.change(expected, PermissionState::Senders(lanes), at)
    }
    pub fn authorizes_alias(
        &self,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        at: CanonicalTime,
    ) -> bool {
        if at < self.changed_at() {
            return false;
        }
        match self.state() {
            PermissionState::Broad(_) => true,
            PermissionState::Restricted { conversations, .. } => conversations
                .get(&conversation)
                .is_some_and(|c| c.sender() == sender),
            _ => false,
        }
    }
    /// Update an already authorized sender lane's allowance without changing its grant revision.
    /// # Errors
    /// Rejects substituted grants and non-successor allowance versions.
    pub fn consume(&self, lane: &SenderScopedLane) -> Result<Self, AliasError> {
        let PermissionState::Senders(lanes) = self.state() else {
            return Err(AliasError::Permission);
        };
        let mut next = lanes.clone();
        let current = next
            .iter_mut()
            .find(|l| l.grant().id == lane.grant().id)
            .ok_or(AliasError::Permission)?;
        if current.grant() != lane.grant()
            || current.version().checked_next() != Some(lane.version())
        {
            return Err(AliasError::Conflict);
        }
        *current = lane.clone();
        PermissionRecord {
            state: PermissionState::Senders(next),
            ..self.0.clone()
        }
        .try_into()
    }
}
