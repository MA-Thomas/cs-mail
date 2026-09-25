//! Deterministic, explicitly scoped consent. Authentication is supplied by the application.
use cs_mail_content::EndpointPublicKey;
use cs_mail_primitives::{
    AccountId, CanonicalTime, MessageId, OperationalKeyRef, ProtocolIdentity,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const MAX_SCOPE_MESSAGES: usize = 100;
pub const MAX_GRANT_LIFETIME_MS: u64 = 86_400_000;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConsentError {
    Invalid,
    Unauthorized,
    ScopeDenied,
    Expired,
    Revoked,
    Unavailable,
    Conflict,
}
impl fmt::Display for ConsentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "consent: {self:?}")
    }
}
impl std::error::Error for ConsentError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct GrantId(u128);
impl GrantId {
    /// # Errors
    /// Rejects an empty identifier.
    pub fn new(value: u128) -> Result<Self, ConsentError> {
        if value == 0 {
            Err(ConsentError::Invalid)
        } else {
            Ok(Self(value))
        }
    }
    pub const fn value(self) -> u128 {
        self.0
    }
}
impl TryFrom<u128> for GrantId {
    type Error = ConsentError;
    fn try_from(v: u128) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl From<GrantId> for u128 {
    fn from(v: GrantId) -> Self {
        v.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BTreeSet<MessageId>", into = "BTreeSet<MessageId>")]
pub struct ContentScope(BTreeSet<MessageId>);
impl TryFrom<BTreeSet<MessageId>> for ContentScope {
    type Error = ConsentError;
    fn try_from(v: BTreeSet<MessageId>) -> Result<Self, Self::Error> {
        if v.is_empty() || v.len() > MAX_SCOPE_MESSAGES || v.iter().any(|m| m.0 == 0) {
            return Err(ConsentError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<ContentScope> for BTreeSet<MessageId> {
    fn from(v: ContentScope) -> Self {
        v.0
    }
}
impl ContentScope {
    pub fn messages(&self) -> &BTreeSet<MessageId> {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DecryptionPurpose {
    Display,
    CsqdProcessing { operation: String },
}
impl DecryptionPurpose {
    /// # Errors
    /// Requires a bounded explicit processing operation, never a wildcard purpose.
    pub fn validate(&self) -> Result<(), ConsentError> {
        if let Self::CsqdProcessing { operation } = self
            && (operation.is_empty()
                || operation.len() > 64
                || !operation
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
        {
            return Err(ConsentError::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DecryptionActor {
    UserDevice(OperationalKeyRef),
    CsqdProcessor(OperationalKeyRef),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantUse {
    pub actor: DecryptionActor,
    pub purpose: DecryptionPurpose,
    pub destination: EndpointPublicKey,
}
impl GrantUse {
    /// # Errors
    /// Rejects mixed display/processor roles and empty key identities.
    pub fn validate(&self) -> Result<(), ConsentError> {
        self.purpose.validate()?;
        let ((DecryptionActor::UserDevice(key), DecryptionPurpose::Display)
        | (DecryptionActor::CsqdProcessor(key), DecryptionPurpose::CsqdProcessing { .. })) =
            (&self.actor, &self.purpose)
        else {
            return Err(ConsentError::Invalid);
        };
        if key.0 == 0 || self.destination.reference.0 == 0 || self.destination.bytes == [0; 32] {
            return Err(ConsentError::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GrantState {
    Active,
    Revoked { at: CanonicalTime },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantStatus {
    Active,
    Expired,
    Revoked,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantTerms {
    pub id: GrantId,
    pub account: AccountId,
    pub persona: ProtocolIdentity,
    pub issuer: OperationalKeyRef,
    pub scope: ContentScope,
    pub usage: GrantUse,
    pub issued_at: CanonicalTime,
    pub expires_at: CanonicalTime,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "GrantRecord", into = "GrantRecord")]
pub struct DecryptionGrant(GrantRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantRecord {
    terms: GrantTerms,
    state: GrantState,
}
impl TryFrom<GrantRecord> for DecryptionGrant {
    type Error = ConsentError;
    fn try_from(v: GrantRecord) -> Result<Self, Self::Error> {
        let t = &v.terms;
        t.usage.validate()?;
        if t.account.0 == 0
            || t.persona.0 == 0
            || t.issuer.0 == 0
            || t.expires_at <= t.issued_at
            || t.expires_at.0 - t.issued_at.0 > MAX_GRANT_LIFETIME_MS
            || matches!(v.state, GrantState::Revoked { at } if at < t.issued_at)
            || matches!(t.usage.actor, DecryptionActor::UserDevice(k) if k != t.issuer)
        {
            return Err(ConsentError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<DecryptionGrant> for GrantRecord {
    fn from(v: DecryptionGrant) -> Self {
        v.0
    }
}
impl DecryptionGrant {
    /// # Errors
    /// Checks ownership identifiers, recipient role, scope and bounded lifetime.
    pub fn issue(terms: GrantTerms) -> Result<Self, ConsentError> {
        GrantRecord {
            terms,
            state: GrantState::Active,
        }
        .try_into()
    }
    pub fn terms(&self) -> &GrantTerms {
        &self.0.terms
    }
    pub fn status_at(&self, now: CanonicalTime) -> GrantStatus {
        if matches!(self.0.state, GrantState::Revoked { .. }) {
            GrantStatus::Revoked
        } else if now >= self.terms().expires_at {
            GrantStatus::Expired
        } else {
            GrantStatus::Active
        }
    }
    /// # Errors
    /// Rejects time before issue. Repeated revocation preserves the first fact.
    pub fn revoke(&self, at: CanonicalTime) -> Result<Self, ConsentError> {
        if at < self.terms().issued_at {
            return Err(ConsentError::Invalid);
        }
        let mut next = self.clone();
        if next.0.state == GrantState::Active {
            next.0.state = GrantState::Revoked { at };
        }
        Ok(next)
    }
    /// # Errors
    /// Consent cannot confer content entitlement: the application checks retained copies separately.
    pub fn authorize(
        &self,
        actor: DecryptionActor,
        purpose: &DecryptionPurpose,
        message: MessageId,
        now: CanonicalTime,
    ) -> Result<(), ConsentError> {
        if now < self.terms().issued_at {
            return Err(ConsentError::Unauthorized);
        }
        match self.status_at(now) {
            GrantStatus::Expired => return Err(ConsentError::Expired),
            GrantStatus::Revoked => return Err(ConsentError::Revoked),
            GrantStatus::Active => {}
        }
        if self.terms().usage.actor != actor
            || &self.terms().usage.purpose != purpose
            || !self.terms().scope.messages().contains(&message)
        {
            return Err(ConsentError::ScopeDenied);
        }
        Ok(())
    }
}
