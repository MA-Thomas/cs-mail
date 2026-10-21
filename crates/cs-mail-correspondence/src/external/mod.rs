//! Member-initiated external conversations. Pending access is conversation scoped;
//! standing permission and lanes remain owned by the relationship protocol.
use crate::{Conversation, ConversationId, CorrespondenceError, CorrespondenceRef};
use cs_mail_consent::disclosure::DisclosureConsent;
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{CanonicalTime, ProtocolIdentity, RelationshipVersion};
use cs_mail_protocol::{Relationship, RelationshipState};
use serde::{Deserialize, Serialize};

pub mod delivery;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Invitation {
    /// Existing standing or lane permission governs replies from the outset.
    NotRequired,
    Pending {
        relationship_version: RelationshipVersion,
    },
    Rejected {
        at: CanonicalTime,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Record", into = "Record")]
pub struct ExternalConversation(Record);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    conversation: Conversation,
    member: ProtocolIdentity,
    receiving_alias: cs_mail_primitives::AliasId,
    mailbox: GuestMailbox,
    deployment: [u8; 32],
    invitation: Invitation,
    disclosure: DisclosureConsent,
    created_at: CanonicalTime,
}
impl TryFrom<Record> for ExternalConversation {
    type Error = CorrespondenceError;
    fn try_from(r: Record) -> Result<Self, Self::Error> {
        if r.conversation.scope()
            != CorrespondenceRef::new(
                r.member,
                r.mailbox.synthetic_protocol_identity(&r.deployment),
            )?
            || matches!(r.invitation, Invitation::Rejected { at } if at < r.created_at)
            || matches!(r.disclosure, DisclosureConsent::Granted { at } | DisclosureConsent::Withdrawn { at } if at < r.created_at)
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(r))
    }
}
impl From<ExternalConversation> for Record {
    fn from(c: ExternalConversation) -> Self {
        c.0
    }
}
impl ExternalConversation {
    /// An inbound route records a conversation without inventing an invitation or
    /// outbound consent. Admission still requires current relationship permission.
    /// # Errors
    /// Rejects a route for a different relationship.
    pub fn inbound(
        id: ConversationId,
        member: ProtocolIdentity,
        receiving_alias: cs_mail_primitives::AliasId,
        mailbox: GuestMailbox,
        deployment: [u8; 32],
        relationship: &Relationship,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        if relationship.key.sender != mailbox.synthetic_protocol_identity(&deployment)
            || relationship.key.recipient != member
        {
            return Err(CorrespondenceError::Unauthorized);
        }
        Record {
            conversation: Conversation::new(
                id,
                CorrespondenceRef::new(member, relationship.key.sender)?,
            ),
            member,
            receiving_alias,
            mailbox,
            deployment,
            invitation: Invitation::NotRequired,
            disclosure: DisclosureConsent::NotGranted,
            created_at: now,
        }
        .try_into()
    }
    /// # Errors
    /// Only the member can grant disclosure for the fixed destination.
    pub fn grant_disclosure(
        &self,
        actor: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        if actor != self.member() || now < self.0.created_at {
            return Err(CorrespondenceError::Unauthorized);
        }
        let mut next = self.clone();
        next.0.disclosure = next
            .0
            .disclosure
            .grant(now)
            .map_err(|_| CorrespondenceError::Invalid)?;
        Ok(next)
    }
    /// The member explicitly supplies disclosure consent in the signed start command.
    /// # Errors
    /// Rejects a different relationship or blocked guest.
    pub fn start(
        id: ConversationId,
        member: ProtocolIdentity,
        receiving_alias: cs_mail_primitives::AliasId,
        mailbox: GuestMailbox,
        deployment: [u8; 32],
        relationship: &Relationship,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        if relationship.key.sender != mailbox.synthetic_protocol_identity(&deployment)
            || relationship.key.recipient != member
            || relationship.state == RelationshipState::Blocked
        {
            return Err(CorrespondenceError::Unauthorized);
        }
        Record {
            conversation: Conversation::new(
                id,
                CorrespondenceRef::new(member, mailbox.synthetic_protocol_identity(&deployment))?,
            ),
            member,
            receiving_alias,
            mailbox,
            deployment,
            invitation: match relationship.state {
                RelationshipState::Accepted | RelationshipState::ExpressLane(_) => {
                    Invitation::NotRequired
                }
                _ => Invitation::Pending {
                    relationship_version: relationship.version,
                },
            },
            disclosure: DisclosureConsent::Granted { at: now },
            created_at: now,
        }
        .try_into()
    }
    pub fn guest(&self) -> ProtocolIdentity {
        self.mailbox()
            .synthetic_protocol_identity(&self.0.deployment)
    }
    pub const fn receiving_alias(&self) -> cs_mail_primitives::AliasId {
        self.0.receiving_alias
    }
    pub const fn member(&self) -> ProtocolIdentity {
        self.0.member
    }
    pub const fn id(&self) -> ConversationId {
        self.0.conversation.id()
    }
    pub fn mailbox(&self) -> &GuestMailbox {
        &self.0.mailbox
    }
    pub const fn invitation(&self) -> Invitation {
        self.0.invitation
    }
    pub const fn disclosure(&self) -> DisclosureConsent {
        self.0.disclosure
    }
    /// Any relationship decision invalidates the old pending opportunity. Later
    /// acceptance/revocation cannot accidentally resurrect it.
    pub fn pending_access(&self, relationship: &Relationship) -> bool {
        relationship.key.sender == self.guest()
            && relationship.key.recipient == self.member()
            && relationship.state != RelationshipState::Blocked
            && matches!(self.invitation(), Invitation::Pending { relationship_version } if relationship_version == relationship.version)
    }
    /// # Errors
    /// Rejects a nonowner or a decision predating the invitation.
    pub fn reject(
        &self,
        actor: ProtocolIdentity,
        relationship: &Relationship,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        if actor != self.member()
            || now < self.0.created_at
            || !self.pending_access(relationship)
            || relationship.state == RelationshipState::Accepted
        {
            return Err(CorrespondenceError::Unauthorized);
        }
        let mut next = self.clone();
        next.0.invitation = Invitation::Rejected { at: now };
        Ok(next)
    }
    /// # Errors
    /// Only the member can withdraw this route's disclosure authority.
    pub fn withdraw(
        &self,
        actor: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        if actor != self.member() {
            return Err(CorrespondenceError::Unauthorized);
        }
        let mut next = self.clone();
        next.0.disclosure = next
            .0
            .disclosure
            .withdraw(now)
            .map_err(|_| CorrespondenceError::Invalid)?;
        Ok(next)
    }
}
