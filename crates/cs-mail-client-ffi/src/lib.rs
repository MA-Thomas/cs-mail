//! Generated bindings for the portable member client; no domain rules live here.
use cs_mail_client::member as core;
use std::sync::Arc;
uniffi::setup_scaffolding!();
mod drafts;
mod mailbox;
pub use drafts::*;
pub use mailbox::*;
mod addresses;
pub use addresses::*;

#[derive(Debug, uniffi::Error)]
pub enum ClientError {
    InvalidAliasName,
    DraftChanged,
    CopyDeletionRefused,
    NotConfigured,
    AlreadyConfigured,
    KeyUnavailable,
    StorageUnavailable,
    InvalidData,
    Offline,
    Unauthorized,
    Unavailable,
    ExternalPending,
    ExternalRefused,
    DecisionPending,
    SendPending,
    SendRefused,
    QuoteExpired,
    RequestPending,
    RequestRefused,
    PolicyRefused,
    SourceUnavailable,
    SetupPending,
    SetupRefused,
    NoPendingRequest,
    DecisionRefused,
    PublicationRefused,
    AddressUnavailable,
    EnrollmentRequired,
    LoginUnavailable,
    LoginCancelled,
}
impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ClientError {}
impl From<core::Error> for ClientError {
    fn from(error: core::Error) -> Self {
        match error {
            core::Error::InvalidAliasName => Self::InvalidAliasName,
            core::Error::CopyDeletionRefused => Self::CopyDeletionRefused,
            core::Error::DraftChanged => Self::DraftChanged,
            core::Error::ExternalPending => Self::ExternalPending,
            core::Error::ExternalRefused => Self::ExternalRefused,
            core::Error::NotConfigured => Self::NotConfigured,
            core::Error::AlreadyConfigured => Self::AlreadyConfigured,
            core::Error::KeyUnavailable => Self::KeyUnavailable,
            core::Error::StorageUnavailable => Self::StorageUnavailable,
            core::Error::InvalidData => Self::InvalidData,
            core::Error::Offline => Self::Offline,
            core::Error::Unauthorized => Self::Unauthorized,
            core::Error::Unavailable => Self::Unavailable,
            core::Error::DecisionPending => Self::DecisionPending,
            core::Error::SendPending => Self::SendPending,
            core::Error::SendRefused => Self::SendRefused,
            core::Error::QuoteExpired => Self::QuoteExpired,
            core::Error::RequestPending => Self::RequestPending,
            core::Error::RequestRefused => Self::RequestRefused,
            core::Error::PolicyRefused => Self::PolicyRefused,
            core::Error::SourceUnavailable => Self::SourceUnavailable,
            core::Error::SetupPending => Self::SetupPending,
            core::Error::SetupRefused => Self::SetupRefused,
            core::Error::NoPendingRequest => Self::NoPendingRequest,
            core::Error::DecisionRefused => Self::DecisionRefused,
            core::Error::PublicationRefused => Self::PublicationRefused,
            core::Error::AddressUnavailable => Self::AddressUnavailable,
            core::Error::EnrollmentRequired => Self::EnrollmentRequired,
            core::Error::LoginUnavailable => Self::LoginUnavailable,
            core::Error::LoginCancelled => Self::LoginCancelled,
        }
    }
}

