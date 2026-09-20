//! Exclusive key ownership, independent of the registries that use a key.
use cs_mail_primitives::{AccountId, ProviderRef};
use cs_mail_protocol::ActorRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyAuthorityOwner {
    Account(AccountId),
    Provider(ProviderRef),
    Scheduler(ProviderRef),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyClaim {
    Reserved {
        enrollment: String,
        intended_owner: AccountId,
        actor: ActorRef,
        key: [u8; 32],
    },
    Assigned {
        owner: KeyAuthorityOwner,
        actor: ActorRef,
        key: [u8; 32],
    },
}
impl KeyClaim {
    /// # Errors
    /// Only the enrollment that reserved a key can assign it to its fixed owner.
    pub fn activate(
        &self,
        enrollment: &str,
        account: AccountId,
    ) -> Result<Self, super::AccountError> {
        match self {
            Self::Reserved {
                enrollment: expected,
                intended_owner,
                actor,
                key,
            } if expected == enrollment && *intended_owner == account => Ok(Self::Assigned {
                owner: KeyAuthorityOwner::Account(account),
                actor: *actor,
                key: *key,
            }),
            _ => Err(super::AccountError::InvalidInput),
        }
    }
}
