//! Personal mailbox state has no authority over permission, retention or delivery.
use super::{
    Draft, Error, MemberClient, State,
    storage::{self, Secrets},
};
use cs_mail_application::correspondence::external::Outcome;
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{AccountId, CanonicalTime, MessageId, ProtocolIdentity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum MailKey {
    Native(ConversationId),
    External(ConversationId),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum MailFolder {
    #[default]
    Inbox,
    Archive,
    Trash,
}
#[derive(Clone, Debug)]
pub enum MailAction {
    Move(MailFolder),
    Restore,
    Flag(bool),
    Read(Vec<MessageId>),
    Unread(Vec<MessageId>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersonalThread {
    key: MailKey,
    folder: MailFolder,
    before_trash: MailFolder,
    flagged: bool,
    read: BTreeSet<MessageId>,
}
impl PersonalThread {
    fn new(key: MailKey) -> Self {
        Self {
            key,
            folder: MailFolder::Inbox,
            before_trash: MailFolder::Inbox,
            flagged: false,
            read: BTreeSet::new(),
        }
    }
    fn transition(&mut self, action: MailAction) -> Result<(), Error> {
        match action {
            MailAction::Move(folder) => {
                if folder == MailFolder::Trash && self.folder != MailFolder::Trash {
                    self.before_trash = self.folder;
                }
                self.folder = folder;
            }
            MailAction::Restore => {
                self.folder = if self.folder == MailFolder::Trash {
                    self.before_trash
                } else {
                    MailFolder::Inbox
                };
            }
            MailAction::Flag(flag) => self.flagged = flag,
            MailAction::Read(ids) => self.read.extend(ids),
            MailAction::Unread(ids) => {
                for id in ids {
                    self.read.remove(&id);
                }
            }
        }
        if self.read.len() > 100_000 {
            return Err(Error::InvalidData);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorkStatus {
    AwaitingConfirmation,
    Committed,
    Queued,
    Refused,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) enum WorkCommand {
    Native(Box<super::correspondence::PendingSend>),
    External(Box<cs_mail_application::correspondence::external::SignedCommand>),
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) enum WorkEvidence {
    Native(super::SendOutcome),
    External(cs_mail_application::correspondence::external::Acknowledgement),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkRecord {
    operation: u128,
    key: MailKey,
    message: MessageId,
    draft: Option<(super::DraftId, super::DraftRevision)>,
    command: WorkCommand,
    evidence: Option<WorkEvidence>,
    prepared_at: CanonicalTime,
    resolved_at: Option<CanonicalTime>,
    status: WorkStatus,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Personal {
    account: AccountId,
    persona: ProtocolIdentity,
    threads: Vec<PersonalThread>,
    work: Vec<WorkRecord>,
    #[serde(default)]
    pub(super) resources: Vec<super::resources::ResourceRecord>,
    #[serde(default)]
    pub(super) email_text_revision: u8,
}
const CONTEXT: &[u8] = b"cs-mail/personal-mailbox/v1";
#[derive(Clone, Debug)]
pub struct MailItem {
    pub key: MailKey,
    pub other: String,
    pub title: String,
    pub last_at: u64,
    pub messages: Vec<MessageId>,
    pub unread: u64,
    pub folder: MailFolder,
    pub flagged: bool,
    pub has_sent: bool,
    pub receiving: Vec<super::MessageRoute>,
}
#[derive(Clone, Debug)]
pub struct OutgoingWork {
    pub operation: String,
    pub key: MailKey,
    pub message: String,
    pub prepared_at: u64,
    pub resolved_at: Option<u64>,
    pub status: WorkStatus,
}
impl Personal {
    pub(super) fn unread(&self, key: MailKey, ids: &[MessageId]) -> u64 {
        let read = self.threads.iter().find(|t| t.key == key).map(|t| &t.read);
        ids.iter()
            .filter(|id| read.is_none_or(|r| !r.contains(id)))
            .count() as u64
    }
    pub(super) fn load(
        client: &MemberClient,
        keys: &Secrets,
        state: &State,
    ) -> Result<Self, Error> {
        let value: Self = match client.store.load_personal()? {
            Some(bytes) => serde_json::from_slice(&storage::decrypt(keys, &bytes, CONTEXT)?)
                .map_err(|_| Error::InvalidData)?,
            None => Self {
                account: state.profile.account,
                persona: state.profile.persona,
                threads: Vec::new(),
                work: Vec::new(),
                resources: Vec::new(),
                email_text_revision: 0,
            },
        };
        super::resources::validate_records(&value.resources)?;
        if value.email_text_revision > 1 {
            return Err(Error::InvalidData);
        }
        let unique: BTreeSet<_> = value.threads.iter().map(|t| t.key).collect();
        let operations: BTreeSet<_> = value.work.iter().map(|w| w.operation).collect();
        if value.account != state.profile.account
            || value.persona != state.profile.persona
            || unique.len() != value.threads.len()
            || operations.len() != value.work.len()
            || value.threads.len() > 10_000
            || value.work.len() > 10_000
            || value.threads.iter().any(|t| {
                t.before_trash == MailFolder::Trash
                    || t.read.len() > 100_000
                    || t.read.iter().any(|m| m.0 == 0)
            })
        {
            return Err(Error::InvalidData);
        }
        for record in &value.work {
            let (operation, key, message) = WorkCommand::identity(&record.command)?;
            if (operation, key, message) != (record.operation, record.key, record.message)
                || record.operation == 0
                || record.message.0 == 0
                || record.resolved_at.is_some()
                    != (record.status != WorkStatus::AwaitingConfirmation)
                || record.resolved_at.is_some_and(|at| at < record.prepared_at)
            {
                return Err(Error::InvalidData);
            }
            match (&record.command, &record.evidence, record.status) {
                (_, None, WorkStatus::AwaitingConfirmation) => (),
                (
                    WorkCommand::Native(_),
                    Some(WorkEvidence::Native(super::SendOutcome::Committed(id))),
                    WorkStatus::Committed,
                ) if *id == record.message => (),
                (
                    WorkCommand::Native(p),
                    Some(WorkEvidence::Native(super::SendOutcome::Refused { operation, .. })),
                    WorkStatus::Refused,
                ) if *operation == p.send.operation
                    || p.create.as_ref().is_some_and(|c| c.operation == *operation) => {}
                (WorkCommand::External(p), Some(WorkEvidence::External(ack)), status) => {
                    ack.verify(p, &state.profile.deployment.provider_key)
                        .map_err(|_| Error::InvalidData)?;
                    if !matches!((&ack.receipt.outcome, status), (Outcome::Queued(id), WorkStatus::Queued) if *id == record.message)
                        && !matches!(
                            (&ack.receipt.outcome, status),
                            (Outcome::Refused(_), WorkStatus::Refused)
                        )
                    {
                        return Err(Error::InvalidData);
                    }
                }
                _ => return Err(Error::InvalidData),
            }
            match &record.command {
                WorkCommand::Native(p)
                    if p.send.account == value.account && p.send.persona == value.persona => {}
                WorkCommand::External(p)
                    if p.account == value.account && p.persona == value.persona => {}
                _ => return Err(Error::InvalidData),
            }
        }
        Ok(value)
    }
    pub(super) fn bytes(&self, keys: &Secrets) -> Result<Vec<u8>, Error> {
        storage::encrypt(keys, self, CONTEXT)
    }
    pub(super) fn prepare(
        &mut self,
        state: &State,
        draft: Option<&Draft>,
        now: CanonicalTime,
    ) -> Result<(), Error> {
        let command = if let Some(p) = &state.sending {
            Some(WorkCommand::Native(Box::new(p.clone())))
        } else {
            state
                .external
                .as_ref()
                .and_then(|w| w.pending.as_ref())
                .filter(|p| {
                    matches!(
                        p.command,
                        cs_mail_application::correspondence::external::Command::Start { .. }
                            | cs_mail_application::correspondence::external::Command::Send { .. }
                    )
                })
                .map(|p| WorkCommand::External(Box::new(p.clone())))
        };
        if let Some(command) = command {
            let (operation, key, message) = command.identity()?;
            if self.work.iter().any(|w| w.operation == operation) {
                return Err(Error::InvalidData);
            }
            if self.work.len() >= 10_000 {
                return Err(Error::InvalidData);
            }
            self.work.push(WorkRecord {
                operation,
                key,
                message,
                draft: draft.map(|d| (d.id(), d.revision())),
                command,
                evidence: None,
                prepared_at: now,
                resolved_at: None,
                status: WorkStatus::AwaitingConfirmation,
            });
        }
        Ok(())
    }
    pub(super) fn resolve(
        &mut self,
        state: &State,
        refused: bool,
        evidence: Option<WorkEvidence>,
        now: CanonicalTime,
    ) {
        // The current journal remains the single owner of in-flight command execution.
        if let Some(record) = self
            .work
            .iter_mut()
            .find(|w| w.status == WorkStatus::AwaitingConfirmation)
        {
            let pending = match record.command {
                WorkCommand::Native(_) => state.sending.is_some(),
                WorkCommand::External(_) => {
                    state.external.as_ref().is_some_and(|w| w.pending.is_some())
                }
            };
            if !pending {
                record.status = if refused {
                    WorkStatus::Refused
                } else if matches!(record.command, WorkCommand::Native(_)) {
                    WorkStatus::Committed
                } else {
                    WorkStatus::Queued
                };
                record.resolved_at = Some(now.max(record.prepared_at));
                record.evidence = evidence;
            }
        }
    }
}
impl WorkCommand {
    fn identity(&self) -> Result<(u128, MailKey, MessageId), Error> {
        use cs_mail_application::correspondence::{Command, external};
        match self {
            Self::Native(p) => {
                if let Command::Send { package, .. } | Command::SendToAlias { package, .. } =
                    &p.send.command
                {
                    Ok((
                        p.send.operation.0,
                        MailKey::Native(package.manifest.conversation()),
                        package.manifest.message(),
                    ))
                } else {
                    Err(Error::InvalidData)
                }
            }
            Self::External(p) => {
                if let external::Command::Start { message, .. }
                | external::Command::Send { message, .. } = &p.command
                {
                    Ok((
                        p.operation.0,
                        MailKey::External(p.conversation),
                        message.manifest.message(),
                    ))
                } else {
                    Err(Error::InvalidData)
                }
            }
        }
    }
}
pub(super) fn items(state: &State, personal: &Personal) -> Result<Vec<MailItem>, Error> {
    let mut result: BTreeMap<MailKey, MailItem> = BTreeMap::new();
    for (_, record) in &state.mailbox.records {
        let id = record.manifest.conversation();
        let key = MailKey::Native(id);
        let other = *state
            .mailbox
            .participants
            .get(&id.value())
            .ok_or(Error::InvalidData)?;
        let title = state
            .inbox
            .iter()
            .find(|v| v.handle.sender == other)
            .map_or_else(
                || "Native contact".into(),
                |v| {
                    v.display_name
                        .clone()
                        .unwrap_or_else(|| "Native contact".into())
                },
            );
        let item = result.entry(key).or_insert(MailItem {
            key,
            other: other.0.to_string(),
            title,
            last_at: 0,
            messages: Vec::new(),
            unread: 0,
            folder: MailFolder::Inbox,
            flagged: false,
            has_sent: false,
            receiving: Vec::new(),
        });
        observe(
            item,
            personal,
            record.manifest.message(),
            record.author == state.profile.persona,
            record.committed_at.0,
        );
    }
    if let Some(external) = &state.external {
        for record in external.records.values() {
            let c = &record.conversation;
            let key = MailKey::External(c.id());
            let item = result.entry(key).or_insert(MailItem {
                key,
                other: c.guest().0.to_string(),
                title: c.mailbox().address(),
                last_at: 0,
                messages: Vec::new(),
                unread: 0,
                folder: MailFolder::Inbox,
                flagged: false,
                has_sent: false,
                receiving: Vec::new(),
            });
            for entry in &record.messages {
                observe(
                    item,
                    personal,
                    entry.message.manifest.message(),
                    entry.message.direction
                        == cs_mail_application::correspondence::external::Direction::MemberToGuest,
                    entry.message.committed_at.0,
                );
            }
        }
    }
    let mut result: Vec<_> = result.into_values().collect();
    for item in &mut result {
        if let Some(p) = personal.threads.iter().find(|t| t.key == item.key) {
            item.folder = p.folder;
            item.flagged = p.flagged;
        }
    }
    result.sort_by_key(|item| (std::cmp::Reverse(item.last_at), item.key));
    Ok(result)
}
fn observe(item: &mut MailItem, personal: &Personal, message: MessageId, outgoing: bool, at: u64) {
    item.messages.push(message);
    item.last_at = item.last_at.max(at);
    item.has_sent |= outgoing;
    if !outgoing
        && !personal
            .threads
            .iter()
            .find(|t| t.key == item.key)
            .is_some_and(|t| t.read.contains(&message))
    {
        item.unread += 1;
    }
}
impl MemberClient {
    /// Metadata-only local projection. Synchronization never marks a message read.
    /// # Errors
    /// Unavailable custody or invalid personal state.
    pub fn mail_items(&self) -> Result<Vec<MailItem>, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let result = items(&state, &Personal::load(self, &keys, &state)?)?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        Ok(result)
    }
    /// Only the supplied displayed IDs change read state; later arrivals remain unread.
    /// # Errors
    /// Unknown thread/message, denied custody, or failed durable commit.
    pub fn organize_mail(
        &self,
        key: MailKey,
        scope: super::InboxScope,
        action: MailAction,
    ) -> Result<(), Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let mut personal = Personal::load(self, &keys, &state)?;
        let known = items(&state, &personal)?
            .into_iter()
            .find(|i| i.key == key)
            .ok_or(Error::InvalidData)?;
        let visible = self.scope_items(&state, vec![known.clone()], scope, &personal)?;
        let known = visible.first().ok_or(Error::InvalidData)?;
        if let MailAction::Read(ids) | MailAction::Unread(ids) = &action {
            if ids.len() > 10_000 || ids.iter().any(|m| !known.messages.contains(m)) {
                return Err(Error::InvalidData);
            }
        }
        let mut next = personal
            .threads
            .iter()
            .find(|t| t.key == key)
            .cloned()
            .unwrap_or_else(|| PersonalThread::new(key));
        next.transition(action)?;
        personal.threads.retain(|t| t.key != key);
        personal.threads.push(next);
        if personal.threads.len() > 10_000 {
            return Err(Error::InvalidData);
        }
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.store.save_personal(&personal.bytes(&keys)?)
    }
    /// # Errors
    /// Unavailable custody or invalid operation evidence.
    pub fn outgoing_work(&self) -> Result<Vec<OutgoingWork>, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let personal = Personal::load(self, &keys, &state)?;
        let mut work: Vec<_> = personal
            .work
            .into_iter()
            .map(|w| OutgoingWork {
                operation: w.operation.to_string(),
                key: w.key,
                message: w.message.0.to_string(),
                prepared_at: w.prepared_at.0,
                resolved_at: w.resolved_at.map(|t| t.0),
                status: w.status,
            })
            .collect();
        work.sort_by_key(|w| std::cmp::Reverse(w.prepared_at));
        storage::check_keys(self.store.as_ref(), &keys)?;
        Ok(work)
    }
}
