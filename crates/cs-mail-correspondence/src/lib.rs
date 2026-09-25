//! Deterministic native correspondence rules. No storage, clock, transport or keys.
mod document;
mod image;
mod policy;
mod selection;
pub use document::*;
pub use image::*;
pub use policy::*;
pub use selection::*;

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CorrespondenceError {
    Invalid,
    Unauthorized,
    Conflict,
    PolicyRestricted,
    PolicyUnresolved,
    Unavailable,
}
impl fmt::Display for CorrespondenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "correspondence: {self:?}")
    }
}
impl std::error::Error for CorrespondenceError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct ConversationId(u128);
impl ConversationId {
    /// # Errors
    /// Rejects the empty identifier.
    pub fn new(value: u128) -> Result<Self, CorrespondenceError> {
        if value == 0 {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(value))
    }
    pub const fn value(self) -> u128 {
        self.0
    }
}
impl TryFrom<u128> for ConversationId {
    type Error = CorrespondenceError;
    fn try_from(value: u128) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ConversationId> for u128 {
    fn from(value: ConversationId) -> Self {
        value.0
    }
}
