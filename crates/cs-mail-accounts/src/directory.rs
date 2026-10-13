//! Address-owned public presentation. These statements are not identity-service evidence.
use crate::AccountError;
use cs_mail_primitives::ProtocolIdentity;
use serde::{Deserialize, Serialize};

/// A deployment-issued public mailbox, distinct from its protocol identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct MemberAddress {
    value: String,
    domain: String,
}
impl TryFrom<String> for MemberAddress {
    type Error = AccountError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.trim().to_ascii_lowercase();
        let (local, domain) = value.split_once('@').ok_or(AccountError::InvalidInput)?;
        if local.is_empty()
            || local.len() > 64
            || domain.len() > 253
            || !local
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            || local.starts_with('.')
            || local.ends_with('.')
            || local.contains("..")
            || local.starts_with("member-")
            || domain.split('.').count() < 2
            || domain.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(AccountError::InvalidInput);
        }
        let domain = domain.to_owned();
        Ok(Self { value, domain })
    }
}
impl From<MemberAddress> for String {
    fn from(value: MemberAddress) -> Self {
        value.value
    }
}
impl MemberAddress {
    pub fn as_str(&self) -> &str {
        &self.value
    }
    pub fn domain(&self) -> &str {
        &self.domain
    }
}

/// Exact directory references select a public identity without granting contact authority.
#[derive(Clone, Debug)]
pub enum DirectoryReference {
    Protocol(ProtocolIdentity),
    Mailbox(MemberAddress),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiscoveryVisibility {
    #[default]
    Listed,
    Unlisted,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DirectoryRevision(pub u64);

/// Bounded PNG upload. Header checks establish dimensions, not decoder acceptance.
/// Hosts must decode defensively; no external image URL is followed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "Vec<u8>", into = "Vec<u8>")]
pub struct ProfileImage(Vec<u8>);
impl TryFrom<Vec<u8>> for ProfileImage {
    type Error = AccountError;
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if bytes.len() < 33
            || bytes.len() > 262_144
            || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
            || &bytes[12..16] != b"IHDR"
            || bytes[8..12] != 13_u32.to_be_bytes()
        {
            return Err(AccountError::InvalidInput);
        }
        let dimension = |slice: &[u8]| u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
        if !(1..=512).contains(&dimension(&bytes[16..20]))
            || !(1..=512).contains(&dimension(&bytes[20..24]))
        {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self(bytes))
    }
}
impl From<ProfileImage> for Vec<u8> {
    fn from(image: ProfileImage) -> Self {
        image.0
    }
}
impl ProfileImage {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ProfileRecord", into = "ProfileRecord")]
pub struct PublicProfile(ProfileRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ProfileRecord {
    display_name: String,
    description: String,
    image: Option<ProfileImage>,
}
impl TryFrom<ProfileRecord> for PublicProfile {
    type Error = AccountError;
    fn try_from(record: ProfileRecord) -> Result<Self, Self::Error> {
        if record.display_name.trim().is_empty()
            || record.display_name.len() > 160
            || record.description.len() > 2048
            || record.display_name.chars().any(char::is_control)
            || record
                .description
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self(record))
    }
}
impl From<PublicProfile> for ProfileRecord {
    fn from(profile: PublicProfile) -> Self {
        profile.0
    }
}
impl PublicProfile {
    /// # Errors
    /// Rejects empty names, oversized text and unsupported control characters.
    pub fn new(
        display_name: String,
        description: String,
        image: Option<ProfileImage>,
    ) -> Result<Self, AccountError> {
        ProfileRecord {
            display_name,
            description,
            image,
        }
        .try_into()
    }
    pub fn display_name(&self) -> &str {
        &self.0.display_name
    }
    pub fn description(&self) -> &str {
        &self.0.description
    }
    pub fn image(&self) -> Option<&ProfileImage> {
        self.0.image.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "EntryRecord", into = "EntryRecord")]
pub struct DirectoryEntry(EntryRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct EntryRecord {
    address: ProtocolIdentity,
    mailbox: Option<MemberAddress>,
    revision: DirectoryRevision,
    profile: PublicProfile,
    visibility: DiscoveryVisibility,
}
impl TryFrom<EntryRecord> for DirectoryEntry {
    type Error = AccountError;
    fn try_from(record: EntryRecord) -> Result<Self, Self::Error> {
        if record.address.0 == 0 || record.revision.0 == 0 {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self(record))
    }
}
impl From<DirectoryEntry> for EntryRecord {
    fn from(entry: DirectoryEntry) -> Self {
        entry.0
    }
}
impl DirectoryEntry {
    /// Deterministic replacement; persistence protects the revision and address ownership.
    /// # Errors
    /// Rejects a stale revision, wrong address or revision overflow.
    pub fn replace(
        current: Option<&Self>,
        address: ProtocolIdentity,
        expected: DirectoryRevision,
        profile: PublicProfile,
        visibility: DiscoveryVisibility,
        mailbox: Option<MemberAddress>,
    ) -> Result<Self, AccountError> {
        if current.map_or(DirectoryRevision(0), Self::revision) != expected
            || current.is_some_and(|entry| entry.address() != address)
            || current
                .and_then(Self::mailbox)
                .is_some_and(|old| Some(old) != mailbox.as_ref())
        {
            return Err(AccountError::InvalidInput);
        }
        EntryRecord {
            address,
            mailbox,
            revision: DirectoryRevision(
                expected
                    .0
                    .checked_add(1)
                    .ok_or(AccountError::InvalidInput)?,
            ),
            profile,
            visibility,
        }
        .try_into()
    }
    pub const fn address(&self) -> ProtocolIdentity {
        self.0.address
    }
    pub fn mailbox(&self) -> Option<&MemberAddress> {
        self.0.mailbox.as_ref()
    }
    pub const fn revision(&self) -> DirectoryRevision {
        self.0.revision
    }
    pub fn profile(&self) -> &PublicProfile {
        &self.0.profile
    }
    pub const fn visibility(&self) -> DiscoveryVisibility {
        self.0.visibility
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "QueryRecord", into = "QueryRecord")]
pub struct DirectoryQuery(QueryRecord);
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct QueryRecord {
    text: String,
    offset: u32,
}
impl TryFrom<QueryRecord> for DirectoryQuery {
    type Error = AccountError;
    fn try_from(record: QueryRecord) -> Result<Self, Self::Error> {
        if record.text.len() > 160
            || record.text.chars().any(char::is_control)
            || record.offset > 10_000
        {
            return Err(AccountError::InvalidInput);
        }
        Ok(Self(record))
    }
}
impl From<DirectoryQuery> for QueryRecord {
    fn from(query: DirectoryQuery) -> Self {
        query.0
    }
}
impl DirectoryQuery {
    /// # Errors
    /// Rejects oversized queries and pagination beyond the bounded first slice.
    pub fn new(text: String, offset: u32) -> Result<Self, AccountError> {
        QueryRecord { text, offset }.try_into()
    }
    pub fn text(&self) -> &str {
        self.0.text.trim()
    }
    pub const fn offset(&self) -> u32 {
        self.0.offset
    }
}
