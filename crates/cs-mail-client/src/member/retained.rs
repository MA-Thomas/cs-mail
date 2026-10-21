//! Device-retained correspondence and a rebuildable word index share one encrypted commit.
use super::{
    Error, MailKey, MemberClient, State,
    storage::{self, Secrets},
};
use cs_mail_application::correspondence::{Command, MessageView, Response};
use cs_mail_correspondence::{ConversationSubject, Document};
use cs_mail_primitives::{CanonicalTime, MessageId, ProtocolIdentity};
use serde::{Deserialize, Serialize};

pub(super) use super::retained_index::Retained;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub(super) struct CopyKey {
    pub thread: MailKey,
    pub message: MessageId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub key: CopyKey,
    pub native: Option<MessageView>,
    pub text: String,
    pub subject: Option<ConversationSubject>,
    pub expires: CanonicalTime,
}
#[derive(Clone, Debug)]
pub struct RetentionProgress {
    pub retained: u64,
    pub unavailable: u64,
    pub remaining: u64,
}
impl MemberClient {
    /// Build bounded index entries from copies already held on this device. No network.
    /// # Errors
    /// Invalid limits, custody, restored content or atomic persistence.
    pub fn index_local_mail(&self, limit: u16) -> Result<u64, Error> {
        if limit == 0 || limit > 25 {
            return Err(Error::InvalidData);
        }
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let mut retained = Retained::load(self, &keys, &state)?;
        let processed = self.index_external(&state, &keys, &mut retained, usize::from(limit))?;
        if processed > 0 {
            retained.save(self, &keys)?;
        }
        u64::try_from(processed).map_err(|_| Error::InvalidData)
    }
    /// Rebuild derived words from retained authenticated sources, preserving exclusions.
    /// # Errors
    /// Invalid source content, denied custody or failed atomic replacement.
    pub fn rebuild_search_index(&self) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let mut retained = Retained::load(self, &keys, &state)?;
        let known: Vec<_> = retained.entries.keys().copied().collect();
        // Prepare the complete replacement before publishing any part of the rebuilt index.
        for key in known {
            let Some(mut entry) = retained.copy(self, &keys, key, (self.clock)())? else {
                retained.exclude(key);
                continue;
            };
            let original_text = entry.text.clone();
            if let Some(view) = &entry.native {
                let (secret, public) = super::correspondence::own_keys(&state.profile, &keys)?;
                let doc = crate::correspondence::open(
                    &crate::correspondence::EndpointKeys {
                        owner: state.profile.persona,
                        secret: &secret,
                        public,
                    },
                    view,
                    view.record.sender_key,
                )
                .map_err(|_| Error::InvalidData)?;
                let subject = doc.subject().ok_or(Error::InvalidData)?;
                entry.text = format!(
                    "{}\n\n{}",
                    subject.text(),
                    super::ReadingContent::from_document(&doc).search_text()?
                );
            } else {
                entry.text = self.external_text(&state, &keys, key.thread, key.message)?;
            }
            if entry.text != original_text {
                return Err(Error::InvalidData);
            }
            retained.reindex(&entry)?;
        }
        retained.save(self, &keys)
    }
    pub(super) fn retained_document(
        &self,
        state: &State,
        keys: &Secrets,
        record: &cs_mail_application::correspondence::MessageRecord,
    ) -> Result<Option<Document>, Error> {
        let retained = Retained::load(self, keys, state)?;
        let key = CopyKey {
            thread: MailKey::Native(record.manifest.conversation()),
            message: record.manifest.message(),
        };
        let Some(entry) = retained.copy(self, keys, key, (self.clock)())? else {
            return Ok(None);
        };
        let view = entry.native.as_ref().ok_or(Error::InvalidData)?;
        if &view.record != record {
            return Err(Error::InvalidData);
        }
        let (secret, public) = super::correspondence::own_keys(&state.profile, keys)?;
        let document = crate::correspondence::open(
            &crate::correspondence::EndpointKeys {
                owner: state.profile.persona,
                secret: &secret,
                public,
            },
            view,
            record.sender_key,
        )
        .map_err(|_| Error::InvalidData)?;
        Ok(((self.clock)() < entry.expires).then_some(document))
    }
    /// Download bounded eligible copies after an explicit observation, never resume mutations.
    /// # Errors
    /// Invalid limits, unavailable transport/custody, substituted content or failed persistence.
    pub fn retain_mail(&self, limit: u16) -> Result<RetentionProgress, Error> {
        if limit == 0 || limit > 25 {
            return Err(Error::InvalidData);
        }
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        self.retain_batch(&state, &keys, usize::from(limit), None)
    }
    pub(super) fn retain_batch(
        &self,
        state: &State,
        keys: &Secrets,
        limit: usize,
        contact: Option<ProtocolIdentity>,
    ) -> Result<RetentionProgress, Error> {
        let mut retained = Retained::load(self, keys, state)?;
        if self.index_external(state, keys, &mut retained, limit)? > 0 {
            retained.save(self, keys)?;
        }
        let pending: Vec<_> = retained
            .missing_native
            .iter()
            .filter(|(key, _)| {
                contact.is_none_or(|p| match key.thread {
                    MailKey::Native(id) => state.mailbox.participants.get(&id.value()) == Some(&p),
                    MailKey::External(_) => false,
                })
            })
            .take(limit)
            .map(|(key, position)| (*key, *position))
            .collect();
        let epoch = self
            .retained_epoch
            .load(std::sync::atomic::Ordering::SeqCst);
        drop(retained);
        for (key, position) in pending {
            let record = &state
                .mailbox
                .records
                .get(position)
                .ok_or(Error::InvalidData)?
                .1;
            let signed = Self::signed(
                &state.profile,
                keys,
                Command::Fetch {
                    message: key.message,
                },
            )?;
            let response = self.correspondence_call(&state.profile, &signed)?;
            let mut retained = Retained::load_at_epoch(self, keys, state, Some(epoch))?;
            match response {
                Response::Message(view) => {
                    if &view.record != record || view.ciphertext.expires_at <= (self.clock)() {
                        return Err(Error::InvalidData);
                    }
                    let (secret, public) = super::correspondence::own_keys(&state.profile, keys)?;
                    let doc = crate::correspondence::open(
                        &crate::correspondence::EndpointKeys {
                            owner: state.profile.persona,
                            secret: &secret,
                            public,
                        },
                        &view,
                        record.sender_key,
                    )
                    .map_err(|_| Error::InvalidData)?;
                    let subject = doc.subject().cloned().ok_or(Error::InvalidData)?;
                    let text = format!(
                        "{}\n\n{}",
                        subject.text(),
                        super::ReadingContent::from_document(&doc).search_text()?
                    );
                    retained.insert(Entry {
                        key,
                        expires: view.ciphertext.expires_at,
                        native: Some(*view),
                        text,
                        subject: Some(subject),
                    });
                }
                Response::Unavailable => retained.exclude(key),
                Response::AccessDenied => return Err(Error::Unauthorized),
                _ => return Err(Error::InvalidData),
            }
            // Each fetched copy and its index are a resumable atomic checkpoint.
            retained.save(self, keys)?;
        }
        Ok(Retained::load_at_epoch(self, keys, state, Some(epoch))?.progress((self.clock)()))
    }
    pub(super) fn index_external(
        &self,
        state: &State,
        keys: &Secrets,
        retained: &mut Retained,
        limit: usize,
    ) -> Result<usize, Error> {
        let pending: Vec<_> = retained
            .missing_external
            .iter()
            .take(limit)
            .map(|(key, location)| (*key, *location))
            .collect();
        let processed = pending.len();
        for (key, (id, position)) in pending {
            let view = state
                .external
                .as_ref()
                .and_then(|e| e.records.get(&id))
                .and_then(|r| r.messages.get(position))
                .ok_or(Error::InvalidData)?;
            if view.message.member_copy.expires_at <= (self.clock)() {
                retained.exclude(key);
                continue;
            }
            let text = self.external_text(state, keys, key.thread, key.message)?;
            retained.insert(Entry {
                key,
                native: None,
                text,
                subject: None,
                expires: view.message.member_copy.expires_at,
            });
        }
        Ok(processed)
    }
}
