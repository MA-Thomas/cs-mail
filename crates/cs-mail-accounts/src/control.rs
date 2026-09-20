//! Product lifecycle and explicitly granted management authority.
use crate::AccountError;
use cs_mail_primitives::{AccountId, IdempotencyKey, OperationalKeyRef, ProtocolIdentity};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Active,
    Suspended,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredControl")]
pub struct AccountControl {
    status: AccountStatus,
    revision: u64,
    personas: BTreeSet<ProtocolIdentity>,
    managers: BTreeSet<OperationalKeyRef>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredControl {
    status: AccountStatus,
    revision: u64,
    personas: BTreeSet<ProtocolIdentity>,
    managers: BTreeSet<OperationalKeyRef>,
}
impl TryFrom<StoredControl> for AccountControl {
    type Error = AccountError;
    fn try_from(v: StoredControl) -> Result<Self, Self::Error> {
        if v.personas.is_empty()
            || v.personas.iter().any(|id| id.0 == 0)
            || v.managers.is_empty()
            || v.managers.iter().any(|id| id.0 == 0)
        {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self {
            status: v.status,
            revision: v.revision,
            personas: v.personas,
            managers: v.managers,
        })
    }
}
impl AccountControl {
    /// # Errors
    /// Rejects empty persona or manager identifiers.
    pub fn new(
        persona: ProtocolIdentity,
        manager: OperationalKeyRef,
    ) -> Result<Self, AccountError> {
        StoredControl {
            status: AccountStatus::Active,
            revision: 0,
            personas: [persona].into(),
            managers: [manager].into(),
        }
        .try_into()
    }
    pub const fn status(&self) -> AccountStatus {
        self.status
    }
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    pub fn personas(&self) -> &BTreeSet<ProtocolIdentity> {
        &self.personas
    }
    pub fn is_manager(&self, key: OperationalKeyRef) -> bool {
        self.managers.contains(&key)
    }
    pub const fn allows_service(&self) -> bool {
        matches!(self.status, AccountStatus::Active)
    }
    /// Used only after the application verifies an ordered identity-security event.
    /// # Errors
    /// Rejects invalid replacement authority or revision overflow.
    pub fn apply_security_change(
        &mut self,
        replacement_manager: Option<OperationalKeyRef>,
    ) -> Result<(), AccountError> {
        if replacement_manager.is_some_and(|key| key.0 == 0) {
            return Err(AccountError::InvalidInput);
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(AccountError::InvalidInput)?;
        if self.status != AccountStatus::Closed {
            self.status = AccountStatus::Suspended;
        }
        if let Some(key) = replacement_manager {
            self.managers = [key].into();
        }
        Ok(())
    }
    /// Pure transition; the application verifies signature, ownership and revision first.
    /// # Errors
    /// Rejects invalid lifecycle or authority transitions and revision overflow.
    pub fn apply(&mut self, command: &AccountCommand) -> Result<(), AccountError> {
        if self.status == AccountStatus::Closed {
            return Err(AccountError::InvalidInput);
        }
        let mut next = self.clone();
        match *command {
            AccountCommand::RegisterKey(ref key)
                if next.allows_service()
                    && key.reference.0 != 0
                    && matches!(key.actor, cs_mail_protocol::ActorRef::Sender(persona) | cs_mail_protocol::ActorRef::Recipient(persona) if next.personas.contains(&persona)) =>
                {}
            AccountCommand::SetStatus(status) => next.status = status,
            AccountCommand::AddPersona(persona) if next.allows_service() && persona.0 != 0 => {
                if !next.personas.insert(persona) {
                    return Err(AccountError::InvalidInput);
                }
            }
            AccountCommand::GrantManager(key) if next.allows_service() && key.0 != 0 => {
                if !next.managers.insert(key) {
                    return Err(AccountError::InvalidInput);
                }
            }
            AccountCommand::RevokeManager(key) if next.managers.len() > 1 => {
                if !next.managers.remove(&key) {
                    return Err(AccountError::InvalidInput);
                }
            }
            _ => return Err(AccountError::InvalidInput),
        }
        next.revision = self
            .revision
            .checked_add(1)
            .ok_or(AccountError::InvalidInput)?;
        *self = next;
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountCommand {
    RegisterKey(NewOperationalKey),
    SetStatus(AccountStatus),
    AddPersona(ProtocolIdentity),
    GrantManager(OperationalKeyRef),
    RevokeManager(OperationalKeyRef),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountCommand {
    pub account: AccountId,
    pub operational_key: OperationalKeyRef,
    pub expected_revision: u64,
    pub idempotency_key: IdempotencyKey,
    pub product: String,
    pub command: AccountCommand,
    pub signature: Vec<u8>,
}
impl SignedAccountCommand {
    fn bytes(&self) -> Result<Vec<u8>, AccountError> {
        if self.product.is_empty() || self.account.0 == 0 {
            return Err(AccountError::InvalidInput);
        }
        serde_json::to_vec(&(
            "cs-mail/product-account-command/v1",
            self.account,
            self.operational_key,
            self.expected_revision,
            self.idempotency_key,
            &self.product,
            &self.command,
        ))
        .map_err(|_| AccountError::InvalidInput)
    }
    /// # Errors
    /// Rejects invalid context or unencodable signing bytes.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, AccountError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects malformed keys, signatures or substituted context.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), AccountError> {
        VerifyingKey::from_bytes(key)
            .map_err(|_| AccountError::InvalidInput)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| AccountError::InvalidInput)?,
            )
            .map_err(|_| AccountError::InvalidInput)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewOperationalKey {
    pub reference: OperationalKeyRef,
    pub actor: cs_mail_protocol::ActorRef,
    pub key: [u8; 32],
    pub proof: Vec<u8>,
}
impl NewOperationalKey {
    fn bytes(&self, account: AccountId, product: &str) -> Result<Vec<u8>, AccountError> {
        serde_json::to_vec(&(
            "cs-mail/account-key-possession/v1",
            account,
            product,
            self.reference,
            self.actor,
            self.key,
        ))
        .map_err(|_| AccountError::InvalidInput)
    }
    /// # Errors
    /// Rejects unencodable account or key context.
    pub fn prove(
        account: AccountId,
        product: &str,
        reference: OperationalKeyRef,
        actor: cs_mail_protocol::ActorRef,
        secret: &[u8; 32],
    ) -> Result<Self, AccountError> {
        let signer = SigningKey::from_bytes(secret);
        let mut value = Self {
            reference,
            actor,
            key: signer.verifying_key().to_bytes(),
            proof: Vec::new(),
        };
        value.proof = signer
            .sign(&value.bytes(account, product)?)
            .to_bytes()
            .to_vec();
        Ok(value)
    }
    /// # Errors
    /// Rejects malformed keys, signatures or substituted context.
    pub fn verify(&self, account: AccountId, product: &str) -> Result<(), AccountError> {
        VerifyingKey::from_bytes(&self.key)
            .map_err(|_| AccountError::InvalidInput)?
            .verify_strict(
                &self.bytes(account, product)?,
                &Signature::from_slice(&self.proof).map_err(|_| AccountError::InvalidInput)?,
            )
            .map_err(|_| AccountError::InvalidInput)
    }
}
