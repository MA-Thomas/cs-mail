//! Device persistence: only an authenticated encrypted snapshot crosses the cache port.
use super::{Error, State, enrollment::Enrollment};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const CONTEXT: &[u8] = b"cs-mail/apple-member-cache/v1";

/// The host must hold exclusive ownership of this store for the client's lifetime.
/// Cache replacement must be durable and atomic, including on process termination.
pub trait DeviceStore: Send + Sync {
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
    if matches!(&state, DeviceState::Active(s) if s.format != 1) {
        return Err(Error::InvalidData);
    }
    Ok(state)
}

pub(super) fn save(store: &dyn DeviceStore, keys: &Secrets, state: &State) -> Result<(), Error> {
    #[derive(Serialize)]
    enum ActiveRef<'a> {
        Active(&'a State),
    }
    save_value(store, keys, &ActiveRef::Active(state))
}
pub(super) fn save_value(
    store: &dyn DeviceStore,
    keys: &Secrets,
    state: &impl Serialize,
) -> Result<(), Error> {
    // Recheck key access after intervening network work before making local progress durable.
    let current = secrets(store)?;
    if current.cache != keys.cache {
        return Err(Error::KeyUnavailable);
    }
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
                    aad: CONTEXT,
                },
            )
            .map_err(|_| Error::Unavailable)?,
    );
    store.save_cache(&bytes)
}
