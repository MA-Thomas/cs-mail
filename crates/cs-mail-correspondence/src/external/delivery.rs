//! Durable handoff lifecycle. Dispatch is a once-only attempt: uncertain outcomes
//! are reconciled, never silently retried as a new disclosure.
use crate::CorrespondenceError;
use cs_mail_primitives::CanonicalTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeliveryState {
    Waiting,
    Cancelled {
        at: CanonicalTime,
    },
    DispatchBegun {
        at: CanonicalTime,
    },
    Accepted {
        at: CanonicalTime,
        provider_reference: ProviderReference,
    },
    Failed {
        at: CanonicalTime,
    },
}
impl DeliveryState {
    #[must_use]
    pub fn cancel(&self, now: CanonicalTime) -> Self {
        if *self == Self::Waiting {
            Self::Cancelled { at: now }
        } else {
            self.clone()
        }
    }
    /// # Errors
    /// A dispatch can only begin once, under current consent and live authority.
    pub fn begin(&self, now: CanonicalTime) -> Result<Self, CorrespondenceError> {
        if *self != Self::Waiting {
            return Err(CorrespondenceError::Conflict);
        }
        Ok(Self::DispatchBegun { at: now })
    }
    /// # Errors
    /// Conflicting terminal outcomes and malformed provider references are refused.
    pub fn finish(
        &self,
        accepted: Option<ProviderReference>,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        let next = match accepted {
            Some(reference) => Self::Accepted {
                at: now,
                provider_reference: reference,
            },
            None => Self::Failed { at: now },
        };
        match self {
            Self::DispatchBegun { at } if now >= *at => Ok(next),
            Self::Accepted {
                at,
                provider_reference,
            } if now >= *at
                && matches!(&next, Self::Accepted { provider_reference: r, .. } if r == provider_reference) =>
            {
                Ok(self.clone())
            }
            Self::Failed { at } if now >= *at && matches!(next, Self::Failed { .. }) => {
                Ok(self.clone())
            }
            _ => Err(CorrespondenceError::Conflict),
        }
    }
}

/// A carrier's acceptance identifier, distinct from a cs-mail message identifier.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderReference(String);
impl ProviderReference {
    /// # Errors
    /// Rejects empty, oversized or non-token identifiers.
    pub fn new(value: String) -> Result<Self, CorrespondenceError> {
        if value.is_empty()
            || value.len() > 256
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ProviderReference {
    type Error = CorrespondenceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ProviderReference> for String {
    fn from(value: ProviderReference) -> Self {
        value.0
    }
}

/// Provider event identity is not a provider email identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderEventId(String);
impl ProviderEventId {
    /// # Errors
    /// Rejects malformed event identifiers.
    pub fn new(value: String) -> Result<Self, CorrespondenceError> {
        ProviderReference::new(value.clone())?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ProviderEventId {
    type Error = CorrespondenceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ProviderEventId> for String {
    fn from(value: ProviderEventId) -> Self {
        value.0
    }
}

/// Observations may arrive out of order. Preserve them independently of handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProviderEventKind {
    Processed,
    Delivered,
    HardBounce,
    SoftBounce,
    Rejected,
    SpamComplaint,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEvent {
    pub id: ProviderEventId,
    pub reference: ProviderReference,
    pub message: cs_mail_primitives::MessageId,
    pub sender: cs_mail_primitives::ProtocolIdentity,
    pub recipient: cs_mail_guests::GuestMailbox,
    pub kind: ProviderEventKind,
    /// Provider-reported timestamp retained as evidence, not used as our authority clock.
    pub provider_time: ProviderEventTime,
}

/// A provider report about transactional mail, never a member-authored message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeProviderEvent {
    pub id: ProviderEventId,
    pub reference: ProviderReference,
    pub notice: cs_mail_guests::NoticeId,
    pub recipient: cs_mail_guests::GuestMailbox,
    pub kind: ProviderEventKind,
    pub provider_time: ProviderEventTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderEventTime(String);
impl TryFrom<String> for ProviderEventTime {
    type Error = CorrespondenceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<ProviderEventTime> for String {
    fn from(value: ProviderEventTime) -> Self {
        value.0
    }
}
