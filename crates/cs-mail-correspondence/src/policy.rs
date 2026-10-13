use crate::{ConversationId, CorrespondenceError};
use cs_mail_primitives::{MessageId, ProtocolIdentity, ReplyEpoch};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An unordered persona pair, not a contact grant or a shared person identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "[ProtocolIdentity; 2]", into = "[ProtocolIdentity; 2]")]
pub struct CorrespondenceRef([ProtocolIdentity; 2]);
impl CorrespondenceRef {
    /// # Errors
    /// Requires two distinct nonzero personas.
    pub fn new(a: ProtocolIdentity, b: ProtocolIdentity) -> Result<Self, CorrespondenceError> {
        if a.0 == 0 || b.0 == 0 || a == b {
            return Err(CorrespondenceError::Invalid);
        }
        let mut pair = [a, b];
        pair.sort();
        Ok(Self(pair))
    }
    pub const fn participants(self) -> [ProtocolIdentity; 2] {
        self.0
    }
    pub fn contains(self, actor: ProtocolIdentity) -> bool {
        self.0.contains(&actor)
    }
    /// # Errors
    /// Rejects a nonparticipant without disclosing another participant.
    pub fn other(self, actor: ProtocolIdentity) -> Result<ProtocolIdentity, CorrespondenceError> {
        if actor == self.0[0] {
            Ok(self.0[1])
        } else if actor == self.0[1] {
            Ok(self.0[0])
        } else {
            Err(CorrespondenceError::Unauthorized)
        }
    }
}
impl TryFrom<[ProtocolIdentity; 2]> for CorrespondenceRef {
    type Error = CorrespondenceError;
    fn try_from(v: [ProtocolIdentity; 2]) -> Result<Self, Self::Error> {
        Self::new(v[0], v[1])
    }
}
impl From<CorrespondenceRef> for [ProtocolIdentity; 2] {
    fn from(v: CorrespondenceRef) -> Self {
        v.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReusePolicy {
    AcrossRelationships,
    OriginatingRelationshipOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PolicyAction {
    Restrict,
    ProposeRelaxation,
    ApproveRelaxation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ConversationRecord", into = "ConversationRecord")]
pub struct Conversation(ConversationRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationRecord {
    id: ConversationId,
    scope: CorrespondenceRef,
    policy: ReusePolicy,
    revision: u64,
    relaxation_proposer: Option<ProtocolIdentity>,
    received: BTreeMap<ProtocolIdentity, ReceivedMessage>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceivedMessage {
    message: MessageId,
    epoch: ReplyEpoch,
}
impl TryFrom<ConversationRecord> for Conversation {
    type Error = CorrespondenceError;
    fn try_from(v: ConversationRecord) -> Result<Self, Self::Error> {
        if v.received
            .iter()
            .any(|(actor, receipt)| !v.scope.contains(*actor) || receipt.message.0 == 0)
        {
            return Err(CorrespondenceError::Invalid);
        }
        if v.relaxation_proposer.is_some_and(|p| {
            !v.scope.contains(p)
                || v.policy != ReusePolicy::OriginatingRelationshipOnly
                || v.revision == 0
        }) {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<Conversation> for ConversationRecord {
    fn from(v: Conversation) -> Self {
        v.0
    }
}
impl Conversation {
    pub fn begun_by_request(id: ConversationId, scope: CorrespondenceRef) -> Self {
        Self(ConversationRecord {
            id,
            scope,
            policy: ReusePolicy::OriginatingRelationshipOnly,
            revision: 0,
            relaxation_proposer: None,
            received: BTreeMap::new(),
        })
    }
    pub fn new(id: ConversationId, scope: CorrespondenceRef) -> Self {
        Self(ConversationRecord {
            id,
            scope,
            policy: ReusePolicy::AcrossRelationships,
            revision: 0,
            relaxation_proposer: None,
            received: BTreeMap::new(),
        })
    }
    pub const fn id(&self) -> ConversationId {
        self.0.id
    }
    pub const fn scope(&self) -> CorrespondenceRef {
        self.0.scope
    }
    pub const fn policy(&self) -> ReusePolicy {
        self.0.policy
    }
    pub const fn revision(&self) -> u64 {
        self.0.revision
    }
    pub const fn relaxation_proposer(&self) -> Option<ProtocolIdentity> {
        self.0.relaxation_proposer
    }
    /// Reply evidence applies only in this conversation and only in the current withdrawal epoch.
    pub fn reply_source(&self, actor: ProtocolIdentity, epoch: ReplyEpoch) -> Option<MessageId> {
        self.0
            .received
            .get(&actor)
            .filter(|receipt| receipt.epoch == epoch)
            .map(|receipt| receipt.message)
    }
    /// # Errors
    /// Rejects a foreign recipient or an empty message identity. Publication commits this with delivery.
    pub fn receive(
        &self,
        recipient: ProtocolIdentity,
        message: MessageId,
        epoch: ReplyEpoch,
    ) -> Result<Self, CorrespondenceError> {
        if !self.scope().contains(recipient) || message.0 == 0 {
            return Err(CorrespondenceError::Invalid);
        }
        let mut next = self.clone();
        next.0
            .received
            .insert(recipient, ReceivedMessage { message, epoch });
        Ok(next)
    }
    pub fn permits(&self, destination: CorrespondenceRef) -> bool {
        self.policy() == ReusePolicy::AcrossRelationships || self.scope() == destination
    }
    /// # Errors
    /// Rejects foreign actors, stale proposals, self-approval and revision overflow.
    pub fn change(
        &self,
        actor: ProtocolIdentity,
        revision: u64,
        action: PolicyAction,
    ) -> Result<Self, CorrespondenceError> {
        if !self.scope().contains(actor) {
            return Err(CorrespondenceError::Unauthorized);
        }
        if revision != self.revision() {
            return Err(CorrespondenceError::Conflict);
        }
        let mut next = self.clone();
        match action {
            PolicyAction::Restrict => {
                next.0.policy = ReusePolicy::OriginatingRelationshipOnly;
                next.0.relaxation_proposer = None;
            }
            PolicyAction::ProposeRelaxation
                if self.policy() == ReusePolicy::OriginatingRelationshipOnly
                    && self.0.relaxation_proposer.is_none() =>
            {
                next.0.relaxation_proposer = Some(actor);
            }
            PolicyAction::ApproveRelaxation
                if self.0.relaxation_proposer.is_some_and(|p| p != actor) =>
            {
                next.0.policy = ReusePolicy::AcrossRelationships;
                next.0.relaxation_proposer = None;
            }
            _ => return Err(CorrespondenceError::Conflict),
        }
        next.0.revision = revision
            .checked_add(1)
            .ok_or(CorrespondenceError::Invalid)?;
        Ok(next)
    }
}
