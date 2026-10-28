use crate::{Error, SourceIdentity, SourceVersion};
use cs_mail_primitives::CanonicalTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct IndexGeneration(u64);
impl TryFrom<u64> for IndexGeneration {
    type Error = Error;
    fn try_from(value: u64) -> Result<Self, Error> {
        if value == 0 {
            Err(Error::Invalid)
        } else {
            Ok(Self(value))
        }
    }
}
impl From<IndexGeneration> for u64 {
    fn from(v: IndexGeneration) -> Self {
        v.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceStatus {
    Retained { expires_at: CanonicalTime },
    PrivateDraft,
    Removed { removed_at: CanonicalTime },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SourceState", into = "SourceState")]
pub struct DerivedSource(SourceState);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceState {
    identity: SourceIdentity,
    version: SourceVersion,
    generation: IndexGeneration,
    status: SourceStatus,
}
impl TryFrom<SourceState> for DerivedSource {
    type Error = Error;
    fn try_from(v: SourceState) -> Result<Self, Error> {
        if !v.version.matches(&v.identity)
            || !matches!(
                (v.version, v.status),
                (_, SourceStatus::Removed { .. })
                    | (SourceVersion::Message(_), SourceStatus::Retained { .. })
                    | (
                        SourceVersion::PrivateDraft { .. },
                        SourceStatus::PrivateDraft
                    )
            )
        {
            return Err(Error::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<DerivedSource> for SourceState {
    fn from(v: DerivedSource) -> Self {
        v.0
    }
}

/// Storage must atomically commit `next` and `work`; I/O occurs afterward.
#[derive(Clone, Debug)]
pub struct DerivedChange {
    next: DerivedSource,
    work: WorkIntent,
}
impl DerivedChange {
    pub const fn next(&self) -> &DerivedSource {
        &self.next
    }
    pub const fn work(&self) -> &WorkIntent {
        &self.work
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkIntent {
    ExtractAndSynchronize {
        generation: IndexGeneration,
    },
    RemoveAndSynchronize {
        generation: IndexGeneration,
        backup_deadline: CanonicalTime,
    },
}
impl DerivedSource {
    /// Rehydrate an already established source projection; this declares no new work.
    /// # Errors
    /// Inconsistent source identity, version or status.
    pub fn restored(
        identity: SourceIdentity,
        version: SourceVersion,
        generation: IndexGeneration,
        status: SourceStatus,
    ) -> Result<Self, Error> {
        SourceState {
            identity,
            version,
            generation,
            status,
        }
        .try_into()
    }
    /// Establish a derived view only from explicit facts supplied by the source owner.
    /// # Errors
    /// The source/version/status meanings do not match.
    pub fn observed(
        identity: SourceIdentity,
        version: SourceVersion,
        status: SourceStatus,
    ) -> Result<DerivedChange, Error> {
        let next: Self = SourceState {
            identity,
            version,
            generation: IndexGeneration(1),
            status,
        }
        .try_into()?;
        next.change()
    }
    fn change(self) -> Result<DerivedChange, Error> {
        let generation = self.0.generation;
        let work = match self.0.status {
            SourceStatus::Removed { removed_at } => WorkIntent::RemoveAndSynchronize {
                generation,
                backup_deadline: crate::backup_removal_deadline(removed_at)?,
            },
            _ => WorkIntent::ExtractAndSynchronize { generation },
        };
        Ok(DerivedChange { next: self, work })
    }
    /// Advance under the source owner's concurrency protection. A removed identity is terminal.
    /// # Errors
    /// Stale generation, attempted revival, inconsistent source facts or exhausted generation.
    pub fn advance(
        &self,
        expected: IndexGeneration,
        version: SourceVersion,
        status: SourceStatus,
    ) -> Result<DerivedChange, Error> {
        if self.0.generation != expected {
            return Err(Error::Stale);
        }
        if matches!(self.0.status, SourceStatus::Removed { .. }) {
            return Err(Error::Removed);
        }
        if let (
            SourceVersion::PrivateDraft {
                revision: previous, ..
            },
            SourceVersion::PrivateDraft { revision: next, .. },
        ) = (self.0.version, version)
        {
            if !matches!(status, SourceStatus::Removed { .. }) && next <= previous {
                return Err(Error::Stale);
            }
        }
        if matches!(status, SourceStatus::Removed { .. }) && version != self.0.version {
            return Err(Error::Stale);
        }
        let next: Self = SourceState {
            identity: self.0.identity.clone(),
            version,
            generation: IndexGeneration(self.0.generation.0.checked_add(1).ok_or(Error::Invalid)?),
            status,
        }
        .try_into()?;
        next.change()
    }
    pub const fn generation(&self) -> IndexGeneration {
        self.0.generation
    }
    pub const fn identity(&self) -> &SourceIdentity {
        &self.0.identity
    }
    pub const fn version(&self) -> SourceVersion {
        self.0.version
    }
    /// Conditional publication checks current source facts; readable content is still
    /// required separately, and publication confers no reuse permission.
    /// # Errors
    /// Removed, expired or superseded source generation/version.
    pub fn allow_publication(
        &self,
        identity: &SourceIdentity,
        version: SourceVersion,
        generation: IndexGeneration,
        now: CanonicalTime,
    ) -> Result<(), Error> {
        if &self.0.identity != identity
            || self.0.version != version
            || self.0.generation != generation
        {
            return Err(Error::Stale);
        }
        match self.0.status {
            SourceStatus::Removed { .. } => Err(Error::Removed),
            SourceStatus::Retained { expires_at } if now >= expires_at => Err(Error::Expired),
            _ => Ok(()),
        }
    }
}
