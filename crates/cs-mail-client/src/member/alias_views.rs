//! Owner address projections and commands. Presentation does not establish routing authority.
use super::{Error, MemberClient, State, enrollment::Setup, storage};
use cs_mail_accounts::alias_permission::PermissionState;
use cs_mail_accounts::{
    aliases::{Alias, AliasKind, AliasLifecycle, AliasPortfolio},
    directory::MemberAddress,
};
use cs_mail_application::aliases::{AliasCreation, Command, Outcome, ReplacementAddress};
use cs_mail_primitives::{AliasId, CanonicalTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressKind {
    Private,
    Public,
}
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressConversation {
    pub id: String,
    pub sender: String,
    pub approved: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedAddress {
    pub permission: AddressPermission,
    pub permission_revision: u64,
    pub permitted_senders: Vec<String>,
    pub permission_expires_at: Option<u64>,
    pub id: String,
    pub address: String,
    pub description: Option<String>,
    pub revision: u64,
    pub kind: AddressKind,
    pub state: AddressState,
    pub can_restrict: bool,
    pub can_replace: bool,
    pub can_retire: bool,
    pub conversations: Vec<AddressConversation>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressPermission {
    Unpaired,
    Broad,
    Restricted,
    Senders,
}
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddressMutation {
    Permission {
        id: String,
        revision: u64,
        permission_revision: u64,
        action: PermissionMutation,
    },
    CreatePrivate {
        description: String,
    },
    CreatePublic {
        address: String,
    },
    Restrict {
        id: String,
        revision: u64,
    },
    ReplacePublic {
        id: String,
        revision: u64,
        address: String,
        until: u64,
    },
    ReplacePrivate {
        id: String,
        revision: u64,
        until: u64,
    },
    Retire {
        id: String,
        revision: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingAddressAction {
    CreatePrivate { description: String },
    CreatePublic { address: String },
    Restrict { id: String },
    Replace { id: String },
    Retire { id: String },
    ApproveConversation { id: String },
    ScopeSender,
    Permission { id: String },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAddressUpdate {
    pub operation: String,
    pub action: PendingAddressAction,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddressUpdateResult {
    Applied { created_alias: Option<String> },
    Refused { explanation: String },
}
/// An alias-scoped grant and its receiving route, including explicit closed states.
/// Classification comes from the checked domain grant, never a UI address heuristic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivingLaneView {
    pub address: ManagedAddress,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressWorkspace {
    pub alias_lanes: Vec<ReceivingLaneView>,
    pub sender_alias_lanes: Vec<ReceivingLaneView>,
    pub addresses: Vec<ManagedAddress>,
    pub observed_at: u64,
    pub current_public_alias: Option<String>,
}
impl MemberClient {
    /// # Errors
    /// Missing custody, unavailable authority, or an altered provider response.
    pub fn address_workspace(&self) -> Result<AddressWorkspace, Error> {
        let addresses = self.aliases()?;
        let at = (self.clock)();
        let records = addresses.iter().map(|a| (a.id(), a.clone())).collect();
        let owner = addresses.first().map(Alias::owner);
        let current_public_alias = owner
            .map(|owner| {
                AliasPortfolio::new(owner, &records)
                    .map(|p| p.current_public().map(|a| a.id().value().to_string()))
            })
            .transpose()
            .map_err(|_| Error::InvalidData)?
            .flatten();
        Ok(AddressWorkspace {
            sender_alias_lanes: addresses
                .iter()
                .filter(|a| matches!(a.permission().state(), PermissionState::Senders(_)))
                .map(|a| ReceivingLaneView {
                    address: project(a, at),
                })
                .collect(),
            current_public_alias,
            alias_lanes: addresses
                .iter()
                .filter(|a| {
                    matches!(
                        a.permission().state(),
                        PermissionState::Broad(_) | PermissionState::Restricted { .. }
                    )
                })
                .map(|a| ReceivingLaneView {
                    address: project(a, at),
                })
                .collect(),
            addresses: addresses.iter().map(|a| project(a, at)).collect(),
            observed_at: at.0,
        })
    }
    /// # Errors
    /// Missing credential or an unreadable encrypted journal.
    pub fn pending_address_update(&self) -> Result<Option<PendingAddressUpdate>, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        pending(&state)
    }
    /// # Errors
    /// Invalid inputs, unresolved existing work, unavailable transport or altered receipt.
    pub fn update_address(&self, mutation: AddressMutation) -> Result<AddressUpdateResult, Error> {
        let command = match mutation {
            AddressMutation::Permission {
                id,
                revision,
                permission_revision,
                action,
            } => {
                let action = match action {
                    PermissionMutation::Open => {
                        cs_mail_application::aliases::PermissionAction::Open
                    }
                    PermissionMutation::Restrict => {
                        cs_mail_application::aliases::PermissionAction::Restrict
                    }
                    PermissionMutation::Remove => {
                        cs_mail_application::aliases::PermissionAction::Remove
                    }
                    PermissionMutation::Senders {
                        domains,
                        senders,
                        valid_until,
                        max_messages_per_day,
                    } => self.paired_grants(domains, senders, valid_until, max_messages_per_day)?,
                };
                Command::Permission {
                    id: alias_id(&id)?,
                    expected_address: revision,
                    expected_permission: permission_revision.into(),
                    action,
                }
            }
            AddressMutation::CreatePrivate { description } => Command::Create {
                id: fresh_id()?,
                creation: AliasCreation::Private {
                    description: description.try_into().map_err(|_| Error::InvalidData)?,
                },
            },
            AddressMutation::CreatePublic { address } => Command::Create {
                id: fresh_id()?,
                creation: AliasCreation::Public {
                    address: member_address(address)?,
                },
            },
            AddressMutation::Restrict { id, revision } => Command::Restrict {
                id: alias_id(&id)?,
                expected: revision,
            },
            AddressMutation::ReplacePublic {
                id,
                revision,
                address,
                until,
            } => Command::Replace {
                id: alias_id(&id)?,
                expected: revision,
                replacement: fresh_id()?,
                address: ReplacementAddress::Public {
                    address: member_address(address)?,
                },
                until: CanonicalTime(until),
            },
            AddressMutation::ReplacePrivate {
                id,
                revision,
                until,
            } => Command::Replace {
                id: alias_id(&id)?,
                expected: revision,
                replacement: fresh_id()?,
                address: ReplacementAddress::GeneratePrivate,
                until: CanonicalTime(until),
            },
            AddressMutation::Retire { id, revision } => Command::Retire {
                id: alias_id(&id)?,
                expected: revision,
            },
        };
        result(&self.change_alias(command)?)
    }
    /// # Errors
    /// Retry preserves the signed operation on unavailable transport or invalid evidence.
    pub fn resume_address_update(&self) -> Result<Option<AddressUpdateResult>, Error> {
        self.retry_alias()?
            .map(|outcome| result(&outcome))
            .transpose()
    }
}
fn member_address(value: String) -> Result<MemberAddress, Error> {
    value.try_into().map_err(|_| Error::InvalidData)
}
fn alias_id(value: &str) -> Result<AliasId, Error> {
    AliasId::new(value.parse().map_err(|_| Error::InvalidData)?).map_err(|_| Error::InvalidData)
}
fn fresh_id() -> Result<AliasId, Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    AliasId::new(u128::from_be_bytes(bytes) | 1).map_err(|_| Error::InvalidData)
}
fn view_kind(kind: AliasKind) -> AddressKind {
    match kind {
        AliasKind::Private => AddressKind::Private,
        AliasKind::Public => AddressKind::Public,
    }
}
fn pending(state: &State) -> Result<Option<PendingAddressUpdate>, Error> {
    let Some(Setup::Alias(request)) = &state.setup else {
        return Ok(None);
    };
    let action = match &request.command {
        Command::Create { creation, .. } => match creation {
            AliasCreation::Private { description } => PendingAddressAction::CreatePrivate {
                description: description.as_str().into(),
            },
            AliasCreation::Public { address } => PendingAddressAction::CreatePublic {
                address: address.as_str().into(),
            },
        },
        Command::Restrict { id, .. } => PendingAddressAction::Restrict {
            id: id.value().to_string(),
        },
        Command::Replace { id, .. } => PendingAddressAction::Replace {
            id: id.value().to_string(),
        },
        Command::Retire { id, .. } => PendingAddressAction::Retire {
            id: id.value().to_string(),
        },
        Command::ApproveConversation { id, .. } => PendingAddressAction::ApproveConversation {
            id: id.value().to_string(),
        },
        Command::ScopeSender { .. } => PendingAddressAction::ScopeSender,
        Command::Permission { id, .. } => PendingAddressAction::Permission {
            id: id.value().to_string(),
        },
        Command::Inspect => return Err(Error::InvalidData),
    };
    Ok(Some(PendingAddressUpdate {
        operation: request.operation.0.to_string(),
        action,
    }))
}

fn result(outcome: &Outcome) -> Result<AddressUpdateResult, Error> {
    use cs_mail_accounts::aliases::AliasError;
    match outcome {
        Outcome::Changed(aliases) => Ok(AddressUpdateResult::Applied {
            created_alias: aliases.iter().find(|a| a.revision() == 0 && a.permission().revision().value() == 0).map(|a| a.id().value().to_string()),
        }),
        Outcome::Scoped(_) => Ok(AddressUpdateResult::Applied { created_alias: None }),
        Outcome::Aliases(_) => Err(Error::InvalidData),
        Outcome::Refused(error) => Ok(AddressUpdateResult::Refused {
            explanation: match error {
                AliasError::PublicAliasExists => "You already have a public-facing alias. Replace it to choose a different address.",
                AliasError::Conflict => {
                    "The address is reserved or the alias has changed. Refresh before making a new update."
                }
                AliasError::Retired => "This alias no longer permits that change.",
                AliasError::Restricted => "This alias is already restricted.",
                AliasError::Blocked => {
                    "The sender is blocked. This update cannot override that block."
                }
                AliasError::Coverage => "Current service coverage is required.",
                AliasError::Authentication | AliasError::Permission => {
                    "Current authority does not permit this update."
                }
                AliasError::Invalid => "The alias description, address or transition details were not accepted.",
            }
            .into(),
        }),
    }
}
pub(super) fn project(alias: &Alias, at: CanonicalTime) -> ManagedAddress {
    let mut conversations = vec![];
    let (state, retained) = match alias.lifecycle() {
        AliasLifecycle::Active => (AddressState::Active, None),
        AliasLifecycle::Restricted {
            conversations,
            approvals,
            ..
        } => (AddressState::Restricted, Some((conversations, approvals))),
        AliasLifecycle::Replaced {
            replacement,
            until,
            conversations,
            approvals,
            ..
        } => (
            AddressState::Replaced {
                replacement: replacement.value().to_string(),
                until: until.0,
                transition_ended: at >= *until,
            },
            Some((conversations, approvals)),
        ),
        AliasLifecycle::Retired { .. } => (AddressState::Retired, None),
    };
    if let Some((admitted, approved)) = retained {
        conversations.extend(admitted.values().map(|c| AddressConversation {
            id: c.conversation().value().to_string(),
            sender: c.sender().0.to_string(),
            approved: false,
        }));
        conversations.extend(approved.iter().map(|(id, c)| AddressConversation {
            id: id.value().to_string(),
            sender: c.sender().0.to_string(),
            approved: true,
        }));
    }
    let (permission, permitted_senders, permission_expires_at) = match alias.permission().state() {
        PermissionState::None => (AddressPermission::Unpaired, vec![], None),
        PermissionState::Broad(_) => (AddressPermission::Broad, vec![], None),
        PermissionState::Restricted { .. } => (AddressPermission::Restricted, vec![], None),
        PermissionState::Senders(lanes) => (
            AddressPermission::Senders,
            lanes
                .iter()
                .map(|l| match &l.grant().subject {
                    cs_mail_capabilities::LaneSubject::LegacyDomain(d) => d.as_str().to_owned(),
                    _ => l.grant().sender.0.to_string(),
                })
                .collect(),
            lanes.iter().map(|l| l.grant().not_after.0).min(),
        ),
    };
    ManagedAddress {
        permission,
        permission_revision: alias.permission().revision().value(),
        permitted_senders,
        permission_expires_at,
        id: alias.id().value().to_string(),
        address: alias.address().as_str().into(),
        description: alias.description().map(|d| d.as_str().to_owned()),
        revision: alias.revision(),
        kind: view_kind(alias.kind()),
        can_restrict: matches!(state, AddressState::Active),
        can_replace: matches!(state, AddressState::Active | AddressState::Restricted),
        can_retire: !matches!(state, AddressState::Retired),
        state,
        conversations,
    }
}

impl MemberClient {
    fn paired_grants(
        &self,
        domains: Vec<String>,
        senders: Vec<String>,
        valid_until: u64,
        max_messages_per_day: u32,
    ) -> Result<cs_mail_application::aliases::PermissionAction, Error> {
        use cs_mail_capabilities::{
            DeclarationAuthorityConstraint, LaneMode, LaneSubject, RateLimit, SenderScopedGrant,
            SignedLaneGrant,
        };
        use cs_mail_primitives::{DeclaredPurpose, Duration, LaneId, OriginMode, Version};
        use ed25519_dalek::Signer;
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let at = (self.clock)();
        let lifetime = valid_until
            .checked_sub(at.0)
            .filter(|n| *n > 0)
            .ok_or(Error::InvalidData)?;
        if domains.len() + senders.len() > 32 || max_messages_per_day == 0 {
            return Err(Error::InvalidData);
        }
        let mut subjects = Vec::new();
        for text in domains {
            let domain = cs_mail_adapters::DomainIdentity::parse_ascii(text.trim())
                .map_err(|_| Error::InvalidData)?;
            let sender = domain
                .synthetic_protocol_identity(&state.profile.deployment.scope.deployment_domain, 1);
            subjects.push((
                LaneSubject::LegacyDomain(domain),
                sender,
                DeclarationAuthorityConstraint::LegacyGateway,
            ));
        }
        for text in senders {
            let sender = super::parse_sender(&text)?;
            let view = state
                .inbox
                .iter()
                .find(|v| v.handle.sender == sender)
                .ok_or(Error::InvalidData)?;
            let guest = view.guest.is_some();
            subjects.push((
                if guest {
                    LaneSubject::GuestMailbox(sender)
                } else {
                    LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                        *view.handle.relationship.as_bytes(),
                    ))
                },
                sender,
                if guest {
                    DeclarationAuthorityConstraint::GuestGateway
                } else {
                    DeclarationAuthorityConstraint::NativeSender
                },
            ));
        }
        let mut grants = Vec::new();
        for (subject, sender, authority) in subjects {
            let grant = SenderScopedGrant {
                id: LaneId(fresh_id()?.value()),
                subject,
                sender,
                recipient: state.profile.persona,
                protocol_version: state.profile.deployment.policy.protocol_version,
                deployment_domain: state.profile.deployment.scope.deployment_domain,
                intended_provider: state.profile.deployment.scope.provider,
                recipient_operational_key: state.profile.recipient_key,
                purpose: if authority == DeclarationAuthorityConstraint::NativeSender {
                    DeclaredPurpose::Known(cs_mail_primitives::KnownPurpose::Personal)
                } else {
                    DeclaredPurpose::Unspecified
                },
                origin: if authority == DeclarationAuthorityConstraint::NativeSender {
                    Some(OriginMode::HumanInitiated)
                } else {
                    Some(OriginMode::LegacyOrUnspecified)
                },
                declaration_authority: authority,
                lifetime: Duration(lifetime),
                rate_limit: RateLimit {
                    max_messages: max_messages_per_day,
                    interval: Duration(86_400_000),
                },
                mode: LaneMode::Expiring,
                issued_at: at,
                not_before: at,
                not_after: CanonicalTime(valid_until),
                version: Version(1),
            };
            let signature = ed25519_dalek::SigningKey::from_bytes(&keys.recipient)
                .sign(&grant.signing_bytes().map_err(|_| Error::InvalidData)?)
                .to_bytes();
            grants.push(SignedLaneGrant { grant, signature });
        }
        Ok(cs_mail_application::aliases::PermissionAction::Senders(
            grants,
        ))
    }
}
