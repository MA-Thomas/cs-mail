//! Product account ownership and enrollment. No database or service policy evaluation.
use cs_mail_finance::BankVerification;
use cs_mail_primitives::{
    AccountId, BillingAccountId, MemberId, OperationalKeyRef, PrincipalRef, ProtocolIdentity,
};
use cs_mail_protocol::ActorRef;
/// Domain validation errors are independent of HTTP and signature verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountError {
    InvalidInput,
}
impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid account input")
    }
}
impl std::error::Error for AccountError {}
use AccountError as Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentInput {
    pub bank: BankVerification,
    pub persona: ProtocolIdentity,
    pub actor: ActorRef,
    pub key_ref: OperationalKeyRef,
    pub initial_key: [u8; 32],
    pub maximum_unresolved: u32,
}
impl EnrollmentInput {
    /// Ownership reserved by one operation cannot be claimed by another.
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.bank.account == other.bank.account
            || self.bank.member == other.bank.member
            || self.bank.bank_token == other.bank.bank_token
            || self.persona == other.persona
            || self.key_ref == other.key_ref
    }
    /// # Errors
    /// Rejects non-persona authority, empty identifiers and funding limits.
    pub fn validate(&self) -> Result<(), Error> {
        if self.persona.0 == 0
            || self.key_ref.0 == 0
            || self.maximum_unresolved == 0
            || !matches!(self.actor,ActorRef::Sender(id)|ActorRef::Recipient(id) if id==self.persona)
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountIdentityBinding {
    issuer: String,
    product: String,
    subject_ref: identity_contract::ProductSubjectRef,
    version: u64,
}
impl AccountIdentityBinding {
    /// # Errors
    /// Rejects absent deployment identity or an unsupported binding version.
    pub fn new(
        issuer: String,
        product: String,
        subject_ref: identity_contract::ProductSubjectRef,
        version: u64,
    ) -> Result<Self, AccountError> {
        if issuer.is_empty() || product.is_empty() || version != 1 {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self {
            issuer,
            product,
            subject_ref,
            version,
        })
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn product(&self) -> &str {
        &self.product
    }
    pub fn subject_ref(&self) -> &str {
        self.subject_ref.as_str()
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    id: AccountId,
    principal: PrincipalRef,
    billing: BillingAccountId,
    member: MemberId,
    binding: AccountIdentityBinding,
}
impl Account {
    /// Constructs domain state; authorization belongs to the application activation boundary.
    /// # Errors
    /// Rejects invalid identifiers and account input.
    pub fn new(
        id: AccountId,
        principal: PrincipalRef,
        input: &EnrollmentInput,
        binding: AccountIdentityBinding,
    ) -> Result<Self, AccountError> {
        input.validate()?;
        if id.0 == 0 || principal.0 == 0 {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self {
            id,
            principal,
            billing: input.bank.account,
            member: input.bank.member,
            binding,
        })
    }
    pub const fn id(&self) -> AccountId {
        self.id
    }
    pub const fn principal(&self) -> PrincipalRef {
        self.principal
    }
    pub const fn billing(&self) -> BillingAccountId {
        self.billing
    }
    pub const fn member(&self) -> MemberId {
        self.member
    }
    pub fn binding(&self) -> &AccountIdentityBinding {
        &self.binding
    }
}

pub mod keys;

pub mod control;
