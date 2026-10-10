//! Consent to disclose a conversation through its fixed external route.
//! The owning conversation binds this lifecycle to its member and destination.
use crate::ConsentError;
use cs_mail_primitives::CanonicalTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DisclosureConsent {
    NotGranted,
    Granted { at: CanonicalTime },
    Withdrawn { at: CanonicalTime },
}
impl DisclosureConsent {
    /// # Errors
    /// A new grant cannot predate the recorded withdrawal or prior grant.
    pub fn grant(self, now: CanonicalTime) -> Result<Self, ConsentError> {
        match self {
            Self::Granted { at } | Self::Withdrawn { at } if now < at => Err(ConsentError::Invalid),
            granted @ Self::Granted { .. } => Ok(granted),
            _ => Ok(Self::Granted { at: now }),
        }
    }
    pub const fn permits_dispatch(self) -> bool {
        matches!(self, Self::Granted { .. })
    }
    /// # Errors
    /// A withdrawal cannot predate the recorded consent.
    pub fn withdraw(self, now: CanonicalTime) -> Result<Self, ConsentError> {
        let (Self::Granted { at } | Self::Withdrawn { at }) = self else {
            return Ok(self);
        };
        if now < at {
            return Err(ConsentError::Invalid);
        }
        Ok(match self {
            Self::NotGranted => Self::NotGranted,
            Self::Granted { .. } => Self::Withdrawn { at: now },
            withdrawn @ Self::Withdrawn { .. } => withdrawn,
        })
    }
}
