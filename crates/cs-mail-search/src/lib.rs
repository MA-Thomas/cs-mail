//! Derived retrieval evidence. Source owners retain reading, reuse and retention authority.
//! All decisions take explicit source facts, versions and time; adapters perform I/O.
use cs_mail_correspondence::{ConversationId, MessageVersion};
use cs_mail_primitives::{AccountId, CanonicalTime, MessageId};
use serde::{Deserialize, Serialize};

mod lifecycle;
mod profile;
mod query;
pub use lifecycle::{DerivedChange, DerivedSource, IndexGeneration, SourceStatus, WorkIntent};
pub use profile::{CompatibilityProfile, Embedding, ProfileId};
pub use query::{Query, literal_range};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Stale,
    Removed,
    Incompatible,
    Expired,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CorrespondenceStream {
    Native,
    External,
}

/// Account and occurrence identity are independent of text similarity or checksums.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SourceRecord", into = "SourceRecord")]
pub struct SourceIdentity(SourceRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRecord {
    account: AccountId,
    occurrence: SourceOccurrence,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceOccurrence {
    Message {
        stream: CorrespondenceStream,
        conversation: ConversationId,
        message: MessageId,
    },
    PrivateDraft {
        id: u128,
    },
}
impl TryFrom<SourceRecord> for SourceIdentity {
    type Error = Error;
    fn try_from(v: SourceRecord) -> Result<Self, Error> {
        if v.account.0 == 0
            || matches!(
                v.occurrence,
                SourceOccurrence::PrivateDraft { id: 0 }
                    | SourceOccurrence::Message {
                        message: MessageId(0),
                        ..
                    }
            )
        {
            return Err(Error::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<SourceIdentity> for SourceRecord {
    fn from(v: SourceIdentity) -> Self {
        v.0
    }
}
impl SourceIdentity {
    /// # Errors
    /// Requires nonzero account and occurrence identifiers.
    pub fn new(account: AccountId, occurrence: SourceOccurrence) -> Result<Self, Error> {
        SourceRecord {
            account,
            occurrence,
        }
        .try_into()
    }
    pub const fn account(&self) -> AccountId {
        self.0.account
    }
    pub const fn occurrence(&self) -> &SourceOccurrence {
        &self.0.occurrence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceVersion {
    Message(MessageVersion),
    PrivateDraft {
        revision: std::num::NonZeroU64,
        checksum: [u8; 32],
    },
}
impl SourceVersion {
    pub(crate) fn matches(self, source: &SourceIdentity) -> bool {
        matches!(
            (source.occurrence(), self),
            (SourceOccurrence::Message { .. }, Self::Message(_))
                | (
                    SourceOccurrence::PrivateDraft { .. },
                    Self::PrivateDraft { .. }
                )
        )
    }
}

/// Coordinates refer to one canonical extracted text part, never extra model context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TextPart {
    Subject,
    Body { part: u32 },
    Quotation { part: u32, block: u32 },
    SharedCopy { part: u32, block: u32 },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PassageRecord", into = "PassageRecord")]
pub struct Passage(PassageRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PassageRecord {
    source: SourceIdentity,
    version: SourceVersion,
    extraction: u32,
    location: TextPart,
    start: u32,
    end: u32,
    checksum: [u8; 32],
}
impl TryFrom<PassageRecord> for Passage {
    type Error = Error;
    fn try_from(v: PassageRecord) -> Result<Self, Error> {
        if !v.version.matches(&v.source) || v.extraction == 0 || v.start >= v.end {
            return Err(Error::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<Passage> for PassageRecord {
    fn from(v: Passage) -> Self {
        v.0
    }
}
impl Passage {
    /// Bind a passage to an exact UTF-8 range in its canonical extracted source part.
    /// # Errors
    /// Invalid identity/version pairing, extraction version or text boundaries.
    pub fn new(
        source: SourceIdentity,
        version: SourceVersion,
        extraction: u32,
        location: TextPart,
        text: &str,
        start: u32,
        end: u32,
    ) -> Result<Self, Error> {
        use sha2::{Digest, Sha256};
        let selected = text
            .get(start as usize..end as usize)
            .ok_or(Error::Invalid)?;
        PassageRecord {
            source,
            version,
            extraction,
            location,
            start,
            end,
            checksum: Sha256::digest(selected.as_bytes()).into(),
        }
        .try_into()
    }
    pub const fn source(&self) -> &SourceIdentity {
        &self.0.source
    }
    pub const fn version(&self) -> SourceVersion {
        self.0.version
    }
    pub const fn location(&self) -> TextPart {
        self.0.location
    }
    pub const fn range(&self) -> (u32, u32) {
        (self.0.start, self.0.end)
    }
    /// Opening/reuse must still obtain current eligibility from the source owner.
    /// # Errors
    /// The extracted source no longer matches this passage's bounds or checksum.
    pub fn verify<'a>(&self, text: &'a str) -> Result<&'a str, Error> {
        use sha2::{Digest, Sha256};
        let selected = text
            .get(self.0.start as usize..self.0.end as usize)
            .ok_or(Error::Stale)?;
        if <[u8; 32]>::from(Sha256::digest(selected.as_bytes())) != self.0.checksum {
            return Err(Error::Stale);
        }
        Ok(selected)
    }
}

/// Owner-authorized maximum for removal of deleted source content and its derivatives
/// from server backups. It does not extend eligibility or promise a deployed backup service.
pub const BACKUP_REMOVAL_MAX_MILLIS: u64 = 7 * 24 * 60 * 60 * 1000;
/// # Errors
/// The explicit deletion time cannot represent the complete deadline.
pub fn backup_removal_deadline(deleted_at: CanonicalTime) -> Result<CanonicalTime, Error> {
    deleted_at
        .0
        .checked_add(BACKUP_REMOVAL_MAX_MILLIS)
        .map(CanonicalTime)
        .ok_or(Error::Invalid)
}
