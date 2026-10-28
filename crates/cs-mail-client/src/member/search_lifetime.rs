//! An unlocked lifetime is distinct from a query and from access to the device credential.
use super::{Error, MemberClient, storage};
use std::sync::atomic::Ordering;

/// A caller must capture this before dispatching work. It cannot revive a closed lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchSessionId(u64);
impl TryFrom<u64> for SearchSessionId {
    type Error = Error;
    fn try_from(value: u64) -> Result<Self, Error> {
        if value % 2 == 1 && value < u64::MAX {
            Ok(Self(value))
        } else {
            Err(Error::InvalidData)
        }
    }
}
impl SearchSessionId {
    pub const fn value(self) -> u64 {
        self.0
    }
}
impl MemberClient {
    /// Capture the current unlocked lifetime before dispatching asynchronous work.
    /// # Errors
    /// The search lifetime is closed.
    pub fn search_session(&self) -> Result<SearchSessionId, Error> {
        SearchSessionId::try_from(self.retained_epoch.load(Ordering::SeqCst))
            .map_err(|_| Error::Unavailable)
    }
    pub(super) fn check_search_session(&self, session: SearchSessionId) -> Result<(), Error> {
        if self.search_session()? == session {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    /// Start an explicitly unlocked lifetime after checking existing device custody.
    /// No credential is created or replaced. Old dispatched work remains invalid.
    /// # Errors
    /// Unavailable custody, invalid account state, exhausted generation or poisoned locks.
    pub fn open_search_session(&self) -> Result<SearchSessionId, Error> {
        let expected = self.retained_epoch.load(Ordering::SeqCst);
        let next =
            SearchSessionId::try_from(expected.checked_add(2).ok_or(Error::Unavailable)? | 1)?;
        let _workspace = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        storage::open(self.store.as_ref(), &keys)?;
        let mut retained = self.retained.lock().map_err(|_| Error::Unavailable)?;
        let mut query = self.search.lock().map_err(|_| Error::Unavailable)?;
        let mut receiving = self.receiving.lock().map_err(|_| Error::Unavailable)?;
        self.retained_epoch
            .compare_exchange(expected, next.value(), Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Error::Unavailable)?;
        if let Some(runtime) = &self.retrieval {
            runtime.interrupt_before(next.value());
        }
        *retained = None;
        *query = None;
        *receiving = None;
        Ok(next)
    }
    /// The user's immediate lock action also cancels an unlock paused in custody
    /// or storage. Unlike stale worker cleanup, it targets the current lifetime.
    /// # Errors
    /// The generation space is exhausted (the lifetime remains closed).
    pub fn invalidate_search_session(&self) -> Result<(), Error> {
        let prior = self
            .retained_epoch
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(if n % 2 == 1 { 1 } else { 2 })
            })
            .map_err(|_| Error::Unavailable)?;
        self.release_search_handles(prior + if prior % 2 == 1 { 1 } else { 2 });
        Ok(())
    }
    /// Invalidate immediately without waiting on network, extraction, or an index lock.
    /// A holder of the old index releases it on drop; late queries cannot reopen it.
    /// Cleanup from an earlier lifetime cannot close a newly unlocked lifetime.
    /// # Errors
    /// The generation space is exhausted.
    pub fn close_search_session(&self, session: SearchSessionId) -> Result<(), Error> {
        let next = session.0.checked_add(1).ok_or(Error::Unavailable)?;
        if self
            .retained_epoch
            .compare_exchange(session.0, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.release_search_handles(next);
        }
        Ok(())
    }
    fn release_search_handles(&self, next: u64) {
        if let Some(runtime) = &self.retrieval {
            runtime.interrupt_before(next);
        }
        if let Ok(mut value) = self.retained.try_lock() {
            if self.retained_epoch.load(Ordering::SeqCst) == next {
                *value = None;
            }
        }
        if let Ok(mut value) = self.search.try_lock() {
            if self.retained_epoch.load(Ordering::SeqCst) == next {
                *value = None;
            }
        }
        if let Ok(mut value) = self.receiving.try_lock() {
            if self.retained_epoch.load(Ordering::SeqCst) == next {
                *value = None;
            }
        }
    }
}
