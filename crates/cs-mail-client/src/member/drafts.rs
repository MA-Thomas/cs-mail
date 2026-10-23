//! Device-local writing. Draft transitions have no communication or payment authority.
use super::{
    Error, MemberClient, SourceDraft, State,
    storage::{self, Secrets},
};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{AccountId, CanonicalTime, ProtocolIdentity, RequestClassId, RequestId};
use serde::{Deserialize, Serialize};

/// Local identity and revision are distinct from message and operation identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct DraftId(u128);
impl TryFrom<u128> for DraftId {
    type Error = Error;
    fn try_from(value: u128) -> Result<Self, Error> {
        if value == 0 {
            Err(Error::InvalidData)
        } else {
            Ok(Self(value))
        }
    }
}
impl From<DraftId> for u128 {
    fn from(value: DraftId) -> Self {
        value.0
    }
}
impl DraftId {
    pub const fn value(self) -> u128 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct DraftRevision(u64);
impl TryFrom<u64> for DraftRevision {
    type Error = Error;
    fn try_from(value: u64) -> Result<Self, Error> {
        if value == 0 {
            Err(Error::InvalidData)
        } else {
            Ok(Self(value))
        }
    }
}
impl From<DraftRevision> for u64 {
    fn from(value: DraftRevision) -> Self {
        value.0
    }
}
impl DraftRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DraftTarget {
    Native {
        other: ProtocolIdentity,
        conversation: Option<ConversationId>,
    },
    External {
        conversation: Option<ConversationId>,
    },
    Request {
        alias: cs_mail_primitives::AliasId,
        other: ProtocolIdentity,
        class: RequestClassId,
    },
    Followup {
        other: ProtocolIdentity,
        request: RequestId,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DraftBody {
    Native {
        text: String,
        sources: Vec<SourceDraft>,
    },
    External {
        mailbox: String,
        text: String,
    },
    Request {
        text: String,
    },
}
#[derive(Clone, Debug)]
pub enum DraftIssue {
    SubjectTooLong { max_bytes: u64 },
    EmptyMessage,
    TextTooLong { max_bytes: u64 },
    InvalidMailbox,
}
impl DraftBody {
    pub fn issues(&self) -> Vec<DraftIssue> {
        let mut issues = Vec::new();
        let has_sources = matches!(self, Self::Native { sources, .. } if !sources.is_empty());
        if self.text().trim().is_empty() && !has_sources {
            issues.push(DraftIssue::EmptyMessage);
        }
        let max = if matches!(self, Self::External { .. }) {
            super::external::MAX_TEXT_BYTES
        } else {
            super::correspondence::MAX_TEXT_BYTES
        };
        if self.text().len() > max {
            issues.push(DraftIssue::TextTooLong {
                max_bytes: max as u64,
            });
        }
        if let Self::External { mailbox, .. } = self {
            if cs_mail_guests::GuestMailbox::parse(mailbox).is_err() {
                issues.push(DraftIssue::InvalidMailbox);
            }
        }
        issues
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Native { text, .. } | Self::External { text, .. } | Self::Request { text } => {
                text
            }
        }
    }
    fn validate(&self, target: &DraftTarget) -> Result<(), Error> {
        let shape = matches!(
            (target, self),
            (DraftTarget::Native { .. }, Self::Native { .. })
                | (DraftTarget::External { .. }, Self::External { .. })
                | (
                    DraftTarget::Request { .. } | DraftTarget::Followup { .. },
                    Self::Request { .. }
                )
        );
        if !shape || self.text().len() > 1_048_576 {
            return Err(Error::InvalidData);
        }
        match self {
            Self::Native { sources, .. } if sources.len() > 128 => Err(Error::InvalidData),
            Self::External { mailbox, .. } if mailbox.len() > 320 => Err(Error::InvalidData),
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "DraftRecord", into = "DraftRecord")]
pub struct Draft(DraftRecord);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRecord {
    subject: String,
    local_offset_seconds: i32,
    id: DraftId,
    revision: DraftRevision,
    target: DraftTarget,
    body: DraftBody,
    updated_at: CanonicalTime,
}
impl TryFrom<DraftRecord> for Draft {
    type Error = Error;
    fn try_from(record: DraftRecord) -> Result<Self, Error> {
        record.body.validate(&record.target)?;
        if record.subject.len() > 65_536
            || record.subject.chars().any(char::is_control)
            || !(-86_399..=86_399).contains(&record.local_offset_seconds)
            || (!matches!(
                record.target,
                DraftTarget::Native {
                    conversation: None,
                    ..
                } | DraftTarget::Request { .. }
            ) && !record.subject.is_empty())
        {
            return Err(Error::InvalidData);
        }
        match record.target {
            DraftTarget::Native { other, .. }
            | DraftTarget::Request { other, .. }
            | DraftTarget::Followup { other, .. }
                if other.0 == 0 =>
            {
                return Err(Error::InvalidData);
            }
            DraftTarget::Request { class, .. } if class.0 == 0 => return Err(Error::InvalidData),
            DraftTarget::Followup { request, .. } if request.0 == 0 => {
                return Err(Error::InvalidData);
            }
            _ => (),
        }
        Ok(Self(record))
    }
}
impl From<Draft> for DraftRecord {
    fn from(value: Draft) -> Self {
        value.0
    }
}
impl Draft {
    pub const fn id(&self) -> DraftId {
        self.0.id
    }
    pub const fn revision(&self) -> DraftRevision {
        self.0.revision
    }
    pub fn target(&self) -> &DraftTarget {
        &self.0.target
    }
    pub fn issues(&self) -> Vec<DraftIssue> {
        let mut issues = self.body().issues();
        if self.subject().trim().len() > cs_mail_correspondence::MAX_SUBJECT_BYTES {
            issues.push(DraftIssue::SubjectTooLong {
                max_bytes: cs_mail_correspondence::MAX_SUBJECT_BYTES as u64,
            });
        }
        issues
    }
    pub fn subject(&self) -> &str {
        &self.0.subject
    }
    pub const fn local_offset_seconds(&self) -> i32 {
        self.0.local_offset_seconds
    }
    pub fn body(&self) -> &DraftBody {
        &self.0.body
    }
    pub const fn updated_at(&self) -> CanonicalTime {
        self.0.updated_at
    }
    fn update(
        &self,
        expected: DraftRevision,
        body: DraftBody,
        subject: String,
        local_offset_seconds: i32,
        now: CanonicalTime,
    ) -> Result<Self, Error> {
        if self.revision() != expected {
            return Err(Error::DraftChanged);
        }
        DraftRecord {
            subject,
            local_offset_seconds,
            id: self.id(),
            revision: DraftRevision(expected.0.checked_add(1).ok_or(Error::InvalidData)?),
            target: self.target().clone(),
            body,
            updated_at: now.max(self.updated_at()),
        }
        .try_into()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    account: AccountId,
    persona: ProtocolIdentity,
    drafts: Vec<Draft>,
    submitted: Option<Draft>,
}
impl Workspace {
    fn load(client: &MemberClient, keys: &Secrets, state: &State) -> Result<Self, Error> {
        let value: Self = match client.store.load_workspace()? {
            Some(bytes) => serde_json::from_slice(&storage::decrypt(keys, &bytes, CONTEXT)?)
                .map_err(|_| Error::InvalidData)?,
            None => Self {
                account: state.profile.account,
                persona: state.profile.persona,
                drafts: Vec::new(),
                submitted: None,
            },
        };
        let ids: std::collections::BTreeSet<_> = value.drafts.iter().map(|d| d.id().0).collect();
        if value.account != state.profile.account
            || value.persona != state.profile.persona
            || value.drafts.len() + usize::from(value.submitted.is_some()) > 256
            || ids.len() != value.drafts.len()
            || value
                .submitted
                .as_ref()
                .is_some_and(|d| ids.contains(&d.id().value()))
        {
            return Err(Error::InvalidData);
        }
        Ok(value)
    }
    fn bytes(&self, keys: &Secrets) -> Result<Vec<u8>, Error> {
        storage::encrypt(keys, self, CONTEXT)
    }
    fn get(&self, id: DraftId, revision: DraftRevision) -> Result<Draft, Error> {
        let draft = self
            .drafts
            .iter()
            .find(|d| d.id() == id)
            .ok_or(Error::DraftChanged)?;
        if draft.revision() != revision {
            return Err(Error::DraftChanged);
        }
        Ok(draft.clone())
    }
}
#[derive(Clone, Debug)]
pub enum DraftSubmission {
    Native {
        message: cs_mail_primitives::MessageId,
    },
    External {
        conversation: ConversationId,
    },
    Request {
        request: RequestId,
    },
}
#[derive(Clone, Copy)]
pub(super) enum DraftResolution {
    RemoveSubmitted,
    RestoreEditable,
}
#[derive(Clone, Copy)]
enum OpenMode {
    Resume,
    New,
}
/// Commands carry caller-supplied identifiers and time. This decision performs no I/O.
enum WorkspaceCommand {
    Open {
        id: DraftId,
        target: DraftTarget,
        mode: OpenMode,
    },
    Save {
        id: DraftId,
        expected: DraftRevision,
        body: DraftBody,
        subject: String,
        local_offset_seconds: i32,
    },
    Discard {
        id: DraftId,
        expected: DraftRevision,
    },
    Submit {
        id: DraftId,
        expected: DraftRevision,
    },
    Resolve {
        resolution: DraftResolution,
    },
}
struct WorkspaceDecision {
    next: Workspace,
    draft: Option<Draft>,
}
impl Workspace {
    fn transition(
        mut self,
        command: WorkspaceCommand,
        now: CanonicalTime,
    ) -> Result<WorkspaceDecision, Error> {
        let mut output = None;
        match command {
            WorkspaceCommand::Open { id, target, mode } => {
                let existing = if matches!(mode, OpenMode::Resume) {
                    self.drafts
                        .iter()
                        .filter(|d| d.target() == &target)
                        .max_by_key(|d| (d.updated_at(), d.id().value()))
                } else {
                    None
                };
                if let Some(draft) = existing {
                    output = Some(draft.clone());
                } else {
                    if self.drafts.len() + usize::from(self.submitted.is_some()) >= 256
                        || self.drafts.iter().any(|d| d.id() == id)
                        || self.submitted.as_ref().is_some_and(|d| d.id() == id)
                    {
                        return Err(Error::InvalidData);
                    }
                    let body = match target {
                        DraftTarget::Native { .. } => DraftBody::Native {
                            text: String::new(),
                            sources: Vec::new(),
                        },
                        DraftTarget::External { .. } => DraftBody::External {
                            mailbox: String::new(),
                            text: String::new(),
                        },
                        _ => DraftBody::Request {
                            text: String::new(),
                        },
                    };
                    let draft: Draft = DraftRecord {
                        subject: String::new(),
                        local_offset_seconds: 0,
                        id,
                        revision: DraftRevision(1),
                        target,
                        body,
                        updated_at: now,
                    }
                    .try_into()?;
                    self.drafts.push(draft.clone());
                    output = Some(draft);
                }
            }
            WorkspaceCommand::Save {
                id,
                expected,
                body,
                subject,
                local_offset_seconds,
            } => {
                let next = self.get(id, expected)?.update(
                    expected,
                    body,
                    subject,
                    local_offset_seconds,
                    now,
                )?;
                *self
                    .drafts
                    .iter_mut()
                    .find(|d| d.id() == id)
                    .ok_or(Error::DraftChanged)? = next.clone();
                output = Some(next);
            }
            WorkspaceCommand::Discard { id, expected } => {
                self.get(id, expected)?;
                self.drafts.retain(|d| d.id() != id);
            }
            WorkspaceCommand::Submit { id, expected } => {
                let draft = self.get(id, expected)?;
                if self.submitted.is_some() {
                    return Err(Error::DraftChanged);
                }
                self.drafts.retain(|d| d.id() != id);
                self.submitted = Some(draft);
            }
            WorkspaceCommand::Resolve { resolution } => {
                if let Some(draft) = self.submitted.take() {
                    if matches!(resolution, DraftResolution::RestoreEditable) {
                        self.drafts.push(draft.update(
                            draft.revision(),
                            draft.body().clone(),
                            draft.subject().to_owned(),
                            draft.local_offset_seconds(),
                            now,
                        )?);
                    }
                }
            }
        }
        Ok(WorkspaceDecision {
            next: self,
            draft: output,
        })
    }
}
const CONTEXT: &[u8] = b"cs-mail/member-workspace/v3";

impl MemberClient {
    /// Local reads and writes use a separate short lock; they never wait for network work.
    /// # Errors
    /// Unavailable custody, malformed workspace, or account mismatch.
    pub fn drafts(&self) -> Result<Vec<Draft>, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        Ok(Workspace::load(self, &keys, &state)?.drafts)
    }
    /// Resumes the existing target draft or creates an empty local draft. No external I/O.
    /// # Errors
    /// Unavailable custody, invalid target, or full workspace.
    pub fn open_draft(&self, target: DraftTarget) -> Result<Draft, Error> {
        self.open_draft_mode(target, OpenMode::Resume)
    }
    /// Starts separate writing even when another draft has the same destination.
    /// # Errors
    /// Unavailable custody, invalid target, or full workspace.
    pub fn create_draft(&self, target: DraftTarget) -> Result<Draft, Error> {
        self.open_draft_mode(target, OpenMode::New)
    }
    fn open_draft_mode(&self, target: DraftTarget, mode: OpenMode) -> Result<Draft, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let workspace = Workspace::load(self, &keys, &state)?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        let decision = workspace.transition(
            WorkspaceCommand::Open {
                id: DraftId(u128::from_be_bytes(random) | 1),
                target,
                mode,
            },
            (self.clock)(),
        )?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.store.save_workspace(&decision.next.bytes(&keys)?)?;
        let draft = decision.draft.ok_or(Error::InvalidData)?;
        Ok(draft)
    }
    /// # Errors
    /// Stale editor revision, invalid content, or failed atomic persistence.
    pub fn save_draft(
        &self,
        id: DraftId,
        expected: DraftRevision,
        body: DraftBody,
        subject: &str,
        local_offset_seconds: i32,
    ) -> Result<Draft, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let workspace = Workspace::load(self, &keys, &state)?;
        let decision = workspace.transition(
            WorkspaceCommand::Save {
                id,
                expected,
                body,
                subject: subject.to_owned(),
                local_offset_seconds,
            },
            (self.clock)(),
        )?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.store.save_workspace(&decision.next.bytes(&keys)?)?;
        let next = decision.draft.ok_or(Error::InvalidData)?;
        Ok(next)
    }
    /// # Errors
    /// A later edit cannot be discarded by an older editor.
    pub fn discard_draft(&self, id: DraftId, expected: DraftRevision) -> Result<(), Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let workspace = Workspace::load(self, &keys, &state)?;
        let decision =
            workspace.transition(WorkspaceCommand::Discard { id, expected }, (self.clock)())?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.store.save_workspace(&decision.next.bytes(&keys)?)
    }
    /// Revision check and draft-to-operation handoff are one local transaction.
    pub(super) fn commit_draft(
        &self,
        keys: &Secrets,
        state: &State,
        draft: Option<&Draft>,
    ) -> Result<(), Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let workspace = Workspace::load(self, keys, state)?;
        let next = if let Some(draft) = draft {
            workspace
                .transition(
                    WorkspaceCommand::Submit {
                        id: draft.id(),
                        expected: draft.revision(),
                    },
                    (self.clock)(),
                )?
                .next
        } else {
            workspace
        };
        let mut personal = super::personal::Personal::load(self, keys, state)?;
        personal.prepare(state, draft, (self.clock)())?;
        storage::check_keys(self.store.as_ref(), keys)?;
        self.store.save_submission(
            &storage::state_bytes(keys, state)?,
            &next.bytes(keys)?,
            &personal.bytes(keys)?,
        )
    }
    /// Terminal journal outcome and draft disposition commit together. Uncertainty never calls this.
    pub(super) fn finish_draft(
        &self,
        keys: &Secrets,
        state: &State,
        resolution: DraftResolution,
        evidence: Option<super::personal::WorkEvidence>,
    ) -> Result<(), Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let workspace = Workspace::load(self, keys, state)?;
        let decision =
            workspace.transition(WorkspaceCommand::Resolve { resolution }, (self.clock)())?;
        let mut personal = super::personal::Personal::load(self, keys, state)?;
        personal.resolve(
            state,
            matches!(resolution, DraftResolution::RestoreEditable),
            evidence,
            (self.clock)(),
        );
        storage::check_keys(self.store.as_ref(), keys)?;
        self.store.save_submission(
            &storage::state_bytes(keys, state)?,
            &decision.next.bytes(keys)?,
            &personal.bytes(keys)?,
        )
    }
    /// Explicit send intent; saved disclosure checkboxes and cached permission are not authority.
    /// # Errors
    /// Changed draft, invalid content, unavailable current authority, or unresolved submission.
    pub fn submit_draft(
        &self,
        id: DraftId,
        expected: DraftRevision,
        disclose: bool,
        reviewed_request: Option<RequestId>,
    ) -> Result<DraftSubmission, Error> {
        let draft = self
            .drafts()?
            .into_iter()
            .find(|d| d.id() == id && d.revision() == expected)
            .ok_or(Error::DraftChanged)?;
        match (draft.target(), draft.body()) {
            (
                DraftTarget::Native {
                    other,
                    conversation,
                },
                DraftBody::Native { text, sources },
            ) => {
                let message = self.send_message_with_draft(
                    &other.0.to_string(),
                    conversation.map(ConversationId::value),
                    text,
                    sources,
                    Some(&draft),
                    None,
                )?;
                Ok(DraftSubmission::Native { message })
            }
            (DraftTarget::External { conversation }, DraftBody::External { mailbox, text }) => {
                let conversation = self.send_external_with_draft(
                    conversation.map(|v| v.value().to_string()).as_deref(),
                    mailbox,
                    text,
                    disclose,
                    Some(&draft),
                )?;
                Ok(DraftSubmission::External { conversation })
            }
            (
                DraftTarget::Request {
                    other,
                    class,
                    alias,
                },
                DraftBody::Request { text },
            ) => {
                self.authorize_request_with_draft(
                    reviewed_request.ok_or(Error::InvalidData)?,
                    text,
                    Some((&draft, *other, *class, *alias)),
                )?;
                Ok(DraftSubmission::Request {
                    request: reviewed_request.ok_or(Error::InvalidData)?,
                })
            }
            (DraftTarget::Followup { other, request }, DraftBody::Request { text }) => {
                self.send_followup_with_draft(&other.0.to_string(), *request, text, Some(&draft))?;
                Ok(DraftSubmission::Request { request: *request })
            }
            _ => Err(Error::InvalidData),
        }
    }
}
