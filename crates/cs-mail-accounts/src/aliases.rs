//! Participant-owned entrances. Alias lifecycle never mutates participant identity,
//! sender blocks, delivered correspondence, consent or financial obligations.
use crate::alias_permission::ReceivingPermission;
use crate::directory::MemberAddress;
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{AliasId, CanonicalTime, ProtocolIdentity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AliasError {
    Invalid,
    Conflict,
    AddressTaken,
    Retired,
    Restricted,
    Blocked,
    Coverage,
    Authentication,
    Permission,
}
impl std::fmt::Display for AliasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "alias: {self:?}")
    }
}
impl std::error::Error for AliasError {}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReceivingPolicy {
    Bonded,
    AliasScoped,
}

/// Private context metadata. It is never an address, routing evidence or publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct AliasDescription(String);
impl TryFrom<String> for AliasDescription {
    type Error = AliasError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty()
            || value.chars().count() > 240
            || value.chars().any(char::is_control)
        {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value.trim().to_owned()))
    }
}
impl From<AliasDescription> for String {
    fn from(value: AliasDescription) -> Self {
        value.0
    }
}
impl AliasDescription {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// Purpose and receiving policy are independent of address choice and publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AliasConfiguration {
    pub description: AliasDescription,
    pub receiving: ReceivingPolicy,
}
/// A complete protected owner snapshot owns the ownership invariant.
pub struct AliasPortfolio<'a> {
    aliases: &'a BTreeMap<AliasId, Alias>,
    owner: ProtocolIdentity,
}
impl<'a> AliasPortfolio<'a> {
    /// # Errors
    /// Refuses mixed ownership, substituted identifiers.
    pub fn new(
        owner: ProtocolIdentity,
        aliases: &'a BTreeMap<AliasId, Alias>,
    ) -> Result<Self, AliasError> {
        if owner.0 == 0
            || aliases
                .iter()
                .any(|(id, a)| *id != a.id() || a.owner() != owner)
        {
            return Err(AliasError::Invalid);
        }
        Ok(Self { aliases, owner })
    }
    /// # Errors
    /// Rejects a reused alias identity in this protected owner portfolio.
    pub fn create(
        &self,
        id: AliasId,
        address: MemberAddress,
        configuration: AliasConfiguration,
        at: CanonicalTime,
    ) -> Result<Alias, AliasError> {
        if self.aliases.contains_key(&id) {
            return Err(AliasError::Conflict);
        }
        Alias::create(id, self.owner, address, configuration, at)
    }
}

