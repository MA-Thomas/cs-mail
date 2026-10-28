//! Independently protected copies and index records, with a session-owned inverted index.
use super::retained::{CopyKey, Entry};
use super::{
    Error, MailKey, MemberClient, State,
    storage::{
        self, RetainedCommit, RetainedCopyWrite, RetainedIndexRow, RetainedRecordId, RetainedWrite,
        Secrets,
    },
};
use cs_mail_correspondence::ConversationSubject;
use cs_mail_primitives::{AccountId, CanonicalTime, ProtocolIdentity};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
    sync::MutexGuard,
};
const META: &[u8] = b"cs-mail/retained-metadata/v3";
fn context(kind: &[u8], id: RetainedRecordId) -> Vec<u8> {
    [kind, id.bytes()].concat()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    account: AccountId,
    owner: ProtocolIdentity,
    aliases: Vec<cs_mail_accounts::aliases::Alias>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Indexed {
    key: CopyKey,
    pub subject: Option<ConversationSubject>,
    pub expires: CanonicalTime,
    pub content_digest: [u8; 32],
    pub projection: super::semantic::Projection,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum IndexRecord {
    Available(Indexed),
    Excluded(CopyKey),
}
impl IndexRecord {
    fn key(&self) -> CopyKey {
        match self {
            Self::Available(v) => v.key,
            Self::Excluded(k) => *k,
        }
    }
}
enum PendingRecord {
    Replace(Box<Entry>),
    Reindex(Indexed),
    Exclude,
}

pub(super) struct Retained {
    epoch: u64,
    metadata: Metadata,
    revision: u64,
    initialized: bool,
    reload_required: bool,
    key_tag: [u8; 32],
    pub entries: BTreeMap<CopyKey, Indexed>,
    pub versions: BTreeMap<CopyKey, cs_mail_correspondence::MessageVersion>,
    pub excluded: BTreeSet<CopyKey>,
    by_thread: BTreeMap<MailKey, BTreeSet<CopyKey>>,
    pending: BTreeMap<CopyKey, PendingRecord>,
    pub known_keys: BTreeSet<CopyKey>,
    pub received_keys: BTreeMap<cs_mail_primitives::AliasId, BTreeSet<CopyKey>>,
    observed: (usize, usize),
    pub missing_native: BTreeMap<CopyKey, usize>,
    pub missing_external: BTreeMap<CopyKey, (u128, usize)>,
    deadlines: BTreeMap<CanonicalTime, BTreeSet<CopyKey>>,
    suspended: BTreeSet<cs_mail_correspondence::ConversationId>,
}
pub(super) struct Access<'a>(MutexGuard<'a, Option<Retained>>, &'a MemberClient, u64);
impl Deref for Access<'_> {
    type Target = Retained;
    fn deref(&self) -> &Retained {
        self.0.as_ref().expect("initialized retained index")
    }
}
impl DerefMut for Access<'_> {
    fn deref_mut(&mut self) -> &mut Retained {
        self.0.as_mut().expect("initialized retained index")
    }
}
impl Drop for Access<'_> {
    fn drop(&mut self) {
        if self
            .1
            .retained_epoch
            .load(std::sync::atomic::Ordering::SeqCst)
            != self.2
            || self.0.as_ref().is_some_and(|index| index.reload_required)
        {
            *self.0 = None;
            if self
                .1
                .retained_epoch
                .load(std::sync::atomic::Ordering::SeqCst)
                != self.2
            {
                if let Ok(mut query) = self.1.search.try_lock() {
                    *query = None;
                }
            }
        } else if let Some(index) = self.0.as_mut() {
            index.pending.clear();
        }
    }
}
impl Retained {
    pub fn load<'a>(
        client: &'a MemberClient,
        keys: &Secrets,
        state: &State,
    ) -> Result<Access<'a>, Error> {
        Self::load_at_epoch(client, keys, state, None)
    }
    pub fn load_at_epoch<'a>(
        client: &'a MemberClient,
        keys: &Secrets,
        state: &State,
        epoch: Option<u64>,
    ) -> Result<Access<'a>, Error> {
        let session = client.search_session()?;
        let mut guard = client.retained.lock().map_err(|_| Error::Unavailable)?;
        if epoch.is_some_and(|value| value != session.value()) {
            return Err(Error::Unavailable);
        }
        client.check_search_session(session)?;
        let tag = Sha256::digest(keys.cache).into();
        if guard.is_none() {
            let (revision, bytes) = client.store.retained_state()?;
            let metadata = match &bytes {
                Some(bytes) => serde_json::from_slice(&storage::decrypt(keys, bytes, META)?)
                    .map_err(|_| Error::InvalidData)?,
                None if revision == 0 => Metadata {
                    account: state.profile.account,
                    owner: state.profile.persona,
                    aliases: Vec::new(),
                },
                None => return Err(Error::InvalidData),
            };
            let mut value = Self {
                epoch: session.value(),
                metadata,
                revision,
                initialized: bytes.is_some(),
                reload_required: false,
                key_tag: tag,
                entries: BTreeMap::new(),
                versions: BTreeMap::new(),
                excluded: BTreeSet::new(),
                by_thread: BTreeMap::new(),
                pending: BTreeMap::new(),
                known_keys: BTreeSet::new(),
                received_keys: BTreeMap::new(),
                observed: (usize::MAX, usize::MAX),
                missing_native: BTreeMap::new(),
                missing_external: BTreeMap::new(),
                deadlines: BTreeMap::new(),
                suspended: BTreeSet::new(),
            };
            value.check(state, tag)?;
            value.validate_metadata()?;
            let mut after = None;
            loop {
                let rows = client.store.retained_indexes(after, 256)?;
                if rows.is_empty() {
                    break;
                }
                if !value.initialized {
                    return Err(Error::InvalidData);
                }
                for row in rows {
                    if after.is_some_and(|id| row.id <= id) {
                        return Err(Error::InvalidData);
                    }
                    let record: IndexRecord = serde_json::from_slice(&storage::decrypt(
                        keys,
                        &row.ciphertext,
                        &context(b"cs-mail/retained-index/v3", row.id),
                    )?)
                    .map_err(|_| Error::InvalidData)?;
                    if value.id(keys, record.key())? != row.id || value.known(record.key()) {
                        return Err(Error::InvalidData);
                    }
                    if let IndexRecord::Available(v) = &record {
                        v.projection.validate()?;
                        if v.subject.is_some() != matches!(v.key.thread, MailKey::Native(_)) {
                            return Err(Error::InvalidData);
                        }
                    }
                    value.publish(record);
                    after = Some(row.id);
                }
            }
            *guard = Some(value);
        }
        let value = guard.as_mut().ok_or(Error::InvalidData)?;
        value.check(state, tag)?;
        value.observe_headers(state);
        let access = Access(guard, client, session.value());
        client.check_search_session(session)?;
        Ok(access)
    }
    fn validate_metadata(&self) -> Result<(), Error> {
        self.inboxes().map(|_| ())
    }
    fn observe_headers(&mut self, state: &State) {
        // Pending deletion is an explicit recoverable suspension, not proof of deletion.
        // It survives a failed exclusion checkpoint because its signed intent is already durable.
        if let Some(signed) = &state.editing_conversation {
            if let cs_mail_application::correspondence::Command::DeleteCopies { id } =
                signed.command
            {
                self.suspended.insert(id);
            }
        }
        let external_count = state
            .external
            .as_ref()
            .map_or(0, |e| e.records.values().map(|r| r.messages.len()).sum());
        let observed = (state.mailbox.records.len(), external_count);
        if self.observed != observed {
            self.known_keys.clear();
            self.versions.clear();
            self.received_keys.clear();
            self.missing_native.clear();
            self.missing_external.clear();
            for (position, (_, record)) in state.mailbox.records.iter().enumerate() {
                let key = CopyKey {
                    thread: MailKey::Native(record.manifest.conversation()),
                    message: record.manifest.message(),
                };
                self.known_keys.insert(key);
                self.versions.insert(key, record.manifest.version());
                if record.receiving.recipient() == self.metadata.owner {
                    self.received_keys
                        .entry(record.receiving.alias())
                        .or_default()
                        .insert(key);
                }
                if !self.known(key) {
                    self.missing_native.insert(key, position);
                }
            }
            if let Some(external) = &state.external {
                for (id, record) in &external.records {
                    for (position, view) in record.messages.iter().enumerate() {
                        let key = CopyKey {
                            thread: MailKey::External(record.conversation.id()),
                            message: view.message.manifest.message(),
                        };
                        self.known_keys.insert(key);
                        self.versions.insert(key, view.message.manifest.version());
                        if let cs_mail_application::correspondence::external::Direction::GuestToMember(route) = &view.message.direction {
                            self.received_keys.entry(route.alias()).or_default().insert(key);
                        }
                        if !self.known(key) {
                            self.missing_external.insert(key, (*id, position));
                        }
                    }
                }
            }
            self.observed = observed;
        }
    }
    pub fn session<'a>(
        client: &'a MemberClient,
        keys: &Secrets,
        session: super::SearchSessionId,
    ) -> Result<Access<'a>, Error> {
        let guard = client.retained.lock().map_err(|_| Error::Unavailable)?;
        client.check_search_session(session)?;
        if let Some(index) = guard.as_ref() {
            if index.key_tag != <[u8; 32]>::from(Sha256::digest(keys.cache)) {
                return Err(Error::KeyUnavailable);
            }
            return Ok(Access(guard, client, session.value()));
        }
        drop(guard);
        let state = storage::open(client.store.as_ref(), keys)?;
        Self::load_at_epoch(client, keys, &state, Some(session.value()))
    }
    fn check(&self, state: &State, tag: [u8; 32]) -> Result<(), Error> {
        if self.metadata.account != state.profile.account
            || self.metadata.owner != state.profile.persona
            || self.key_tag != tag
        {
            return Err(Error::InvalidData);
        }
        Ok(())
    }
    fn id(&self, keys: &Secrets, key: CopyKey) -> Result<RetainedRecordId, Error> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&keys.cache).map_err(|_| Error::InvalidData)?;
        mac.update(b"cs-mail/retained-record-id/v2");
        mac.update(
            &serde_json::to_vec(&(self.metadata.account, self.metadata.owner, key))
                .map_err(|_| Error::InvalidData)?,
        );
        RetainedRecordId::try_from(<[u8; 32]>::from(mac.finalize().into_bytes()))
    }
    pub fn inboxes(&self) -> Result<super::inboxes::InboxCatalog, Error> {
        super::inboxes::InboxCatalog::new(
            self.metadata.account,
            self.metadata.owner,
            &self.metadata.aliases,
        )
    }
    pub fn aliases(&self) -> Vec<cs_mail_accounts::aliases::Alias> {
        self.metadata.aliases.clone()
    }
    pub fn set_aliases(
        &mut self,
        client: &MemberClient,
        keys: &Secrets,
        aliases: Vec<cs_mail_accounts::aliases::Alias>,
    ) -> Result<(), Error> {
        let next = self.revision.checked_add(1).ok_or(Error::InvalidData)?;
        let mut meta = self.metadata.clone();
        meta.aliases = aliases;
        let bytes = storage::encrypt(keys, &meta, META)?;
        storage::check_keys(client.store.as_ref(), keys)?;
        self.commit(
            client,
            RetainedCommit {
                expected_revision: self.revision,
                metadata: Some(bytes),
                writes: Vec::new(),
            },
        )?;
        self.revision = next;
        self.metadata = meta;
        self.initialized = true;
        Ok(())
    }
    pub fn entry(&self, key: CopyKey, now: CanonicalTime) -> Option<&Indexed> {
        if matches!(key.thread, MailKey::Native(id) if self.suspended.contains(&id)) {
            return None;
        }
        self.entries.get(&key).filter(|e| now < e.expires)
    }
    pub fn source(
        &self,
        key: CopyKey,
    ) -> Result<
        (
            cs_mail_search::SourceIdentity,
            cs_mail_search::SourceVersion,
        ),
        Error,
    > {
        use cs_mail_search::{
            CorrespondenceStream, SourceIdentity, SourceOccurrence, SourceVersion,
        };
        let (stream, conversation) = match key.thread {
            MailKey::Native(id) => (CorrespondenceStream::Native, id),
            MailKey::External(id) => (CorrespondenceStream::External, id),
        };
        let identity = SourceIdentity::new(
            self.metadata.account,
            SourceOccurrence::Message {
                stream,
                conversation,
                message: key.message,
            },
        )
        .map_err(|_| Error::InvalidData)?;
        let version = *self.versions.get(&key).ok_or(Error::InvalidData)?;
        Ok((identity, SourceVersion::Message(version)))
    }
    pub fn suspend(&mut self, id: cs_mail_correspondence::ConversationId) {
        self.suspended.insert(id);
    }
    pub fn resume(&mut self, id: cs_mail_correspondence::ConversationId) {
        self.suspended.remove(&id);
    }
    pub fn known(&self, key: CopyKey) -> bool {
        self.entries.contains_key(&key)
            || self.excluded.contains(&key)
            || self.pending.contains_key(&key)
    }
    pub fn subject(&self, thread: MailKey, now: CanonicalTime) -> Option<&ConversationSubject> {
        self.by_thread
            .get(&thread)?
            .iter()
            .find_map(|k| self.entry(*k, now)?.subject.as_ref())
    }
    pub fn insert(&mut self, entry: Entry) {
        if !self.excluded.contains(&entry.key) {
            self.pending
                .insert(entry.key, PendingRecord::Replace(Box::new(entry)));
        }
    }
    pub fn reindex(&mut self, entry: &Entry) -> Result<(), Error> {
        let indexed = Self::indexed(entry)?;
        self.pending
            .insert(entry.key, PendingRecord::Reindex(indexed));
        Ok(())
    }
    pub fn exclude(&mut self, key: CopyKey) {
        self.pending.insert(key, PendingRecord::Exclude);
    }
    /// Exclusion takes effect by time before this bounded cleanup removes the
    /// sensitive derived words and retained copy in one checkpoint.
    pub fn expire(&mut self, now: CanonicalTime, limit: usize) -> usize {
        let expired: Vec<_> = self
            .deadlines
            .range(..=now)
            .flat_map(|(_, keys)| keys.iter())
            .take(limit)
            .copied()
            .collect();
        let count = expired.len();
        for key in expired {
            self.exclude(key);
        }
        count
    }
    fn publish(&mut self, record: IndexRecord) {
        let key = record.key();
        self.missing_native.remove(&key);
        self.missing_external.remove(&key);
        if let Some(old) = self.entries.remove(&key) {
            if let Some(keys) = self.deadlines.get_mut(&old.expires) {
                keys.remove(&key);
                if keys.is_empty() {
                    self.deadlines.remove(&old.expires);
                }
            }
        }
        if let Some(set) = self.by_thread.get_mut(&key.thread) {
            set.remove(&key);
            if set.is_empty() {
                self.by_thread.remove(&key.thread);
            }
        }
        self.excluded.remove(&key);
        match record {
            IndexRecord::Excluded(k) => {
                self.excluded.insert(k);
            }
            IndexRecord::Available(v) => {
                self.deadlines.entry(v.expires).or_default().insert(key);

                self.by_thread.entry(key.thread).or_default().insert(key);
                self.entries.insert(key, v);
            }
        }
    }
    fn indexed(entry: &Entry) -> Result<Indexed, Error> {
        if entry.native.is_some() != matches!(entry.key.thread, MailKey::Native(_))
            || entry.subject.is_some() != matches!(entry.key.thread, MailKey::Native(_))
            || entry.native.as_ref().is_some_and(|v| {
                v.record.manifest.message() != entry.key.message
                    || MailKey::Native(v.record.manifest.conversation()) != entry.key.thread
                    || v.ciphertext.expires_at != entry.expires
                    || v.record.manifest.subject()
                        != entry.subject.as_ref().map(ConversationSubject::commitment)
            })
        {
            return Err(Error::InvalidData);
        }
        Ok(Indexed {
            key: entry.key,
            subject: entry.subject.clone(),
            expires: entry.expires,
            content_digest: super::semantic::content_digest(&entry.text),
            projection: super::semantic::Projection::Pending,
        })
    }
    fn commit(&mut self, client: &MemberClient, change: RetainedCommit) -> Result<(), Error> {
        client.check_search_session(super::SearchSessionId::try_from(self.epoch)?)?;
        if self.reload_required {
            return Err(Error::StorageUnavailable);
        }
        match client.store.commit_retained(change) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.reload_required = true;
                Err(error)
            }
        }
    }
    pub fn save(&mut self, client: &MemberClient, keys: &Secrets) -> Result<(), Error> {
        let pending = std::mem::take(&mut self.pending);
        let invalidates_projection = pending
            .values()
            .any(|p| !matches!(p, PendingRecord::Reindex(_)));
        let mut records = Vec::new();
        let mut writes = Vec::new();
        for (key, entry) in pending {
            let id = self.id(keys, key)?;
            let (record, copy) = match entry {
                PendingRecord::Replace(entry) => (
                    IndexRecord::Available(Self::indexed(&entry)?),
                    RetainedCopyWrite::Replace(storage::encrypt(
                        keys,
                        &entry,
                        &context(b"cs-mail/retained-copy/v2", id),
                    )?),
                ),
                PendingRecord::Reindex(indexed) => {
                    (IndexRecord::Available(indexed), RetainedCopyWrite::Keep)
                }
                PendingRecord::Exclude => (IndexRecord::Excluded(key), RetainedCopyWrite::Remove),
            };
            writes.push(RetainedWrite {
                index: RetainedIndexRow {
                    id,
                    ciphertext: storage::encrypt(
                        keys,
                        &record,
                        &context(b"cs-mail/retained-index/v3", id),
                    )?,
                },
                copy,
            });
            records.push(record);
        }
        let metadata = if self.initialized {
            None
        } else {
            Some(storage::encrypt(keys, &self.metadata, META)?)
        };
        let next = self.revision.checked_add(1).ok_or(Error::InvalidData)?;
        storage::check_keys(client.store.as_ref(), keys)?;
        self.commit(
            client,
            RetainedCommit {
                expected_revision: self.revision,
                metadata,
                writes,
            },
        )?;
        if invalidates_projection {
            if let Some(runtime) = &client.retrieval {
                runtime.interrupt();
            }
        }
        for record in records {
            self.publish(record);
        }
        self.initialized = true;
        self.revision = next;
        Ok(())
    }
    pub fn copy(
        &self,
        client: &MemberClient,
        keys: &Secrets,
        key: CopyKey,
        now: CanonicalTime,
    ) -> Result<Option<Entry>, Error> {
        let Some(indexed) = self.entry(key, now) else {
            return Ok(None);
        };
        let id = self.id(keys, key)?;
        let bytes = client.store.retained_copy(id)?.ok_or(Error::InvalidData)?;
        let entry: Entry = serde_json::from_slice(&storage::decrypt(
            keys,
            &bytes,
            &context(b"cs-mail/retained-copy/v2", id),
        )?)
        .map_err(|_| Error::InvalidData)?;
        if entry.key != key
            || entry.subject != indexed.subject
            || entry.expires != indexed.expires
            || super::semantic::content_digest(&entry.text) != indexed.content_digest
            || entry
                .native
                .as_ref()
                .is_some_and(|v| v.ciphertext.binding.recipient != self.metadata.owner)
        {
            return Err(Error::InvalidData);
        }
        Ok(Some(entry))
    }
    pub fn progress(&self, now: CanonicalTime) -> super::retained::RetentionProgress {
        let expired: usize = self
            .deadlines
            .range(..=now)
            .map(|(_, keys)| keys.len())
            .sum();
        let suspended = self
            .suspended
            .iter()
            .filter_map(|id| self.by_thread.get(&MailKey::Native(*id)))
            .flatten()
            .filter(|key| {
                self.entries
                    .get(key)
                    .is_some_and(|entry| now < entry.expires)
            })
            .count();
        super::retained::RetentionProgress {
            retained: (self.entries.len() - expired - suspended) as u64,
            unavailable: (self.excluded.len() + expired + suspended) as u64,
            remaining: (self.missing_native.len() + self.missing_external.len()) as u64,
        }
    }
    pub fn project(
        &mut self,
        key: CopyKey,
        projection: super::semantic::Projection,
    ) -> Result<(), Error> {
        projection.validate()?;
        let mut indexed = self
            .entries
            .get(&key)
            .ok_or(Error::SourceUnavailable)?
            .clone();
        indexed.projection = projection;
        self.pending.insert(key, PendingRecord::Reindex(indexed));
        Ok(())
    }
}
