//! Typed address management bridge; Rust application projections own the meanings.
use super::{AppleClient, ClientError, core};
#[derive(uniffi::Enum)]
pub enum AddressPolicy {
    Bonded,
    AliasScoped,
}
#[derive(uniffi::Enum)]
pub enum AddressChoice {
    Generated { address: String },
    Chosen { address: String },
}
#[derive(uniffi::Enum)]
pub enum AddressState {
    Active,
    Restricted,
    Replaced {
        replacement: String,
        until: u64,
        transition_ended: bool,
    },
    Retired,
}
#[derive(uniffi::Record)]
pub struct AddressConversation {
    pub id: String,
    pub sender: String,
    pub approved: bool,
}
#[derive(uniffi::Record)]
pub struct ManagedAddress {
    pub permission: AddressPermission,
    pub permission_revision: u64,
    pub permitted_senders: Vec<String>,
    pub permission_expires_at: Option<u64>,
    pub id: String,
    pub address: String,
    pub description: Option<String>,
    pub revision: u64,
    pub policy: AddressPolicy,
    pub state: AddressState,
    pub can_restrict: bool,
    pub can_replace: bool,
    pub can_retire: bool,
    pub conversations: Vec<AddressConversation>,
}
#[derive(uniffi::Enum)]
pub enum AddressPermission {
    Unpaired,
    Broad,
    Restricted,
    Senders,
}
#[derive(uniffi::Enum)]
pub enum PermissionMutation {
    Open,
    Restrict,
    Remove,
    Senders {
        domains: Vec<String>,
        senders: Vec<String>,
        valid_until: u64,
        max_messages_per_day: u32,
    },
}
impl From<PermissionMutation> for core::PermissionMutation {
    fn from(p: PermissionMutation) -> Self {
        match p {
            PermissionMutation::Open => Self::Open,
            PermissionMutation::Restrict => Self::Restrict,
            PermissionMutation::Remove => Self::Remove,
            PermissionMutation::Senders {
                domains,
                senders,
                valid_until,
                max_messages_per_day,
            } => Self::Senders {
                domains,
                senders,
                valid_until,
                max_messages_per_day,
            },
        }
    }
}
impl From<core::AddressPermission> for AddressPermission {
    fn from(p: core::AddressPermission) -> Self {
        match p {
            core::AddressPermission::Unpaired => Self::Unpaired,
            core::AddressPermission::Broad => Self::Broad,
            core::AddressPermission::Restricted => Self::Restricted,
            core::AddressPermission::Senders => Self::Senders,
        }
    }
}
#[derive(uniffi::Enum)]
pub enum AddressMutation {
    UseForCorrespondence {
        id: String,
        expected: u64,
    },
    Permission {
        id: String,
        revision: u64,
        permission_revision: u64,
        action: PermissionMutation,
    },
    Create {
        description: String,
        address: AddressChoice,
        policy: AddressPolicy,
    },
    Restrict {
        id: String,
        revision: u64,
    },
    Replace {
        id: String,
        revision: u64,
        address: AddressChoice,
        until: u64,
    },
    Retire {
        id: String,
        revision: u64,
    },
}
#[derive(uniffi::Enum)]
pub enum PendingAddressAction {
    Correspondence,
    Create { description: String },
    Restrict { id: String },
    Replace { id: String },
    Retire { id: String },
    ApproveConversation { id: String },
    ScopeSender,
    Permission { id: String },
}
#[derive(uniffi::Record)]
pub struct PendingAddressUpdate {
    pub operation: String,
    pub action: PendingAddressAction,
}
#[derive(uniffi::Enum)]
pub enum AddressUpdateResult {
    AddressUnavailable,
    Applied { created_alias: Option<String> },
    Refused { explanation: String },
}
#[derive(uniffi::Record)]
pub struct ReceivingLaneView {
    pub address: ManagedAddress,
}
#[derive(uniffi::Record)]
pub struct AddressWorkspace {
    pub alias_lanes: Vec<ReceivingLaneView>,
    pub sender_alias_lanes: Vec<ReceivingLaneView>,
    pub addresses: Vec<ManagedAddress>,
    pub observed_at: u64,
}

