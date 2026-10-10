//! Generated bindings for the portable member client; no domain rules live here.
use cs_mail_client::member as core;
use std::sync::Arc;
uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
pub enum ClientError {
    NotConfigured,
    AlreadyConfigured,
    KeyUnavailable,
    StorageUnavailable,
    InvalidData,
    Offline,
    Unauthorized,
    Unavailable,
    DecisionPending,
    NoPendingRequest,
    DecisionRefused,
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
            core::Error::NotConfigured => Self::NotConfigured,
            core::Error::AlreadyConfigured => Self::AlreadyConfigured,
            core::Error::KeyUnavailable => Self::KeyUnavailable,
            core::Error::StorageUnavailable => Self::StorageUnavailable,
            core::Error::InvalidData => Self::InvalidData,
            core::Error::Offline => Self::Offline,
            core::Error::Unauthorized => Self::Unauthorized,
            core::Error::Unavailable => Self::Unavailable,
            core::Error::DecisionPending => Self::DecisionPending,
            core::Error::NoPendingRequest => Self::NoPendingRequest,
            core::Error::DecisionRefused => Self::DecisionRefused,
            core::Error::EnrollmentRequired => Self::EnrollmentRequired,
            core::Error::LoginUnavailable => Self::LoginUnavailable,
            core::Error::LoginCancelled => Self::LoginCancelled,
        }
    }
}

#[uniffi::export(callback_interface)]
pub trait AppleStore: Send + Sync {
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
}
struct Store(Box<dyn AppleStore>);
impl core::DeviceStore for Store {
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
    pub account: String,
    pub persona: String,
    pub synchronized_at: Option<u64>,
    pub conversations: Vec<Conversation>,
    pub pending_sender: Option<String>,
    pub last_decision: Option<String>,
}
#[derive(uniffi::Record)]
pub struct Conversation {
    pub sender: String,
    pub title: String,
    pub permission: String,
    pub external: bool,
    pub requests: Vec<Request>,
    pub messages: Vec<Message>,
}
#[derive(uniffi::Record)]
pub struct Request {
    pub id: String,
    pub class_name: String,
    pub status: String,
    pub deadline: Option<u64>,
    pub awaiting_decision: bool,
    pub financial_status: String,
}
#[derive(uniffi::Record)]
pub struct Message {
    pub id: String,
    pub text: String,
}
#[derive(Clone, Copy, uniffi::Enum)]
pub enum Decision {
    Accept,
    Reject,
}
impl From<core::Inbox> for Inbox {
    fn from(value: core::Inbox) -> Self {
        Self {
            account: value.account,
            persona: value.persona,
            synchronized_at: value.synchronized_at,
            conversations: value.conversations.into_iter().map(Into::into).collect(),
            pending_sender: value.pending_sender,
            last_decision: value.last_decision,
        }
    }
}
impl From<core::Conversation> for Conversation {
    fn from(value: core::Conversation) -> Self {
        Self {
            sender: value.sender,
            title: value.title,
            permission: value.permission,
            external: value.external,
            requests: value
                .requests
                .into_iter()
                .map(|r| Request {
                    id: r.id,
                    class_name: r.class_name,
                    status: r.status,
                    deadline: r.deadline,
                    awaiting_decision: r.awaiting_decision,
                    financial_status: r.financial_status,
                })
                .collect(),
            messages: value
                .messages
                .into_iter()
                .map(|m| Message {
                    id: m.id,
                    text: m.text,
                })
                .collect(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct AppleClient(core::MemberClient);
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
    /// Authority, binding or decryption failed.
    pub fn conversation(&self, sender: String) -> Result<Conversation, ClientError> {
        self.0
            .conversation(&sender)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// An operation is pending, authority failed or the outcome is unknown/refused.
    pub fn decide(&self, sender: String, decision: Decision) -> Result<Inbox, ClientError> {
        self.0
            .decide(
                &sender,
                match decision {
                    Decision::Accept => core::Decision::Accept,
                    Decision::Reject => core::Decision::Reject,
                },
            )
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// The original operation still has no authenticated outcome.
    pub fn retry(&self) -> Result<Inbox, ClientError> {
        self.0.retry().map(Into::into).map_err(Into::into)
    }
}