/// The application supplies only already admitted conversations on THIS route.
/// A held message, failed attempt or prepared paid request is not such evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ConversationRecord", into = "ConversationRecord")]
pub struct AdmittedConversation(ConversationRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationRecord {
    alias: AliasId,
    conversation: ConversationId,
    sender: ProtocolIdentity,
    first_admitted_at: CanonicalTime,
}
impl AdmittedConversation {
    /// # Errors
    /// Rejects an absent authenticated sender.
    pub fn new(
        alias: AliasId,
        conversation: ConversationId,
        sender: ProtocolIdentity,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        ConversationRecord {
            alias,
            conversation,
            sender,
            first_admitted_at: at,
        }
        .try_into()
    }
    pub const fn alias(&self) -> AliasId {
        self.0.alias
    }
    pub const fn conversation(&self) -> ConversationId {
        self.0.conversation
    }
    pub const fn sender(&self) -> ProtocolIdentity {
        self.0.sender
    }
    pub const fn admitted_at(&self) -> CanonicalTime {
        self.0.first_admitted_at
    }
}
impl TryFrom<ConversationRecord> for AdmittedConversation {
    type Error = AliasError;
    fn try_from(value: ConversationRecord) -> Result<Self, Self::Error> {
        if value.sender.0 == 0 {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<AdmittedConversation> for ConversationRecord {
    fn from(value: AdmittedConversation) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ApprovalRecord", into = "ApprovalRecord")]
pub struct ConversationApproval(ApprovalRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ApprovalRecord {
    sender: ProtocolIdentity,
    at: CanonicalTime,
}
impl TryFrom<ApprovalRecord> for ConversationApproval {
    type Error = AliasError;
    fn try_from(value: ApprovalRecord) -> Result<Self, Self::Error> {
        if value.sender.0 == 0 {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<ConversationApproval> for ApprovalRecord {
    fn from(value: ConversationApproval) -> Self {
        value.0
    }
}
impl ConversationApproval {
    pub const fn sender(&self) -> ProtocolIdentity {
        self.0.sender
    }
    pub const fn at(&self) -> CanonicalTime {
        self.0.at
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum AliasLifecycle {
    Active,
    Restricted {
        at: CanonicalTime,
        conversations: BTreeMap<ConversationId, AdmittedConversation>,
        approvals: BTreeMap<ConversationId, ConversationApproval>,
    },
    Replaced {
        at: CanonicalTime,
        replacement: AliasId,
        until: CanonicalTime,
        conversations: BTreeMap<ConversationId, AdmittedConversation>,
        approvals: BTreeMap<ConversationId, ConversationApproval>,
    },
    Retired {
        at: CanonicalTime,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "AliasRecord", into = "AliasRecord")]
pub struct Alias(AliasRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct AliasRecord {
    id: AliasId,
    owner: ProtocolIdentity,
    address: MemberAddress,
    configuration: AliasConfiguration,
    created_at: CanonicalTime,
    changed_at: CanonicalTime,
    revision: u64,
    lifecycle: AliasLifecycle,
    permission: ReceivingPermission,
}
impl TryFrom<AliasRecord> for Alias {
    type Error = AliasError;
    fn try_from(value: AliasRecord) -> Result<Self, Self::Error> {
        if value.owner.0 == 0
            || value.changed_at < value.created_at
            || value.permission.alias() != value.id
            || value.permission.owner() != value.owner
            || value.permission.changed_at() < value.created_at
            || (value.configuration.receiving == ReceivingPolicy::Bonded
                && matches!(
                    value.permission.state(),
                    crate::alias_permission::PermissionState::Broad(_)
                        | crate::alias_permission::PermissionState::Restricted { .. }
                ))
        {
            return Err(AliasError::Invalid);
        }
        let (at, conversations) = match &value.lifecycle {
            AliasLifecycle::Active => (value.created_at, None),
            AliasLifecycle::Restricted {
                at, conversations, ..
            } => (*at, Some(conversations)),
            AliasLifecycle::Replaced {
                at,
                replacement,
                until,
                conversations,
                ..
            } => {
                if *replacement == value.id || *until <= *at {
                    return Err(AliasError::Invalid);
                }
                (*at, Some(conversations))
            }
            AliasLifecycle::Retired { at } => (*at, None),
        };
        if at < value.created_at
            || at > value.changed_at
            || (matches!(value.lifecycle, AliasLifecycle::Active)
                && (value.revision != 0 || value.changed_at != value.created_at))
            || (!matches!(value.lifecycle, AliasLifecycle::Active) && value.revision == 0)
            || conversations.is_some_and(|conversations| {
                conversations.iter().any(|(id, c)| {
                    *id != c.conversation()
                        || c.alias() != value.id
                        || c.sender() == value.owner
                        || c.admitted_at() < value.created_at
                        || c.admitted_at() > at
                })
            })
        {
            return Err(AliasError::Invalid);
        }
        match &value.lifecycle {
            AliasLifecycle::Restricted { at, approvals, .. } => {
                let changes = u64::try_from(approvals.len())
                    .map_err(|_| AliasError::Invalid)?
                    .checked_add(1)
                    .ok_or(AliasError::Invalid)?;
                if value.revision != changes
                    || approvals.values().any(|approval| approval.at() < *at)
                    || approvals
                        .values()
                        .map(ConversationApproval::at)
                        .max()
                        .unwrap_or(*at)
                        != value.changed_at
                {
                    return Err(AliasError::Invalid);
                }
            }
            AliasLifecycle::Replaced { at, approvals, .. } => {
                let changes = u64::try_from(approvals.len())
                    .map_err(|_| AliasError::Invalid)?
                    .checked_add(2)
                    .ok_or(AliasError::Invalid)?;
                if value.changed_at != *at
                    || (value.revision != changes && !(approvals.is_empty() && value.revision == 1))
                {
                    return Err(AliasError::Invalid);
                }
            }
            AliasLifecycle::Active | AliasLifecycle::Retired { .. } => {}
        }
        let approvals = match &value.lifecycle {
            AliasLifecycle::Restricted { approvals, .. }
            | AliasLifecycle::Replaced { approvals, .. } => Some(approvals),
            _ => None,
        };
        if approvals.is_some_and(|entries| {
            entries.values().any(|approval| {
                approval.sender().0 == 0
                    || approval.sender() == value.owner
                    || approval.at() < value.created_at
                    || approval.at() > value.changed_at
            })
        }) {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<Alias> for AliasRecord {
    fn from(value: Alias) -> Self {
        value.0
    }
}
impl Alias {
    /// # Errors
    /// Rejects an absent owner. Address creation establishes no receiving permission.
    pub fn create(
        id: AliasId,
        owner: ProtocolIdentity,
        address: MemberAddress,
        configuration: AliasConfiguration,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        AliasRecord {
            id,
            owner,
            address,
            configuration,
            created_at: at,
            changed_at: at,
            revision: 0,
            lifecycle: AliasLifecycle::Active,
            permission: ReceivingPermission::unpaired(id, owner, at)?,
        }
        .try_into()
    }
    pub const fn id(&self) -> AliasId {
        self.0.id
    }
    pub const fn owner(&self) -> ProtocolIdentity {
        self.0.owner
    }
    pub const fn policy(&self) -> ReceivingPolicy {
        self.0.configuration.receiving
    }
    pub const fn configuration(&self) -> &AliasConfiguration {
        &self.0.configuration
    }
    pub const fn description(&self) -> &AliasDescription {
        &self.0.configuration.description
    }
    pub fn is_bonded_route(&self) -> bool {
        self.policy() == ReceivingPolicy::Bonded
            && matches!(
                self.lifecycle(),
                AliasLifecycle::Active | AliasLifecycle::Restricted { .. }
            )
    }
    pub const fn revision(&self) -> u64 {
        self.0.revision
    }
    pub const fn changed_at(&self) -> CanonicalTime {
        self.0.changed_at
    }
    pub const fn created_at(&self) -> CanonicalTime {
        self.0.created_at
    }
    pub const fn lifecycle(&self) -> &AliasLifecycle {
        &self.0.lifecycle
    }
    pub const fn permission(&self) -> &ReceivingPermission {
        &self.0.permission
    }
    /// Permission and address state have separate revisions, checked under the same transaction.
    /// # Errors
    /// Rejects a permission belonging to another address or controlling participant.
    pub fn with_permission(&self, permission: ReceivingPermission) -> Result<Self, AliasError> {
        let mut record = self.0.clone();
        record.permission = permission;
        record.try_into()
    }
    pub const fn address(&self) -> &MemberAddress {
        &self.0.address
    }
    fn update(
        &self,
        expected: u64,
        now: CanonicalTime,
        lifecycle: AliasLifecycle,
    ) -> Result<Self, AliasError> {
        if self.revision() != expected {
            return Err(AliasError::Conflict);
        }
        if matches!(
            self.lifecycle(),
            AliasLifecycle::Retired { .. } | AliasLifecycle::Replaced { .. }
        ) {
            return Err(AliasError::Retired);
        }
        if now < self.changed_at() {
            return Err(AliasError::Invalid);
        }
        let mut record = self.0.clone();
        record.revision = expected.checked_add(1).ok_or(AliasError::Invalid)?;
        record.changed_at = now;
        record.lifecycle = lifecycle;
        record.try_into()
    }
    fn existing(
        &self,
        existing: Vec<AdmittedConversation>,
        at: CanonicalTime,
    ) -> Result<BTreeMap<ConversationId, AdmittedConversation>, AliasError> {
        let count = existing.len();
        let conversations: BTreeMap<_, _> = existing
            .into_iter()
            .map(|c| (c.conversation(), c))
            .collect();
        if count != conversations.len()
            || conversations.values().any(|c| {
                c.alias() != self.id()
                    || c.admitted_at() > at
                    || c.admitted_at() < self.created_at()
                    || c.sender() == self.owner()
            })
        {
            return Err(AliasError::Invalid);
        }
        Ok(conversations)
    }
    /// # Errors
    /// Refuses stale revisions or evidence not admitted on this route before restriction.
    pub fn restrict(
        &self,
        expected: u64,
        existing: Vec<AdmittedConversation>,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        if !matches!(self.lifecycle(), AliasLifecycle::Active) {
            return Err(AliasError::Restricted);
        }
        self.update(
            expected,
            at,
            AliasLifecycle::Restricted {
                at,
                conversations: self.existing(existing, at)?,
                approvals: BTreeMap::new(),
            },
        )
    }
    /// # Errors
    /// Replacement preserves the participant and old evidence; the new route inherits no senders.
    pub fn replace(
        &self,
        expected: u64,
        replacement: &Self,
        until: CanonicalTime,
        existing: Vec<AdmittedConversation>,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        if replacement.owner() != self.owner()
            || replacement.id() == self.id()
            || replacement.address() == self.address()
            || replacement.configuration() != self.configuration()
            || replacement.created_at() != at
            || replacement.revision() != 0
            || !matches!(replacement.lifecycle(), AliasLifecycle::Active)
            || until <= at
        {
            return Err(AliasError::Invalid);
        }
        let approvals = match self.lifecycle() {
            AliasLifecycle::Restricted { approvals, .. } => approvals.clone(),
            _ => BTreeMap::new(),
        };
        let conversations = match self.lifecycle() {
            AliasLifecycle::Restricted { conversations, .. } => conversations.clone(),
            _ => self.existing(existing, at)?,
        };
        self.update(
            expected,
            at,
            AliasLifecycle::Replaced {
                at,
                replacement: replacement.id(),
                until,
                conversations,
                approvals,
            },
        )
    }
    /// # Errors
    /// Refuses stale revisions and backward time. The address stays reserved permanently.
    pub fn retire(&self, expected: u64, at: CanonicalTime) -> Result<Self, AliasError> {
        if matches!(self.lifecycle(), AliasLifecycle::Replaced { .. }) {
            if self.revision() != expected || at < self.changed_at() {
                return Err(AliasError::Conflict);
            }
            let mut r = self.0.clone();
            r.revision = expected.checked_add(1).ok_or(AliasError::Invalid)?;
            r.changed_at = at;
            r.lifecycle = AliasLifecycle::Retired { at };
            return r.try_into();
        }
        self.update(expected, at, AliasLifecycle::Retired { at })
    }
    /// # Errors
    /// One reviewed conversation is authorized explicitly; the admitted-history
    /// snapshot remains immutable and no other conversation gains permission.
    pub fn approve_conversation(
        &self,
        expected: u64,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        at: CanonicalTime,
    ) -> Result<Self, AliasError> {
        let AliasLifecycle::Restricted {
            at: restricted_at,
            conversations,
            approvals,
        } = self.lifecycle()
        else {
            return Err(AliasError::Restricted);
        };
        if sender.0 == 0
            || sender == self.owner()
            || conversations.contains_key(&conversation)
            || approvals.contains_key(&conversation)
        {
            return Err(AliasError::Invalid);
        }
        let mut approvals = approvals.clone();
        approvals.insert(conversation, ApprovalRecord { sender, at }.try_into()?);
        self.update(
            expected,
            at,
            AliasLifecycle::Restricted {
                at: *restricted_at,
                conversations: conversations.clone(),
                approvals,
            },
        )
    }
    /// # Errors
    /// Route status cannot be bypassed by standing or sender-lane permission.
    pub fn check_conversation(
        &self,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        now: CanonicalTime,
    ) -> Result<(), AliasError> {
        if now < self.changed_at() {
            return Err(AliasError::Invalid);
        }
        let (existing, approvals) = match self.lifecycle() {
            AliasLifecycle::Active => return Ok(()),
            AliasLifecycle::Restricted {
                conversations,
                approvals,
                ..
            } => (conversations, approvals),
            AliasLifecycle::Replaced {
                until,
                conversations,
                approvals,
                ..
            } if now < *until => (conversations, approvals),
            AliasLifecycle::Replaced { .. } | AliasLifecycle::Retired { .. } => {
                return Err(AliasError::Retired);
            }
        };
        if existing
            .get(&conversation)
            .is_some_and(|c| c.sender() == sender)
            || approvals
                .get(&conversation)
                .is_some_and(|approval| approval.sender() == sender && approval.at() <= now)
        {
            Ok(())
        } else {
            Err(AliasError::Restricted)
        }
    }
}