impl From<core::AddressPolicy> for AddressPolicy {
    fn from(v: core::AddressPolicy) -> Self {
        match v {
            core::AddressPolicy::Bonded => Self::Bonded,
            core::AddressPolicy::AliasScoped => Self::AliasScoped,
        }
    }
}
impl From<AddressPolicy> for core::AddressPolicy {
    fn from(v: AddressPolicy) -> Self {
        match v {
            AddressPolicy::Bonded => Self::Bonded,
            AddressPolicy::AliasScoped => Self::AliasScoped,
        }
    }
}
impl From<AddressChoice> for core::AddressChoice {
    fn from(v: AddressChoice) -> Self {
        match v {
            AddressChoice::Generated { address } => Self::Generated { address },
            AddressChoice::Chosen { address } => Self::Chosen { address },
        }
    }
}
impl From<core::AddressState> for AddressState {
    fn from(v: core::AddressState) -> Self {
        match v {
            core::AddressState::Active => Self::Active,
            core::AddressState::Restricted => Self::Restricted,
            core::AddressState::Replaced {
                replacement,
                until,
                transition_ended,
            } => Self::Replaced {
                replacement,
                until,
                transition_ended,
            },
            core::AddressState::Retired => Self::Retired,
        }
    }
}
impl From<core::ManagedAddress> for ManagedAddress {
    fn from(v: core::ManagedAddress) -> Self {
        Self {
            permission: v.permission.into(),
            permission_revision: v.permission_revision,
            permitted_senders: v.permitted_senders,
            permission_expires_at: v.permission_expires_at,
            id: v.id,
            address: v.address,
            description: v.description,
            revision: v.revision,
            policy: v.policy.into(),
            state: v.state.into(),
            can_restrict: v.can_restrict,
            can_replace: v.can_replace,
            can_retire: v.can_retire,
            conversations: v
                .conversations
                .into_iter()
                .map(|c| AddressConversation {
                    id: c.id,
                    sender: c.sender,
                    approved: c.approved,
                })
                .collect(),
        }
    }
}
impl From<AddressMutation> for core::AddressMutation {
    fn from(v: AddressMutation) -> Self {
        match v {
            AddressMutation::UseForCorrespondence { id, expected } => {
                Self::UseForCorrespondence { id, expected }
            }
            AddressMutation::Permission {
                id,
                revision,
                permission_revision,
                action,
            } => Self::Permission {
                id,
                revision,
                permission_revision,
                action: action.into(),
            },
            AddressMutation::Create {
                description,
                address,
                policy,
            } => Self::Create {
                description,
                address: address.into(),
                policy: policy.into(),
            },
            AddressMutation::Restrict { id, revision } => Self::Restrict { id, revision },
            AddressMutation::Replace {
                id,
                revision,
                address,
                until,
            } => Self::Replace {
                id,
                revision,
                address: address.into(),
                until,
            },
            AddressMutation::Retire { id, revision } => Self::Retire { id, revision },
        }
    }
}
impl From<core::PendingAddressAction> for PendingAddressAction {
    fn from(v: core::PendingAddressAction) -> Self {
        match v {
            core::PendingAddressAction::Correspondence => Self::Correspondence,
            core::PendingAddressAction::Create { description } => Self::Create { description },
            core::PendingAddressAction::Restrict { id } => Self::Restrict { id },
            core::PendingAddressAction::Replace { id } => Self::Replace { id },
            core::PendingAddressAction::Retire { id } => Self::Retire { id },
            core::PendingAddressAction::ApproveConversation { id } => {
                Self::ApproveConversation { id }
            }
            core::PendingAddressAction::ScopeSender => Self::ScopeSender,
            core::PendingAddressAction::Permission { id } => Self::Permission { id },
        }
    }
}
impl From<core::AddressUpdateResult> for AddressUpdateResult {
    fn from(v: core::AddressUpdateResult) -> Self {
        match v {
            core::AddressUpdateResult::AddressUnavailable => Self::AddressUnavailable,
            core::AddressUpdateResult::Applied { created_alias } => Self::Applied { created_alias },
            core::AddressUpdateResult::Refused { explanation } => Self::Refused { explanation },
        }
    }
}
#[derive(uniffi::Record)]
pub struct AliasSuggestion {
    pub address: String,
    pub fragments: Vec<String>,
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Invalid name, unavailable candidates, transport or verification failure.
    pub fn suggest_alias(
        &self,
        name: String,
        fragment: Option<String>,
    ) -> Result<AliasSuggestion, ClientError> {
        let value = self.0.suggest_alias(name, fragment)?;
        Ok(AliasSuggestion {
            address: value.address.as_str().to_owned(),
            fragments: value.fragments.into_iter().map(Into::into).collect(),
        })
    }
    /// # Errors
    /// Protected owner inspection or transport failed.
    pub fn address_workspace(&self) -> Result<AddressWorkspace, ClientError> {
        let v = self.0.address_workspace()?;
        Ok(AddressWorkspace {
            sender_alias_lanes: v
                .sender_alias_lanes
                .into_iter()
                .map(|lane| ReceivingLaneView {
                    address: lane.address.into(),
                })
                .collect(),
            alias_lanes: v
                .alias_lanes
                .into_iter()
                .map(|lane| ReceivingLaneView {
                    address: lane.address.into(),
                })
                .collect(),
            addresses: v.addresses.into_iter().map(Into::into).collect(),
            observed_at: v.observed_at,
        })
    }
    /// # Errors
    /// Original encrypted journal could not be read.
    pub fn pending_address_update(&self) -> Result<Option<PendingAddressUpdate>, ClientError> {
        Ok(self
            .0
            .pending_address_update()?
            .map(|v| PendingAddressUpdate {
                operation: v.operation,
                action: v.action.into(),
            }))
    }
    /// # Errors
    /// Input, authority, transport or acknowledgement failed; uncertainty remains journaled.
    pub fn update_address(
        &self,
        mutation: AddressMutation,
    ) -> Result<AddressUpdateResult, ClientError> {
        Ok(self.0.update_address(mutation.into())?.into())
    }
    /// # Errors
    /// Uncertain original work remains until its provider acknowledgement is verified.
    pub fn resume_address_update(&self) -> Result<Option<AddressUpdateResult>, ClientError> {
        Ok(self.0.resume_address_update()?.map(Into::into))
    }
}
