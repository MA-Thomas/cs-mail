//! Native adapter contracts for reading and personal organization.
use super::{AppleClient, ClientError, Inbox, core};
use cs_mail_correspondence::{
    ConversationId,
    external::delivery::{DeliveryState, ProviderEvent, ProviderEventKind},
};
#[derive(Clone, Copy, uniffi::Enum)]
pub enum MailTransport {
    Native,
    External,
}
#[derive(Clone, uniffi::Record)]
pub struct MailKey {
    pub transport: MailTransport,
    pub conversation: String,
}
impl TryFrom<MailKey> for core::MailKey {
    type Error = ClientError;
    fn try_from(key: MailKey) -> Result<Self, ClientError> {
        let id = ConversationId::new(
            key.conversation
                .parse()
                .map_err(|_| ClientError::InvalidData)?,
        )
        .map_err(|_| ClientError::InvalidData)?;
        Ok(match key.transport {
            MailTransport::Native => Self::Native(id),
            MailTransport::External => Self::External(id),
        })
    }
}
impl From<core::MailKey> for MailKey {
    fn from(key: core::MailKey) -> Self {
        match key {
            core::MailKey::Native(id) => Self {
                transport: MailTransport::Native,
                conversation: id.value().to_string(),
            },
            core::MailKey::External(id) => Self {
                transport: MailTransport::External,
                conversation: id.value().to_string(),
            },
        }
    }
}
#[derive(Clone, Copy, uniffi::Enum)]
pub enum MailFolder {
    Inbox,
    Archive,
    Trash,
}
impl From<core::MailFolder> for MailFolder {
    fn from(f: core::MailFolder) -> Self {
        match f {
            core::MailFolder::Inbox => Self::Inbox,
            core::MailFolder::Archive => Self::Archive,
            core::MailFolder::Trash => Self::Trash,
        }
    }
}
#[derive(uniffi::Enum)]
pub enum MailAction {
    Move { folder: MailFolder },
    Restore,
    Flag { value: bool },
    Read { displayed: Vec<String> },
    Unread { displayed: Vec<String> },
}
#[derive(Clone, uniffi::Record)]
pub struct MailItem {
    pub key: MailKey,
    pub other: String,
    pub title: String,
    pub last_at: u64,
    pub messages: Vec<String>,
    pub unread: u64,
    pub folder: MailFolder,
    pub flagged: bool,
    pub has_sent: bool,
    pub receiving: Vec<MessageRoute>,
}
impl From<core::MailItem> for MailItem {
    fn from(i: core::MailItem) -> Self {
        Self {
            key: i.key.into(),
            other: i.other,
            title: i.title,
            last_at: i.last_at,
            messages: i.messages.into_iter().map(|m| m.0.to_string()).collect(),
            unread: i.unread,
            folder: i.folder.into(),
            flagged: i.flagged,
            has_sent: i.has_sent,
            receiving: i
                .receiving
                .into_iter()
                .map(|r| MessageRoute {
                    message: r.message,
                    alias: r.alias,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Copy, uniffi::Enum)]
pub enum WorkStatus {
    AwaitingConfirmation,
    Committed,
    Queued,
    Refused,
}
#[derive(uniffi::Record)]
pub struct OutgoingWork {
    pub operation: String,
    pub key: MailKey,
    pub message: String,
    pub prepared_at: u64,
    pub resolved_at: Option<u64>,
    pub status: WorkStatus,
}
#[derive(Clone, uniffi::Enum)]
pub enum ReadingBlock {
    Text { text: String },
    Image { bytes: Vec<u8> },
}
impl From<core::ReadingBlock> for ReadingBlock {
    fn from(b: core::ReadingBlock) -> Self {
        match b {
            core::ReadingBlock::Text(text) => Self::Text { text },
            core::ReadingBlock::Image { bytes } => Self::Image { bytes },
        }
    }
}
#[derive(Clone, uniffi::Record)]
pub struct EmailResource {
    pub id: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, uniffi::Enum)]
pub enum PictureStatus {
    NotLoaded,
    Attempted,
    Loaded,
}
#[derive(Clone, uniffi::Record)]
pub struct EmailPicture {
    pub id: String,
    pub url: String,
    pub status: PictureStatus,
}
#[derive(Clone, uniffi::Record)]
pub struct ReadingEmail {
    pub files: Vec<crate::ComposerFile>,
    pub subject: Option<String>,
    pub html: Option<String>,
    pub text: String,
    pub supplied_text: Option<String>,
    pub resources: Vec<EmailResource>,
    pub pictures: Vec<EmailPicture>,
}
impl From<core::ReadingEmail> for ReadingEmail {
    fn from(e: core::ReadingEmail) -> Self {
        Self {
            files: e
                .files
                .iter()
                .map(|f| crate::ComposerFile {
                    name: f.name().into(),
                    bytes: f.bytes().to_vec(),
                })
                .collect(),
            subject: e.subject,
            html: e.html,
            text: e.text,
            supplied_text: e.supplied_text,
            resources: e
                .resources
                .into_iter()
                .map(|r| EmailResource {
                    id: r.id,
                    mime_type: r.mime_type,
                    bytes: r.bytes,
                })
                .collect(),
            pictures: e
                .pictures
                .into_iter()
                .map(|p| EmailPicture {
                    id: p.id,
                    url: p.url,
                    status: match p.status {
                        core::PictureStatus::NotLoaded => PictureStatus::NotLoaded,
                        core::PictureStatus::Attempted => PictureStatus::Attempted,
                        core::PictureStatus::Loaded => PictureStatus::Loaded,
                    },
                })
                .collect(),
        }
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum ReadingPart {
    Email {
        email: ReadingEmail,
    },
    Text {
        text: String,
    },
    Image {
        bytes: Vec<u8>,
    },
    Quotation {
        source: String,
        blocks: Vec<ReadingBlock>,
    },
    SharedCopy {
        source: String,
        blocks: Vec<ReadingBlock>,
    },
    Reference {
        source: String,
        part: u32,
    },
}
#[derive(Clone, uniffi::Enum)]
pub enum ReadingContent {
    NotRetained,
    Readable { parts: Vec<ReadingPart> },
    Unavailable,
    AccessDenied,
}
impl From<core::ReadingContent> for ReadingContent {
    fn from(c: core::ReadingContent) -> Self {
        match c {
            core::ReadingContent::NotRetained => Self::NotRetained,
            core::ReadingContent::Unavailable => Self::Unavailable,
            core::ReadingContent::AccessDenied => Self::AccessDenied,
            core::ReadingContent::Readable(parts) => Self::Readable {
                parts: parts
                    .into_iter()
                    .map(|p| match p {
                        core::ReadingPart::Email(email) => ReadingPart::Email {
                            email: email.into(),
                        },
                        core::ReadingPart::Text(text) => ReadingPart::Text { text },
                        core::ReadingPart::Image { bytes } => ReadingPart::Image { bytes },
                        core::ReadingPart::Quotation { source, blocks } => ReadingPart::Quotation {
                            source,
                            blocks: blocks.into_iter().map(Into::into).collect(),
                        },
                        core::ReadingPart::SharedCopy { source, blocks } => {
                            ReadingPart::SharedCopy {
                                source,
                                blocks: blocks.into_iter().map(Into::into).collect(),
                            }
                        }
                        core::ReadingPart::Reference { source, part } => {
                            ReadingPart::Reference { source, part }
                        }
                    })
                    .collect(),
            },
        }
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum HandoffState {
    Incoming,
    Waiting,
    Cancelled { at: u64 },
    Begun { at: u64 },
    Accepted { at: u64, reference: String },
    Failed { at: u64 },
}
impl From<Option<DeliveryState>> for HandoffState {
    fn from(s: Option<DeliveryState>) -> Self {
        match s {
            None => Self::Incoming,
            Some(DeliveryState::Waiting) => Self::Waiting,
            Some(DeliveryState::Cancelled { at }) => Self::Cancelled { at: at.0 },
            Some(DeliveryState::DispatchBegun { at }) => Self::Begun { at: at.0 },
            Some(DeliveryState::Accepted {
                at,
                provider_reference,
            }) => Self::Accepted {
                at: at.0,
                reference: provider_reference.as_str().into(),
            },
            Some(DeliveryState::Failed { at }) => Self::Failed { at: at.0 },
        }
    }
}
#[derive(Clone, Copy, uniffi::Enum)]
pub enum ProviderObservationKind {
    Processed,
    Delivered,
    HardBounce,
    SoftBounce,
    Rejected,
    SpamComplaint,
}
#[derive(Clone, uniffi::Record)]
pub struct ProviderObservation {
    pub id: String,
    pub kind: ProviderObservationKind,
    pub provider_time: String,
}
impl From<ProviderEvent> for ProviderObservation {
    fn from(e: ProviderEvent) -> Self {
        Self {
            id: e.id.as_str().into(),
            provider_time: e.provider_time.into(),
            kind: match e.kind {
                ProviderEventKind::Processed => ProviderObservationKind::Processed,
                ProviderEventKind::Delivered => ProviderObservationKind::Delivered,
                ProviderEventKind::HardBounce => ProviderObservationKind::HardBounce,
                ProviderEventKind::SoftBounce => ProviderObservationKind::SoftBounce,
                ProviderEventKind::Rejected => ProviderObservationKind::Rejected,
                ProviderEventKind::SpamComplaint => ProviderObservationKind::SpamComplaint,
            },
        }
    }
}
#[derive(Clone, uniffi::Record)]
pub struct SearchMatch {
    pub key: MailKey,
    pub message: String,
    pub excerpt: String,
    pub expires_at: u64,
}
#[derive(uniffi::Record)]
pub struct SearchPage {
    pub matches: Vec<SearchMatch>,
    pub searched: u64,
    pub unavailable: u64,
    pub not_retained: u64,
    pub next: u64,
    pub total: u64,
}
#[derive(uniffi::Record)]
pub struct MailRefresh {
    pub inbox: Inbox,
    pub native_current: bool,
    pub external_current: bool,
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Unavailable authority, unresolved work, refusal or failed copy-removal persistence.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values across its language boundary.
    pub fn delete_native_copies(&self, conversation: String) -> Result<(), ClientError> {
        let id = ConversationId::new(conversation.parse().map_err(|_| ClientError::InvalidData)?)
            .map_err(|_| ClientError::InvalidData)?;
        self.0.delete_native_copies(id).map_err(Into::into)
    }
    /// # Errors
    /// The search session cannot be cleared.
    pub fn clear_search(&self) -> Result<(), ClientError> {
        self.0.clear_search().map_err(Into::into)
    }
    /// # Errors
    /// Authentication, custody, malformed observations or failed local persistence.
    pub fn refresh_mail(&self) -> Result<MailRefresh, ClientError> {
        self.0
            .refresh_mail()
            .map(|r| MailRefresh {
                inbox: r.inbox.into(),
                native_current: r.native_current,
                external_current: r.external_current,
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// Unavailable custody or invalid local mailbox state.
    pub fn mail_items(&self) -> Result<Vec<MailItem>, ClientError> {
        self.0
            .mail_items()
            .map(|v| v.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }
    /// # Errors
    /// Unknown snapshot identities, invalid personal state or failed durable commit.
    pub fn organize_mail(
        &self,
        key: MailKey,
        scope: InboxScope,
        action: MailAction,
    ) -> Result<(), ClientError> {
        let ids = |ids: Vec<String>| {
            ids.into_iter()
                .map(|m| {
                    m.parse::<u128>()
                        .ok()
                        .filter(|v| *v != 0)
                        .map(cs_mail_primitives::MessageId)
                        .ok_or(ClientError::InvalidData)
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let action = match action {
            MailAction::Move { folder } => core::MailAction::Move(match folder {
                MailFolder::Inbox => core::MailFolder::Inbox,
                MailFolder::Archive => core::MailFolder::Archive,
                MailFolder::Trash => core::MailFolder::Trash,
            }),
            MailAction::Restore => core::MailAction::Restore,
            MailAction::Flag { value } => core::MailAction::Flag(value),
            MailAction::Read { displayed } => core::MailAction::Read(ids(displayed)?),
            MailAction::Unread { displayed } => core::MailAction::Unread(ids(displayed)?),
        };
        self.0
            .organize_mail(key.try_into()?, scope.try_into()?, action)
            .map_err(Into::into)
    }
    /// # Errors
    /// Unavailable custody or invalid retained operation evidence.
    pub fn outgoing_work(&self) -> Result<Vec<OutgoingWork>, ClientError> {
        self.0
            .outgoing_work()
            .map(|v| {
                v.into_iter()
                    .map(|w| OutgoingWork {
                        operation: w.operation,
                        key: w.key.into(),
                        message: w.message,
                        prepared_at: w.prepared_at,
                        resolved_at: w.resolved_at,
                        status: match w.status {
                            core::WorkStatus::AwaitingConfirmation => {
                                WorkStatus::AwaitingConfirmation
                            }
                            core::WorkStatus::Committed => WorkStatus::Committed,
                            core::WorkStatus::Queued => WorkStatus::Queued,
                            core::WorkStatus::Refused => WorkStatus::Refused,
                        },
                    })
                    .collect()
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid query or cursor, denied content access, or interrupted transport.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values across its language boundary.
    pub fn search_mail(
        &self,
        query: String,
        scope: InboxScope,
        after: u64,
        limit: u16,
    ) -> Result<SearchPage, ClientError> {
        self.0
            .search_mail(&query, scope.try_into()?, after, limit)
            .map(|p| SearchPage {
                matches: p
                    .matches
                    .into_iter()
                    .map(|m| SearchMatch {
                        key: m.key.into(),
                        message: m.message,
                        excerpt: m.excerpt,
                        expires_at: m.expires_at,
                    })
                    .collect(),
                searched: p.searched,
                unavailable: p.unavailable,
                not_retained: p.not_retained,
                next: p.next,
                total: p.total,
            })
            .map_err(Into::into)
    }
}

#[derive(Clone, uniffi::Enum)]
pub enum InboxScope {
    All,
    Alias { id: String },
}
impl TryFrom<InboxScope> for core::InboxScope {
    type Error = ClientError;
    fn try_from(s: InboxScope) -> Result<Self, Self::Error> {
        Ok(match s {
            InboxScope::All => Self::All,
            InboxScope::Alias { id } => Self::Alias(
                cs_mail_primitives::AliasId::new(id.parse().map_err(|_| ClientError::InvalidData)?)
                    .map_err(|_| ClientError::InvalidData)?,
            ),
        })
    }
}
#[derive(Clone, uniffi::Record)]
pub struct MessageRoute {
    pub message: String,
    pub alias: String,
}
#[derive(uniffi::Record)]
pub struct AliasInbox {
    pub address: super::ManagedAddress,
    pub unread: u64,
    pub items: Vec<MailItem>,
}
#[derive(uniffi::Record)]
pub struct InboxWorkspace {
    pub all: Vec<MailItem>,
    pub inboxes: Vec<AliasInbox>,
    pub sender_lanes: Vec<SenderLaneScope>,
}
#[derive(Clone, uniffi::Record)]
pub struct SenderLaneScope {
    pub sender: String,
    pub lane: String,
    pub aliases: Vec<String>,
}
#[uniffi::export]
impl AppleClient {
    /// Journal an exact native send to an explicit receiving alias, with no discovery heuristic.
    /// # Errors
    /// Invalid alias or conversation identity, unavailable authority, or failed delivery.
    #[allow(clippy::needless_pass_by_value)] // UniFFI exports own their transport strings.
    pub fn send_message_to_alias(
        &self,
        other: String,
        alias: String,
        conversation: Option<String>,
        text: String,
    ) -> Result<String, ClientError> {
        let id =
            cs_mail_primitives::AliasId::new(alias.parse().map_err(|_| ClientError::InvalidData)?)
                .map_err(|_| ClientError::InvalidData)?;
        self.0
            .send_message_to_alias(
                &other,
                id,
                conversation
                    .map(|s| s.parse().map_err(|_| ClientError::InvalidData))
                    .transpose()?,
                &text,
            )
            .map(|id| id.0.to_string())
            .map_err(Into::into)
    }
    /// # Errors
    /// Unavailable or inconsistent owner-only receiving metadata.
    pub fn inbox_workspace(&self) -> Result<InboxWorkspace, ClientError> {
        self.0.inbox_workspace().map(Into::into).map_err(Into::into)
    }
    /// # Errors
    /// Invalid local receiving evidence or unavailable custody.
    pub fn cached_inbox_workspace(&self) -> Result<InboxWorkspace, ClientError> {
        self.0
            .cached_inbox_workspace()
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid retained content, limits or unavailable local persistence.
    pub fn index_local_mail(&self, limit: u16) -> Result<u64, ClientError> {
        self.0.index_local_mail(limit).map_err(Into::into)
    }
    /// # Errors
    /// Interrupted transport, invalid content or failed protected persistence.
    pub fn retain_mail(&self, limit: u16) -> Result<RetentionProgress, ClientError> {
        self.0
            .retain_mail(limit)
            .map(|p| RetentionProgress {
                retained: p.retained,
                unavailable: p.unavailable,
                remaining: p.remaining,
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// The session locks could not be acquired.
    pub fn clear_inboxes(&self) -> Result<(), ClientError> {
        self.0.clear_inboxes().map_err(Into::into)
    }
}

#[derive(uniffi::Record)]
pub struct RetentionProgress {
    pub retained: u64,
    pub unavailable: u64,
    pub remaining: u64,
}
impl From<core::InboxWorkspace> for InboxWorkspace {
    fn from(w: core::InboxWorkspace) -> Self {
        let (all, inboxes, sender_lanes) = w.into_parts();
        InboxWorkspace {
            sender_lanes: sender_lanes
                .into_iter()
                .map(|s| SenderLaneScope {
                    sender: s.sender,
                    lane: s.lane,
                    aliases: s.aliases,
                })
                .collect(),
            all: all.into_iter().map(Into::into).collect(),
            inboxes: inboxes
                .into_iter()
                .map(|i| AliasInbox {
                    address: i.address.into(),
                    unread: i.unread,
                    items: i.items.into_iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

/// Pure MIME interpretation for import and isolated renderer checks. No network or account authority.
/// # Errors
/// Malformed, oversized or unsupported MIME.
#[uniffi::export]
pub fn email_presentation(mime: Vec<u8>) -> Result<ReadingEmail, ClientError> {
    cs_mail_correspondence::email::EmailContent::from_mime(mime)
        .and_then(|email| email.presentation())
        .map(|p| core::ReadingEmail::from(p).into())
        .map_err(|_| ClientError::InvalidData)
}
