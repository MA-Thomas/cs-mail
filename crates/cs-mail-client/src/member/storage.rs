//! Device persistence: only an authenticated encrypted snapshot crosses the cache port.
use super::{Error, State, enrollment::Enrollment};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const CONTEXT: &[u8] = b"cs-mail/apple-member-cache/v5";

/// Opaque storage address; it is neither a message identity nor an encryption key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RetainedRecordId([u8; 32]);
impl TryFrom<[u8; 32]> for RetainedRecordId {
    type Error = Error;
    fn try_from(bytes: [u8; 32]) -> Result<Self, Error> {
        if bytes == [0; 32] {
            Err(Error::InvalidData)
        } else {
            Ok(Self(bytes))
        }
    }
}
impl TryFrom<Vec<u8>> for RetainedRecordId {
    type Error = Error;
    fn try_from(bytes: Vec<u8>) -> Result<Self, Error> {
        Self::try_from(<[u8; 32]>::try_from(bytes).map_err(|_| Error::InvalidData)?)
    }
}
impl RetainedRecordId {
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Adapter records carry ciphertext and opaque identifiers; Rust owns their meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedIndexRow {
    pub id: RetainedRecordId,
    pub ciphertext: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedCopyWrite {
    Keep,
    Replace(Vec<u8>),
    Remove,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedWrite {
    pub index: RetainedIndexRow,
    pub copy: RetainedCopyWrite,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedCommit {
    pub expected_revision: u64,
    pub metadata: Option<Vec<u8>>,
    pub writes: Vec<RetainedWrite>,
}

/// The host must hold exclusive ownership of this store for the client's lifetime.
/// Cache replacement must be durable and atomic, including on process termination.
pub trait DeviceStore: Send + Sync {
    /// Independently encrypted metadata and its committed progress revision.
    /// # Errors
    /// Retained storage cannot be read.
    fn retained_state(&self) -> Result<(u64, Option<Vec<u8>>), Error>;
    /// Read only encrypted index records, never message copies. Cursor is an opaque record ID.
    /// # Errors
    /// Invalid cursor or unavailable storage.
    fn retained_indexes(
        &self,
        after: Option<RetainedRecordId>,
        limit: u16,
    ) -> Result<Vec<RetainedIndexRow>, Error>;
    /// Fetch one independently encrypted copy by its opaque record ID.
    /// # Errors
    /// Unavailable storage.
    fn retained_copy(&self, id: RetainedRecordId) -> Result<Option<Vec<u8>>, Error>;
    /// Atomically commit affected records and advance progress; reject a stale revision.
    /// # Errors
    /// Conflicting revision or failed transaction; no partial writes may remain.
    fn commit_retained(&self, change: RetainedCommit) -> Result<(), Error>;
    /// # Errors
    /// Locked, denied or unavailable key custody must fail, never return an empty key.
    fn load_secret(&self) -> Result<Vec<u8>, Error>;
    /// Creates a new credential. Must refuse replacing an existing credential.
    /// # Errors
    /// Key custody is unavailable or the credential already exists.
    fn create_secret(&self, bytes: &[u8]) -> Result<(), Error>;
    /// # Errors
    /// Local persistence cannot be read.
    fn load_cache(&self) -> Result<Option<Vec<u8>>, Error>;
    /// # Errors
    /// The atomic durable cache commit failed.
    fn save_cache(&self, bytes: &[u8]) -> Result<(), Error>;
    /// Independent encrypted local writing; absence means no drafts have been created.
    /// # Errors
    /// The workspace cannot be read.
    fn load_workspace(&self) -> Result<Option<Vec<u8>>, Error>;
    /// # Errors
    /// Atomic durable workspace replacement failed.
    fn save_workspace(&self, bytes: &[u8]) -> Result<(), Error>;
    /// Commit prepared operation and consumed draft together, or neither. No I/O to providers.
    /// # Errors
    /// The complete local transaction could not commit.
    fn save_submission(&self, cache: &[u8], workspace: &[u8], personal: &[u8])
    -> Result<(), Error>;
    /// # Errors
    /// Personal-state storage cannot be read.
    fn load_personal(&self) -> Result<Option<Vec<u8>>, Error>;
    /// # Errors
    /// Atomic personal-state replacement failed.
    fn save_personal(&self, bytes: &[u8]) -> Result<(), Error>;
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub(super) struct Secrets {
    pub device: [u8; 32],
    pub recipient: [u8; 32],
    pub content: [u8; 32],
    pub cache: [u8; 32],
    pub operation: String,
}

pub(super) fn secrets(store: &dyn DeviceStore) -> Result<Secrets, Error> {
    let bytes = Zeroizing::new(store.load_secret()?);
    serde_json::from_slice(&bytes).map_err(|_| Error::KeyUnavailable)
}

#[derive(Serialize, Deserialize)]
pub(super) enum DeviceState {
    Enrolling(Box<Enrollment>),
    Active(Box<State>),
}

pub(super) fn open(store: &dyn DeviceStore, keys: &Secrets) -> Result<State, Error> {
    match open_device(store, keys)? {
        DeviceState::Active(state) => Ok(*state),
        DeviceState::Enrolling(_) => Err(Error::EnrollmentRequired),
    }
}
pub(super) fn open_device(store: &dyn DeviceStore, keys: &Secrets) -> Result<DeviceState, Error> {
    let bytes = store.load_cache()?.ok_or(Error::NotConfigured)?;
    if bytes.len() < 40 || bytes.len() > super::MAX_CACHE {
        return Err(Error::InvalidData);
    }
    let cipher = XChaCha20Poly1305::new((&keys.cache).into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::try_from(&bytes[..24]).map_err(|_| Error::InvalidData)?,
                Payload {
                    msg: &bytes[24..],
                    aad: CONTEXT,
                },
            )
            .map_err(|_| Error::InvalidData)?,
    );
    let state: DeviceState = serde_json::from_slice(&plain).map_err(|_| Error::InvalidData)?;
    if matches!(&state, DeviceState::Active(s) if s.format != 5) {
        return Err(Error::InvalidData);
    }
    if let DeviceState::Active(active) = &state {
        validate_mailbox(active)?;
    }
    Ok(state)
}
fn validate_mailbox(state: &State) -> Result<(), Error> {
    let owner = state.profile.persona;
    for (_, record) in &state.mailbox.records {
        let other = state
            .mailbox
            .participants
            .get(&record.manifest.conversation().value())
            .ok_or(Error::InvalidData)?;
        if (record.author == owner && record.receiving.recipient() != *other)
            || (record.author != owner
                && (record.author != *other || record.receiving.recipient() != owner))
        {
            return Err(Error::InvalidData);
        }
    }
    if let Some(external) = &state.external {
        for (id, record) in &external.records {
            if record.conversation.member() != owner || record.conversation.id().value() != *id {
                return Err(Error::InvalidData);
            }
            for view in &record.messages {
                view.message
                    .validate(&record.conversation, &record.relationship)
                    .map_err(|_| Error::InvalidData)?;
            }
        }
    }
    Ok(())
}

pub(super) fn save(store: &dyn DeviceStore, keys: &Secrets, state: &State) -> Result<(), Error> {
    check_keys(store, keys)?;
    store.save_cache(&state_bytes(keys, state)?)
}
pub(super) fn state_bytes(keys: &Secrets, state: &State) -> Result<Vec<u8>, Error> {
    #[derive(Serialize)]
    enum ActiveRef<'a> {
        Active(&'a State),
    }
    encrypt(keys, &ActiveRef::Active(state), CONTEXT)
}
pub(super) fn save_value(
    store: &dyn DeviceStore,
    keys: &Secrets,
    state: &impl Serialize,
) -> Result<(), Error> {
    check_keys(store, keys)?;
    store.save_cache(&encrypt(keys, state, CONTEXT)?)
}
pub(super) fn check_keys(store: &dyn DeviceStore, keys: &Secrets) -> Result<(), Error> {
    let current = secrets(store)?;
    if current.cache != keys.cache {
        return Err(Error::KeyUnavailable);
    }
    Ok(())
}
pub(super) fn encrypt(
    keys: &Secrets,
    state: &impl Serialize,
    context: &[u8],
) -> Result<Vec<u8>, Error> {
    let plain = Zeroizing::new(serde_json::to_vec(state).map_err(|_| Error::InvalidData)?);
    if plain.len() > super::MAX_CACHE - 40 {
        return Err(Error::InvalidData);
    }
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
    let cipher = XChaCha20Poly1305::new((&keys.cache).into());
    let mut bytes = nonce.to_vec();
    bytes.extend(
        cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plain,
                    aad: context,
                },
            )
            .map_err(|_| Error::Unavailable)?,
    );
    Ok(bytes)
}
pub(super) fn decrypt(
    keys: &Secrets,
    bytes: &[u8],
    context: &[u8],
) -> Result<Zeroizing<Vec<u8>>, Error> {
    if bytes.len() < 40 || bytes.len() > super::MAX_CACHE {
        return Err(Error::InvalidData);
    }
    XChaCha20Poly1305::new((&keys.cache).into())
        .decrypt(
            &XNonce::try_from(&bytes[..24]).map_err(|_| Error::InvalidData)?,
            Payload {
                msg: &bytes[24..],
                aad: context,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::InvalidData)
}