#[derive(uniffi::Record)]
pub struct RetainedState {
    pub revision: u64,
    pub metadata: Option<Vec<u8>>,
}
#[derive(uniffi::Record)]
pub struct RetainedIndexRow {
    pub id: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
#[derive(uniffi::Enum)]
pub enum RetainedCopyWrite {
    Keep,
    Replace { bytes: Vec<u8> },
    Remove,
}
#[derive(uniffi::Record)]
pub struct RetainedWrite {
    pub index: RetainedIndexRow,
    pub copy: RetainedCopyWrite,
}

#[uniffi::export(callback_interface)]
pub trait AppleStore: Send + Sync {
    /// # Errors
    /// Retained metadata or progress cannot be read.
    fn retained_state(&self) -> Result<RetainedState, ClientError>;
    /// # Errors
    /// Index storage cannot be read.
    fn retained_indexes(
        &self,
        after: Option<Vec<u8>>,
        limit: u16,
    ) -> Result<Vec<RetainedIndexRow>, ClientError>;
    /// # Errors
    /// Copy storage cannot be read.
    fn retained_copy(&self, id: Vec<u8>) -> Result<Option<Vec<u8>>, ClientError>;
    /// # Errors
    /// Stale revision or failed atomic commit.
    fn commit_retained(
        &self,
        expected_revision: u64,
        metadata: Option<Vec<u8>>,
        writes: Vec<RetainedWrite>,
    ) -> Result<(), ClientError>;
    /// # Errors
    /// Missing credential must be NotConfigured; all custody denials must fail closed.
    fn load_secret(&self) -> Result<Vec<u8>, ClientError>;
    /// # Errors
    /// Denied custody or an existing credential.
    fn create_secret(&self, bytes: Vec<u8>) -> Result<(), ClientError>;
    /// # Errors
    /// Cache read failed.
    fn load_cache(&self) -> Result<Option<Vec<u8>>, ClientError>;
    /// # Errors
    /// Atomic durable cache replacement failed.
    fn save_cache(&self, bytes: Vec<u8>) -> Result<(), ClientError>;
    /// # Errors
    /// Workspace storage cannot be read.
    fn load_workspace(&self) -> Result<Option<Vec<u8>>, ClientError>;
    /// # Errors
    /// Atomic durable workspace replacement failed.
    fn save_workspace(&self, bytes: Vec<u8>) -> Result<(), ClientError>;
    /// # Errors
    /// The complete cache/workspace transaction failed and must roll back.
    fn load_personal(&self) -> Result<Option<Vec<u8>>, ClientError>;
    /// # Errors
    /// Durable personal-state replacement failed.
    fn save_personal(&self, bytes: Vec<u8>) -> Result<(), ClientError>;
    /// # Errors
    /// The complete three-record transaction failed; all writes must roll back.
    fn save_submission(
        &self,
        cache: Vec<u8>,
        workspace: Vec<u8>,
        personal: Vec<u8>,
    ) -> Result<(), ClientError>;
}
struct Store(Box<dyn AppleStore>);
impl core::DeviceStore for Store {
    fn retained_state(&self) -> Result<(u64, Option<Vec<u8>>), core::Error> {
        let state = self
            .0
            .retained_state()
            .map_err(|_| core::Error::StorageUnavailable)?;
        Ok((state.revision, state.metadata))
    }
    fn retained_indexes(
        &self,
        after: Option<core::RetainedRecordId>,
        limit: u16,
    ) -> Result<Vec<core::RetainedIndexRow>, core::Error> {
        self.0
            .retained_indexes(after.map(|id| id.bytes().to_vec()), limit)
            .map_err(|_| core::Error::StorageUnavailable)?
            .into_iter()
            .map(|row| {
                Ok(core::RetainedIndexRow {
                    id: row.id.try_into().map_err(|_| core::Error::InvalidData)?,
                    ciphertext: row.ciphertext,
                })
            })
            .collect()
    }
    fn retained_copy(&self, id: core::RetainedRecordId) -> Result<Option<Vec<u8>>, core::Error> {
        self.0
            .retained_copy(id.bytes().to_vec())
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn commit_retained(&self, change: core::RetainedCommit) -> Result<(), core::Error> {
        let writes = change
            .writes
            .into_iter()
            .map(|w| RetainedWrite {
                index: RetainedIndexRow {
                    id: w.index.id.bytes().to_vec(),
                    ciphertext: w.index.ciphertext,
                },
                copy: match w.copy {
                    core::RetainedCopyWrite::Keep => RetainedCopyWrite::Keep,
                    core::RetainedCopyWrite::Replace(bytes) => RetainedCopyWrite::Replace { bytes },
                    core::RetainedCopyWrite::Remove => RetainedCopyWrite::Remove,
                },
            })
            .collect();
        self.0
            .commit_retained(change.expected_revision, change.metadata, writes)
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn load_secret(&self) -> Result<Vec<u8>, core::Error> {
        self.0.load_secret().map_err(|e| {
            if matches!(e, ClientError::NotConfigured) {
                core::Error::NotConfigured
            } else {
                core::Error::KeyUnavailable
            }
        })
    }
    fn create_secret(&self, bytes: &[u8]) -> Result<(), core::Error> {
        self.0
            .create_secret(bytes.to_vec())
            .map_err(|_| core::Error::KeyUnavailable)
    }
    fn load_cache(&self) -> Result<Option<Vec<u8>>, core::Error> {
        self.0
            .load_cache()
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn save_cache(&self, bytes: &[u8]) -> Result<(), core::Error> {
        self.0
            .save_cache(bytes.to_vec())
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn load_workspace(&self) -> Result<Option<Vec<u8>>, core::Error> {
        self.0
            .load_workspace()
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn save_workspace(&self, bytes: &[u8]) -> Result<(), core::Error> {
        self.0
            .save_workspace(bytes.to_vec())
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn load_personal(&self) -> Result<Option<Vec<u8>>, core::Error> {
        self.0
            .load_personal()
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn save_personal(&self, bytes: &[u8]) -> Result<(), core::Error> {
        self.0
            .save_personal(bytes.to_vec())
            .map_err(|_| core::Error::StorageUnavailable)
    }
    fn save_submission(
        &self,
        cache: &[u8],
        workspace: &[u8],
        personal: &[u8],
    ) -> Result<(), core::Error> {
        self.0
            .save_submission(cache.to_vec(), workspace.to_vec(), personal.to_vec())
            .map_err(|_| core::Error::StorageUnavailable)
    }
}

#[derive(uniffi::Record)]
pub struct EnrollmentDetails {
    pub name: String,
    pub date_of_birth: String,
    pub address: String,
    pub phone: String,
    pub bank_account: String,
}
#[derive(uniffi::Enum)]
pub enum EnrollmentStatus {
    AwaitingLogin,
    ReviewRequired,
    UnderReview,
    ReviewApproved,
    Denied,
    Ready,
}
impl From<core::EnrollmentStatus> for EnrollmentStatus {
    fn from(value: core::EnrollmentStatus) -> Self {
        match value {
            core::EnrollmentStatus::AwaitingLogin => Self::AwaitingLogin,
            core::EnrollmentStatus::ReviewRequired => Self::ReviewRequired,
            core::EnrollmentStatus::UnderReview => Self::UnderReview,
            core::EnrollmentStatus::ReviewApproved => Self::ReviewApproved,
            core::EnrollmentStatus::Denied => Self::Denied,
            core::EnrollmentStatus::Ready => Self::Ready,
        }
    }
}
#[derive(uniffi::Enum)]
pub enum LoginStart {
    Browser {
        url: String,
        callback_scheme: String,
    },
    Status {
        status: EnrollmentStatus,
    },
}

#[derive(uniffi::Record)]
pub struct Inbox {
    pub last_send: Option<SendOutcome>,
    pub pending_request: bool,
    pub request_progress: Option<RequestProgress>,
    pub pending_policy: bool,
    pub pending_deletion: Option<String>,
    pub pending_send: Option<String>,
    pub account: String,
    pub persona: String,
    pub synchronized_at: Option<u64>,
    pub conversations: Vec<Conversation>,
    pub pending_sender: Option<String>,
    pub pending_setup: Option<PendingSetup>,
    pub last_decision: Option<String>,
}
#[derive(uniffi::Enum)]
pub enum PendingSetup {
    DeviceRegistration,
    ContentKeyPublication,
    ServicePurchase,
    DirectoryPublication,
    AliasManagement,
}
impl From<core::PendingSetup> for PendingSetup {
    fn from(value: core::PendingSetup) -> Self {
        match value {
            core::PendingSetup::DeviceRegistration => Self::DeviceRegistration,
            core::PendingSetup::ContentKeyPublication => Self::ContentKeyPublication,
            core::PendingSetup::ServicePurchase => Self::ServicePurchase,
            core::PendingSetup::DirectoryPublication => Self::DirectoryPublication,
            core::PendingSetup::AliasManagement => Self::AliasManagement,
        }
    }
}
#[derive(uniffi::Record)]
pub struct Conversation {
    pub sent_requests: Vec<Request>,
    pub controls: RelationshipControls,
    pub threads: Vec<ConversationSummary>,
    pub reply: ReplyRoute,
    pub request_entry: RequestEntry,
    pub sender: String,
    pub title: String,
    pub permission: String,
    pub permission_route: PermissionRoute,
    pub external: bool,
    pub requests: Vec<Request>,
    pub messages: Vec<Message>,
}
#[derive(uniffi::Record)]
pub struct NavigationLane {
    pub id: String,
    pub expires_at: u64,
    pub scope: String,
    pub max_messages: u32,
    pub interval_ms: u64,
}
#[derive(uniffi::Record)]
pub struct NavigationContact {
    pub sender: String,
    pub title: String,
    pub external: bool,
    pub requests: Vec<NavigationRequest>,
    pub sent_requests: Vec<NavigationRequest>,
    pub incoming_lane: Option<NavigationLane>,
    pub outgoing_lane: Option<NavigationLane>,
}
#[derive(uniffi::Record)]
pub struct NavigationRequest {
    pub record: Request,
    pub issued_at: u64,
    pub financial_summary: String,
}
impl From<core::NavigationRequest> for NavigationRequest {
    fn from(value: core::NavigationRequest) -> Self {
        Self {
            record: request_record(value.record),
            issued_at: value.issued_at,
            financial_summary: value.financial_summary,
        }
    }
}
impl From<core::NavigationLane> for NavigationLane {
    fn from(value: core::NavigationLane) -> Self {
        Self {
            id: value.id,
            expires_at: value.expires_at,
            scope: value.scope,
            max_messages: value.max_messages,
            interval_ms: value.interval_ms,
        }
    }
}
impl From<core::NavigationContact> for NavigationContact {
    fn from(value: core::NavigationContact) -> Self {
        Self {
            sender: value.sender,
            title: value.title,
            external: value.external,
            requests: value.requests.into_iter().map(Into::into).collect(),
            sent_requests: value.sent_requests.into_iter().map(Into::into).collect(),
            incoming_lane: value.incoming_lane.map(Into::into),
            outgoing_lane: value.outgoing_lane.map(Into::into),
        }
    }
}
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum RequestEntry {
    Unverified,
    OwnAddress,
    AlreadyPermitted,
    ReviewApproaches,
    IncomingPending { request: String },
    OutgoingPending { request: String },
    Unavailable,
}
impl From<core::RequestEntry> for RequestEntry {
    fn from(value: core::RequestEntry) -> Self {
        match value {
            core::RequestEntry::Unverified => Self::Unverified,
            core::RequestEntry::OwnAddress => Self::OwnAddress,
            core::RequestEntry::AlreadyPermitted => Self::AlreadyPermitted,
            core::RequestEntry::ReviewApproaches => Self::ReviewApproaches,
            core::RequestEntry::IncomingPending { request } => Self::IncomingPending {
                request: request.0.to_string(),
            },
            core::RequestEntry::OutgoingPending { request } => Self::OutgoingPending {
                request: request.0.to_string(),
            },
            core::RequestEntry::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum PermissionRoute {
    Standing,
    Bounded,
    Closed,
}
impl From<core::PermissionRoute> for PermissionRoute {
    fn from(value: core::PermissionRoute) -> Self {
        match value {
            core::PermissionRoute::Standing => Self::Standing,
            core::PermissionRoute::Bounded => Self::Bounded,
            core::PermissionRoute::Closed => Self::Closed,
        }
    }
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum FundingState {
    Pending,
    CancellationRequested,
    Finalized,
    Voided,
    Reversed,
}
impl From<cs_mail_finance::CaptureStatus> for FundingState {
    fn from(value: cs_mail_finance::CaptureStatus) -> Self {
        match value {
            cs_mail_finance::CaptureStatus::Pending => Self::Pending,
            cs_mail_finance::CaptureStatus::CancellationRequested => Self::CancellationRequested,
            cs_mail_finance::CaptureStatus::FundingFinalized => Self::Finalized,
            cs_mail_finance::CaptureStatus::Voided => Self::Voided,
            cs_mail_finance::CaptureStatus::Reversed => Self::Reversed,
        }
    }
}
#[derive(uniffi::Record)]
pub struct Request {
    pub id: String,
    pub capture: Option<FundingState>,
    pub class_name: String,
    pub status: String,
    pub deadline: Option<u64>,
    pub awaiting_decision: bool,
    pub financial_status: String,
    pub has_pool_forfeiture: bool,
}
#[derive(uniffi::Record)]
pub struct Message {
    pub committed_at: Option<u64>,
    pub sequence: Option<u64>,
    pub content: ReadingContent,
    pub conversation: Option<String>,
    pub outgoing: bool,
    pub id: String,
    pub text: String,
    pub references: Vec<u32>,
}
#[derive(Clone, Copy, uniffi::Enum)]
pub enum Decision {
    Accept,
    Reject,
    ExpressLane { lifetime_ms: u64 },
}
impl From<core::Inbox> for Inbox {
    fn from(value: core::Inbox) -> Self {
        Self {
            last_send: value.last_send.map(Into::into),
            pending_request: value.pending_request,
            request_progress: value.request_progress.map(Into::into),
            pending_policy: value.pending_policy,
            pending_deletion: value.pending_deletion,
            pending_send: value.pending_send,
            account: value.account,
            persona: value.persona,
            synchronized_at: value.synchronized_at,
            conversations: value.conversations.into_iter().map(Into::into).collect(),
            pending_sender: value.pending_sender,
            pending_setup: value.pending_setup.map(Into::into),
            last_decision: value.last_decision,
        }
    }
}
impl From<core::Conversation> for Conversation {
    fn from(value: core::Conversation) -> Self {
        Self {
            sent_requests: value
                .sent_requests
                .into_iter()
                .map(request_record)
                .collect(),
            controls: value.controls.into(),
            threads: value.threads.into_iter().map(Into::into).collect(),
            reply: value.reply.into(),
            request_entry: value.request_entry.into(),
            sender: value.sender,
            title: value.title,
            permission: value.permission,
            permission_route: value.permission_route.into(),
            external: value.external,
            requests: value.requests.into_iter().map(request_record).collect(),
            messages: value
                .messages
                .into_iter()
                .map(|m| Message {
                    committed_at: m.committed_at,
                    sequence: m.sequence,
                    content: m.content.into(),
                    conversation: m.conversation,
                    outgoing: m.outgoing,
                    id: m.id,
                    text: m.text,
                    references: m.references,
                })
                .collect(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct AppleClient(core::MemberClient);

#[derive(uniffi::Record)]
pub struct RequestClassDraft {
    pub id: String,
    pub description: String,
    pub collateral_minor_units: u64,
}
// UniFFI exports owned arguments across the foreign-language boundary.
#[allow(clippy::needless_pass_by_value)]
#[uniffi::export]
impl AppleClient {
    /// Construct and call on a worker queue, never the main actor.
    /// # Errors
    /// HTTP runtime cannot initialize.
    #[uniffi::constructor]
    pub fn new(store: Box<dyn AppleStore>) -> Result<Self, ClientError> {
        Ok(Self(core::MemberClient::new(
            Arc::new(Store(store)),
            Arc::new(core::HttpTransport::new()?),
            Arc::new(|| {
                let millis = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                cs_mail_primitives::CanonicalTime(u64::try_from(millis).unwrap_or(u64::MAX))
            }),
        )))
    }
    /// # Errors
    /// Invalid enrollment input, denied key custody, login configuration or transport failure.
    pub fn begin_login(
        &self,
        server: String,
        details: EnrollmentDetails,
    ) -> Result<LoginStart, ClientError> {
        self.0
            .begin_login(
                &server,
                core::EnrollmentDetails {
                    name: details.name,
                    date_of_birth: details.date_of_birth,
                    address: details.address,
                    phone: details.phone,
                    bank_account: details.bank_account,
                },
            )
            .map(|v| match v {
                core::LoginStart::Browser(a) => LoginStart::Browser {
                    url: a.url,
                    callback_scheme: a.callback_scheme,
                },
                core::LoginStart::Status(s) => LoginStart::Status { status: s.into() },
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// Expired or substituted login, refused enrollment or uncertain network outcome.
    pub fn finish_login(&self, callback: String) -> Result<EnrollmentStatus, ClientError> {
        self.0
            .finish_login(&callback)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Network, custody or durable device setup failed.
    pub fn enrollment_status(&self) -> Result<EnrollmentStatus, ClientError> {
        self.0
            .enrollment_status()
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// The client lock failed.
    pub fn cancel_login(&self) -> Result<(), ClientError> {
        self.0.cancel_login().map_err(Into::into)
    }
    /// # Errors
    /// Local cache or key custody is unavailable.
    pub fn cached_inbox(&self) -> Result<Inbox, ClientError> {
        self.0.cached_inbox().map(Into::into).map_err(Into::into)
    }
    /// # Errors
    /// Network, authorization or durable cache replacement failed.
    pub fn sync(&self) -> Result<Inbox, ClientError> {
        self.0.sync().map(Into::into).map_err(Into::into)
    }
    /// # Errors
    /// Custody, authentication, transport or navigation record binding failed.
    pub fn navigation(&self) -> Result<Vec<NavigationContact>, ClientError> {
        self.0
            .navigation()
            .map(|contacts| contacts.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }
    /// # Errors
    /// A different setup is unresolved, or signing, authority or transport fails.
    pub fn purchase_service(&self, offer: u64) -> Result<bool, ClientError> {
        self.0
            .purchase_service(cs_mail_primitives::PolicyVersion(offer))
            .map_err(Into::into)
    }
    /// # Errors
    /// Unavailable local custody or invalid retained correspondence.
    pub fn cached_conversation(&self, sender: String) -> Result<Conversation, ClientError> {
        self.0
            .cached_conversation(&sender)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Authority, binding or decryption failed.
    pub fn conversation(&self, sender: String) -> Result<Conversation, ClientError> {
        self.0
            .conversation(&sender)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// An operation is pending, authority failed or the outcome is unknown/refused.
    pub fn decide(
        &self,
        sender: String,
        request: String,
        decision: Decision,
    ) -> Result<Inbox, ClientError> {
        self.0
            .decide(
                &sender,
                cs_mail_primitives::RequestId(
                    request.parse().map_err(|_| ClientError::InvalidData)?,
                ),
                match decision {
                    Decision::Accept => core::Decision::Accept,
                    Decision::Reject => core::Decision::Reject,
                    Decision::ExpressLane { lifetime_ms } => {
                        core::Decision::ExpressLane { lifetime_ms }
                    }
                },
            )
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Preserves the exact pending send through uncertainty and custody failure.
    pub fn send_message(
        &self,
        other: String,
        conversation: Option<String>,
        text: String,
        sources: Vec<SourceDraft>,
    ) -> Result<String, ClientError> {
        let conversation = conversation
            .map(|id| id.parse().map_err(|_| ClientError::InvalidData))
            .transpose()?;
        let sources = sources
            .into_iter()
            .map(|source| {
                let message = cs_mail_primitives::MessageId(
                    source
                        .message
                        .parse()
                        .map_err(|_| ClientError::InvalidData)?,
                );
                if message.0 == 0 {
                    return Err(ClientError::InvalidData);
                }
                Ok(core::SourceDraft {
                    selection: source_selection(message, source.version, source.elements)?,
                    mode: match source.mode {
                        ReuseMode::Quotation => core::ReuseMode::Quotation,
                        ReuseMode::Sharing => core::ReuseMode::Sharing,
                        ReuseMode::Reference => core::ReuseMode::Reference,
                    },
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        self.0
            .send_message(&other, conversation, &text, &sources)
            .map(|id| id.0.to_string())
            .map_err(Into::into)
    }
    /// # Errors
    /// The original operation still has no authenticated outcome.
    pub fn retry(&self) -> Result<Inbox, ClientError> {
        self.0.retry().map(Into::into).map_err(Into::into)
    }
}

#[derive(Clone, uniffi::Record)]
pub struct DirectoryClass {
    pub id: String,
    pub description: String,
    pub collateral_minor_units: u64,
    pub conditional_minor_units: Option<u64>,
    pub quotable: bool,
}
#[derive(uniffi::Record)]
pub struct AddressSettings {
    pub alias: String,
    pub covered: bool,
    pub address: String,
    pub mailbox: Option<String>,
    pub link: String,
    pub revision: u64,
    pub class_version: u64,
    pub display_name: String,
    pub description: String,
    pub image: Option<Vec<u8>>,
    pub listed: bool,
    pub classes: Vec<DirectoryClass>,
    pub pending: bool,
    pub processing_minor_units: Option<u64>,
    pub collateral_min: Option<u64>,
    pub collateral_max: Option<u64>,
}
#[derive(uniffi::Record)]
pub struct Approach {
    pub alias: String,
    pub request_entry: RequestEntry,
    pub address: String,
    pub mailbox: Option<String>,
    pub display_name: String,
    pub description: String,
    pub image: Option<Vec<u8>>,
    pub covered: bool,
    pub processing_minor_units: Option<u64>,
    pub classes: Vec<DirectoryClass>,
}
impl From<cs_mail_application::directory::Approach> for Approach {
    fn from(value: cs_mail_application::directory::Approach) -> Self {
        Self {
            alias: value.entry.alias().value().to_string(),
            request_entry: RequestEntry::Unverified,
            address: value.entry.address().0.to_string(),
            mailbox: value.entry.mailbox().map(|m| m.as_str().to_owned()),
            display_name: value.entry.profile().display_name().into(),
            description: value.entry.profile().description().into(),
            image: value.entry.profile().image().map(|i| i.bytes().to_vec()),
            covered: value.covered,
            processing_minor_units: value
                .offer
                .as_ref()
                .map(|o| o.processing_charge.minor_units()),
            classes: value.offer.map_or_else(Vec::new, |o| {
                o.classes
                    .into_iter()
                    .map(|c| DirectoryClass {
                        id: c.id().0.to_string(),
                        description: c.description().into(),
                        collateral_minor_units: c.collateral().minor_units(),
                        conditional_minor_units: o
                            .processing_charge
                            .checked_add(c.collateral())
                            .map(cs_mail_primitives::Money::minor_units),
                        quotable: true,
                    })
                    .collect()
            }),
        }
    }
}
#[derive(uniffi::Record)]
pub struct DirectoryPage {
    pub entries: Vec<Approach>,
    pub next_offset: Option<u32>,
}
#[allow(clippy::needless_pass_by_value)]
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Missing custody, server authority or directory configuration.
    pub fn address_settings(&self, alias: String) -> Result<AddressSettings, ClientError> {
        let alias =
            cs_mail_primitives::AliasId::new(alias.parse().map_err(|_| ClientError::InvalidData)?)
                .map_err(|_| ClientError::InvalidData)?;
        let route = self
            .0
            .aliases()?
            .into_iter()
            .find(|a| a.id() == alias)
            .ok_or(ClientError::InvalidData)?;
        let (address, link, pending) = self.0.directory_identity(alias)?;
        let settings = self.0.directory_settings(alias)?;
        Ok(AddressSettings {
            alias: alias.value().to_string(),
            covered: self.0.service_covered()?,
            mailbox: Some(route.address().as_str().to_owned()),
            address,
            link,
            pending,
            revision: settings.entry.as_ref().map_or(0, |e| e.revision().0),
            class_version: settings.classes.as_ref().map_or(
                0,
                cs_mail_protocol::pricing::RecipientRequestClasses::version,
            ),
            display_name: settings
                .entry
                .as_ref()
                .map_or_else(String::new, |e| e.profile().display_name().into()),
            description: settings
                .entry
                .as_ref()
                .map_or_else(String::new, |e| e.profile().description().into()),
            image: settings
                .entry
                .as_ref()
                .and_then(|e| e.profile().image().map(|i| i.bytes().to_vec())),
            listed: settings.entry.as_ref().is_some_and(|e| {
                e.visibility() == cs_mail_accounts::directory::DiscoveryVisibility::Listed
            }),
            classes: settings.classes.map_or_else(Vec::new, |c| {
                c.classes()
                    .iter()
                    .map(|class| DirectoryClass {
                        id: class.id().0.to_string(),
                        description: class.description().into(),
                        collateral_minor_units: class.collateral().minor_units(),
                        conditional_minor_units: settings
                            .policy
                            .as_ref()
                            .and_then(|p| p.processing_charge().checked_add(class.collateral()))
                            .map(cs_mail_primitives::Money::minor_units),
                        quotable: settings.policy.as_ref().is_some_and(|p| p.quotable(class)),
                    })
                    .collect()
            }),
            processing_minor_units: settings
                .policy
                .as_ref()
                .map(|p| p.processing_charge().minor_units()),
            collateral_min: settings
                .policy
                .as_ref()
                .map(|p| p.collateral_min().minor_units()),
            collateral_max: settings
                .policy
                .as_ref()
                .map(|p| p.collateral_max().minor_units()),
        })
    }
    /// # Errors
    /// Invalid profile/classes, conflicting publication or uncertain network outcome.
    #[allow(clippy::too_many_arguments)] // Foreign interface accepts the reviewed publication as owned fields.
    pub fn publish_address(
        &self,
        alias: String,
        expected: u64,
        class_version: u64,
        display_name: String,
        description: String,
        image: Option<Vec<u8>>,
        listed: bool,
        classes: Vec<RequestClassDraft>,
        mailbox: Option<String>,
    ) -> Result<(), ClientError> {
        let id =
            cs_mail_primitives::AliasId::new(alias.parse().map_err(|_| ClientError::InvalidData)?)
                .map_err(|_| ClientError::InvalidData)?;
        let route = self
            .0
            .aliases()?
            .into_iter()
            .find(|a| a.id() == id)
            .ok_or(ClientError::InvalidData)?;
        let mailbox = Some(match mailbox {
            Some(value) => cs_mail_accounts::directory::MemberAddress::try_from(value)
                .map_err(|_| ClientError::InvalidData)?,
            None => route.address().clone(),
        });
        let image = image
            .map(cs_mail_accounts::directory::ProfileImage::try_from)
            .transpose()
            .map_err(|_| ClientError::InvalidData)?;
        let profile =
            cs_mail_accounts::directory::PublicProfile::new(display_name, description, image)
                .map_err(|_| ClientError::InvalidData)?;
        let classes = classes
            .into_iter()
            .map(|c| {
                cs_mail_protocol::pricing::RequestClass::new(
                    cs_mail_primitives::RequestClassId(
                        c.id.parse().map_err(|_| ClientError::InvalidData)?,
                    ),
                    c.description,
                    cs_mail_primitives::Money::from_minor_units(c.collateral_minor_units),
                )
                .map_err(|_| ClientError::InvalidData)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.0
            .publish_directory(
                cs_mail_primitives::AliasId::new(
                    alias.parse().map_err(|_| ClientError::InvalidData)?,
                )
                .map_err(|_| ClientError::InvalidData)?,
                cs_mail_accounts::directory::DirectoryRevision(expected),
                class_version,
                profile,
                if listed {
                    cs_mail_accounts::directory::DiscoveryVisibility::Listed
                } else {
                    cs_mail_accounts::directory::DiscoveryVisibility::Unlisted
                },
                classes,
                mailbox,
            )
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid or unavailable public search.
    pub fn directory_search(
        &self,
        text: String,
        offset: u32,
    ) -> Result<DirectoryPage, ClientError> {
        let query = cs_mail_accounts::directory::DirectoryQuery::new(text, offset)
            .map_err(|_| ClientError::InvalidData)?;
        let page = self.0.directory_search(&query)?;
        Ok(DirectoryPage {
            entries: page.entries.into_iter().map(Into::into).collect(),
            next_offset: page.next_offset,
        })
    }
    /// # Errors
    /// Unknown/unavailable address or network failure.
    pub fn directory_lookup(&self, reference: String) -> Result<Approach, ClientError> {
        let public = self
            .0
            .directory_lookup(&reference)
            .map_err(ClientError::from)?;
        let entry = self
            .0
            .relationship_request_entry(public.entry.address())
            .map_err(ClientError::from)?;
        let mut result = Approach::from(public);
        result.request_entry = entry.into();
        Ok(result)
    }
}

fn request_record(r: core::Request) -> Request {
    Request {
        id: r.id,
        capture: r.capture.map(Into::into),
        class_name: r.class_name,
        status: r.status,
        deadline: r.deadline,
        awaiting_decision: r.awaiting_decision,
        financial_status: r.financial_status,
        has_pool_forfeiture: r.has_pool_forfeiture,
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationSummary {
    pub id: String,
    pub label: String,
    pub request: Option<String>,
    pub sent_request: Option<String>,
    pub last_at: u64,
    pub reply_allowed: bool,
    pub policy: Option<ConversationPolicy>,
}
impl From<core::ConversationSummary> for ConversationSummary {
    fn from(value: core::ConversationSummary) -> Self {
        Self {
            id: value.id,
            label: value.label,
            request: value.request,
            sent_request: value.sent_request,
            last_at: value.last_at,
            reply_allowed: value.reply_allowed,
            policy: value.policy.map(Into::into),
        }
    }
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ReplyRoute {
    Standing,
    Lane,
    Conversation,
    Closed,
}
impl From<core::ReplyRoute> for ReplyRoute {
    fn from(value: core::ReplyRoute) -> Self {
        match value {
            core::ReplyRoute::Standing => Self::Standing,
            core::ReplyRoute::Lane => Self::Lane,
            core::ReplyRoute::Conversation => Self::Conversation,
            core::ReplyRoute::Closed => Self::Closed,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum SendOutcome {
    Committed { message: String },
    Refused { operation: String },
}
impl From<core::SendOutcome> for SendOutcome {
    fn from(value: core::SendOutcome) -> Self {
        match value {
            core::SendOutcome::Committed(message) => Self::Committed {
                message: message.0.to_string(),
            },
            core::SendOutcome::Refused { operation, .. } => Self::Refused {
                operation: operation.0.to_string(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum RelationshipControl {
    GrantStanding,
    GrantLane {
        lifetime_ms: u64,
    },
    Revoke,
    Block,
    Unblock,
    Followups {
        max_messages: u32,
        max_per_interval: u32,
        interval_ms: u64,
    },
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Current custody, authority, unresolved work or canonical permission refusal.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values crossing the language boundary.
    pub fn control_relationship(
        &self,
        sender: String,
        control: RelationshipControl,
    ) -> Result<Inbox, ClientError> {
        let control = match control {
            RelationshipControl::GrantStanding => core::RelationshipControl::GrantStanding,
            RelationshipControl::GrantLane { lifetime_ms } => {
                core::RelationshipControl::GrantLane { lifetime_ms }
            }
            RelationshipControl::Revoke => core::RelationshipControl::Revoke,
            RelationshipControl::Block => core::RelationshipControl::Block,
            RelationshipControl::Unblock => core::RelationshipControl::Unblock,
            RelationshipControl::Followups {
                max_messages,
                max_per_interval,
                interval_ms,
            } => core::RelationshipControl::Followups {
                max_messages,
                max_per_interval,
                interval_ms,
            },
        };
        self.0
            .control_relationship(&sender, control)
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LaneExpiryReminder {
    pub expires_at: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RelationshipControls {
    pub state: ContactState,
    pub revocable: bool,
    pub followup_max: u32,
    pub followup_per_interval: u32,
    pub followup_interval_ms: u64,
    pub lane: Option<String>,
    pub lane_expiry_reminder: Option<LaneExpiryReminder>,
}
impl From<core::RelationshipControls> for RelationshipControls {
    fn from(value: core::RelationshipControls) -> Self {
        Self {
            state: match value.state {
                core::ContactState::Absent => ContactState::Absent,
                core::ContactState::Closed => ContactState::Closed,
                core::ContactState::Standing => ContactState::Standing,
                core::ContactState::Lane => ContactState::Lane,
                core::ContactState::Blocked => ContactState::Blocked,
                core::ContactState::Revoked => ContactState::Revoked,
            },
            revocable: value.revocable,
            followup_max: value.followup_max,
            followup_per_interval: value.followup_per_interval,
            followup_interval_ms: value.followup_interval_ms,
            lane: value.lane,
            lane_expiry_reminder: value
                .lane_expiry_reminder
                .map(|reminder| LaneExpiryReminder {
                    expires_at: reminder.expires_at.0,
                }),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceDraft {
    pub message: String,
    pub version: Vec<u8>,
    pub elements: Vec<SourceElement>,
    pub mode: ReuseMode,
}
#[derive(Clone, Debug, uniffi::Enum)]
pub enum SourceElement {
    TextRange { block: u32, start: u32, end: u32 },
    ImageBlock { block: u32 },
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceMessage {
    pub message: String,
    pub version: Vec<u8>,
    pub blocks: Vec<SourceBlock>,
}
#[derive(Clone, Debug, uniffi::Enum)]
pub enum SourceBlock {
    Text { block: u32, text: String },
    Image { block: u32, bytes: Vec<u8> },
    Reference { block: u32 },
}
fn source_selection(
    message: cs_mail_primitives::MessageId,
    version: Vec<u8>,
    elements: Vec<SourceElement>,
) -> Result<cs_mail_correspondence::MessageSelection, ClientError> {
    cs_mail_correspondence::MessageSelection::new(
        message,
        cs_mail_correspondence::MessageVersion(
            version.try_into().map_err(|_| ClientError::InvalidData)?,
        ),
        elements
            .into_iter()
            .map(|element| match element {
                SourceElement::TextRange { block, start, end } => {
                    cs_mail_correspondence::SelectionElement::TextRange { block, start, end }
                }
                SourceElement::ImageBlock { block } => {
                    cs_mail_correspondence::SelectionElement::ImageBlock { block }
                }
            })
            .collect(),
    )
    .map_err(|_| ClientError::InvalidData)
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ReuseMode {
    Quotation,
    Sharing,
    Reference,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationPolicy {
    pub restricted: bool,
    pub revision: u64,
    pub relaxation: Relaxation,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum Relaxation {
    None,
    ProposedByMe,
    AwaitingMyApproval,
}
impl From<core::ConversationPolicy> for ConversationPolicy {
    fn from(value: core::ConversationPolicy) -> Self {
        Self {
            restricted: value.restricted,
            revision: value.revision,
            relaxation: match value.relaxation {
                core::Relaxation::None => Relaxation::None,
                core::Relaxation::ProposedByMe => Relaxation::ProposedByMe,
                core::Relaxation::AwaitingMyApproval => Relaxation::AwaitingMyApproval,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum PolicyAction {
    Restrict,
    ProposeRelaxation,
    ApproveRelaxation,
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Invalid selection, stale revision, custody denial or a refused/unknown outcome.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values crossing the language boundary.
    pub fn change_conversation_policy(
        &self,
        conversation: String,
        revision: u64,
        action: PolicyAction,
    ) -> Result<(), ClientError> {
        self.0
            .change_conversation_policy(
                conversation.parse().map_err(|_| ClientError::InvalidData)?,
                revision,
                match action {
                    PolicyAction::Restrict => core::PolicyAction::Restrict,
                    PolicyAction::ProposeRelaxation => core::PolicyAction::ProposeRelaxation,
                    PolicyAction::ApproveRelaxation => core::PolicyAction::ApproveRelaxation,
                },
            )
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum RequestStatus {
    Quoting,
    CommunicationAvailable,
    ExistingRequest,
    Review,
    Funding,
    FundingVoided,
    FundingReversed,
    Sending,
    Stopped,
    Submitted,
    FollowupSent,
    Cancelling,
    Cancelled,
    Refused,
    Changed,
}
impl From<core::RequestStatus> for RequestStatus {
    fn from(value: core::RequestStatus) -> Self {
        match value {
            core::RequestStatus::Quoting => Self::Quoting,
            core::RequestStatus::CommunicationAvailable => Self::CommunicationAvailable,
            core::RequestStatus::ExistingRequest => Self::ExistingRequest,
            core::RequestStatus::Review => Self::Review,
            core::RequestStatus::Funding => Self::Funding,
            core::RequestStatus::FundingVoided => Self::FundingVoided,
            core::RequestStatus::FundingReversed => Self::FundingReversed,
            core::RequestStatus::Sending => Self::Sending,
            core::RequestStatus::Stopped => Self::Stopped,
            core::RequestStatus::Submitted => Self::Submitted,
            core::RequestStatus::FollowupSent => Self::FollowupSent,
            core::RequestStatus::Cancelling => Self::Cancelling,
            core::RequestStatus::Cancelled => Self::Cancelled,
            core::RequestStatus::Refused => Self::Refused,
            core::RequestStatus::Changed => Self::Changed,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct RequestQuote {
    pub alias: String,
    pub class_id: String,
    pub class_name: String,
    pub processing: u64,
    pub collateral: u64,
    pub total: u64,
    pub expires_at: u64,
    pub submission_window_ms: u64,
    pub decision_window_ms: u64,
    pub reply_lifetime_ms: u64,
    pub reply_max_messages: u32,
    pub reply_interval_ms: u64,
    pub reply_purpose: String,
}
impl From<core::RequestQuote> for RequestQuote {
    fn from(value: core::RequestQuote) -> Self {
        Self {
            alias: value.alias,
            class_id: value.class_id,
            class_name: value.class_name,
            processing: value.processing,
            collateral: value.collateral,
            total: value.total,
            expires_at: value.expires_at,
            submission_window_ms: value.submission_window_ms,
            decision_window_ms: value.decision_window_ms,
            reply_lifetime_ms: value.reply_lifetime_ms,
            reply_max_messages: value.reply_max_messages,
            reply_interval_ms: value.reply_interval_ms,
            reply_purpose: value.reply_purpose,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct RequestProgress {
    pub capture: Option<FundingState>,
    pub other: String,
    pub request: String,
    pub status: RequestStatus,
    pub quote: Option<RequestQuote>,
}
impl From<core::RequestProgress> for RequestProgress {
    fn from(value: core::RequestProgress) -> Self {
        Self {
            capture: value.capture.map(Into::into),
            other: value.other,
            request: value.request,
            status: value.status.into(),
            quote: value.quote.map(Into::into),
        }
    }
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Unavailable custody, refused quote or unresolved work. No charge is authorized by quoting.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values crossing the language boundary.
    pub fn quote_request(
        &self,
        other: String,
        class: String,
        alias: String,
    ) -> Result<RequestProgress, ClientError> {
        let class = cs_mail_primitives::RequestClassId(
            class.parse().map_err(|_| ClientError::InvalidData)?,
        );
        if class.0 == 0 {
            return Err(ClientError::InvalidData);
        }
        self.0
            .quote_request(
                &other,
                class,
                cs_mail_primitives::AliasId::new(
                    alias.parse().map_err(|_| ClientError::InvalidData)?,
                )
                .map_err(|_| ClientError::InvalidData)?,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Expired/changed reviewed terms or a refused/unknown command outcome.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values crossing the language boundary.
    pub fn authorize_request(
        &self,
        request: String,
        text: String,
    ) -> Result<RequestProgress, ClientError> {
        self.0
            .authorize_request(
                cs_mail_primitives::RequestId(
                    request.parse().map_err(|_| ClientError::InvalidData)?,
                ),
                &text,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Custody or encrypted device state is unavailable.
    pub fn request_progress(&self) -> Result<Option<RequestProgress>, ClientError> {
        self.0
            .request_progress()
            .map(|value| value.map(Into::into))
            .map_err(Into::into)
    }
    /// # Errors
    /// Original submission remains uncertain, cancellation is refused or custody is unavailable.
    pub fn cancel_request(&self) -> Result<(), ClientError> {
        self.0.cancel_request().map_err(Into::into)
    }
    /// # Errors
    /// Unresolved different work, unavailable original conversation or a refused/unknown admission.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns values crossing the language boundary.
    pub fn send_followup(
        &self,
        other: String,
        request: String,
        text: String,
    ) -> Result<RequestProgress, ClientError> {
        self.0
            .send_followup(
                &other,
                cs_mail_primitives::RequestId(
                    request.parse().map_err(|_| ClientError::InvalidData)?,
                ),
                &text,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ContactState {
    Absent,
    Closed,
    Standing,
    Lane,
    Blocked,
    Revoked,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReferenceContent {
    Readable { text: String },
    Unavailable,
    AccessDenied,
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Source unavailable, invalid document or unavailable custody.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns foreign strings.
    pub fn source_message(&self, message: String) -> Result<SourceMessage, ClientError> {
        let message =
            cs_mail_primitives::MessageId(message.parse().map_err(|_| ClientError::InvalidData)?);
        self.0
            .source_message(message)
            .map(|source| SourceMessage {
                message: source.message.0.to_string(),
                version: source.version.0.to_vec(),
                blocks: source
                    .blocks
                    .into_iter()
                    .map(|block| match block {
                        core::SourceBlock::Text { block, text } => {
                            SourceBlock::Text { block, text }
                        }
                        core::SourceBlock::Image { block, bytes } => {
                            SourceBlock::Image { block, bytes }
                        }
                        core::SourceBlock::Reference { block } => SourceBlock::Reference { block },
                    })
                    .collect(),
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// Unavailable custody/authority, malformed reference or uncertain service response.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns foreign strings.
    pub fn resolve_reference(
        &self,
        message: String,
        part: u32,
    ) -> Result<ReferenceContent, ClientError> {
        let message =
            cs_mail_primitives::MessageId(message.parse().map_err(|_| ClientError::InvalidData)?);
        self.0
            .resolve_reference(message, part)
            .map(|value| match value {
                core::ReferenceContent::Readable(text) => ReferenceContent::Readable { text },
                core::ReferenceContent::Unavailable => ReferenceContent::Unavailable,
                core::ReferenceContent::AccessDenied => ReferenceContent::AccessDenied,
            })
            .map_err(Into::into)
    }
}

#[derive(uniffi::Record)]
pub struct ExternalInbox {
    pub pending: Option<String>,
    pub threads: Vec<ExternalThread>,
}
#[derive(uniffi::Record)]
pub struct ExternalThread {
    pub id: String,
    pub mailbox: String,
    pub guest: String,
    pub permission: String,
    pub permission_route: PermissionRoute,
    pub pending_invitation: bool,
    pub disclosure: bool,
    pub blocked: bool,
    pub messages: Vec<ExternalMessage>,
}
#[derive(uniffi::Record)]
pub struct ExternalMessage {
    pub id: String,
    pub text: String,
    pub outgoing: bool,
    pub committed_at: u64,
    pub sequence: u64,
    pub delivery: HandoffState,
    pub provider_events: Vec<ProviderObservation>,
    pub content: ReadingContent,
}
#[derive(uniffi::Enum)]
pub enum ExternalAction {
    Reject,
    WithdrawDisclosure,
    GrantDisclosure,
}
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Unavailable custody or invalid encrypted content.
    pub fn external_inbox(&self) -> Result<ExternalInbox, ClientError> {
        self.0
            .external_inbox()
            .map(|v| ExternalInbox {
                pending: v.pending,
                threads: v
                    .threads
                    .into_iter()
                    .map(|t| ExternalThread {
                        id: t.id,
                        mailbox: t.mailbox,
                        guest: t.guest,
                        permission: t.permission,
                        permission_route: t.permission_route.into(),
                        pending_invitation: t.pending_invitation,
                        disclosure: t.disclosure,
                        blocked: t.blocked,
                        messages: t
                            .messages
                            .into_iter()
                            .map(|m| ExternalMessage {
                                id: m.id,
                                text: m.text,
                                outgoing: m.outgoing,
                                committed_at: m.committed_at,
                                sequence: m.sequence,
                                delivery: m.delivery.into(),
                                provider_events: m
                                    .provider_events
                                    .into_iter()
                                    .map(Into::into)
                                    .collect(),
                                content: m.content.into(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid inputs, unresolved work, unavailable service or refused command.
    #[allow(clippy::needless_pass_by_value)] // UniFFI requires owned values at the language boundary.
    pub fn send_external(
        &self,
        conversation: Option<String>,
        mailbox: String,
        text: String,
        disclose: bool,
    ) -> Result<String, ClientError> {
        self.0
            .send_external(conversation.as_deref(), &mailbox, &text, disclose)
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid conversation, unresolved work or unknown/refused outcome.
    #[allow(clippy::needless_pass_by_value)] // UniFFI requires owned values at the language boundary.
    pub fn control_external(
        &self,
        conversation: String,
        action: ExternalAction,
    ) -> Result<(), ClientError> {
        self.0
            .control_external(
                &conversation,
                match action {
                    ExternalAction::Reject => core::ExternalAction::Reject,
                    ExternalAction::WithdrawDisclosure => core::ExternalAction::WithdrawDisclosure,
                    ExternalAction::GrantDisclosure => core::ExternalAction::GrantDisclosure,
                },
            )
            .map_err(Into::into)
    }
}
