use super::{
    AppleClient, ClientError, ReuseMode, SourceDraft, SourceElement, core, source_selection,
};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{ProtocolIdentity, RequestClassId, RequestId};

#[derive(Clone, uniffi::Enum)]
pub enum DraftTarget {
    Native {
        other: String,
        conversation: Option<String>,
    },
    External {
        conversation: Option<String>,
    },
    Request {
        other: String,
        class: String,
    },
    Followup {
        other: String,
        request: String,
    },
}
fn number(value: &str) -> Result<u128, ClientError> {
    value
        .parse::<u128>()
        .ok()
        .filter(|v| *v != 0)
        .ok_or(ClientError::InvalidData)
}
fn conversation(value: Option<String>) -> Result<Option<ConversationId>, ClientError> {
    value
        .map(|v| ConversationId::new(number(&v)?).map_err(|_| ClientError::InvalidData))
        .transpose()
}
impl TryFrom<DraftTarget> for core::DraftTarget {
    type Error = ClientError;
    fn try_from(value: DraftTarget) -> Result<Self, ClientError> {
        Ok(match value {
            DraftTarget::Native {
                other,
                conversation: id,
            } => Self::Native {
                other: ProtocolIdentity(number(&other)?),
                conversation: conversation(id)?,
            },
            DraftTarget::External { conversation: id } => Self::External {
                conversation: conversation(id)?,
            },
            DraftTarget::Request { other, class } => Self::Request {
                other: ProtocolIdentity(number(&other)?),
                class: RequestClassId(number(&class)?),
            },
            DraftTarget::Followup { other, request } => Self::Followup {
                other: ProtocolIdentity(number(&other)?),
                request: RequestId(number(&request)?),
            },
        })
    }
}
impl From<&core::DraftTarget> for DraftTarget {
    fn from(value: &core::DraftTarget) -> Self {
        match value {
            core::DraftTarget::Native {
                other,
                conversation,
            } => Self::Native {
                other: other.0.to_string(),
                conversation: conversation.map(|c| c.value().to_string()),
            },
            core::DraftTarget::External { conversation } => Self::External {
                conversation: conversation.map(|c| c.value().to_string()),
            },
            core::DraftTarget::Request { other, class } => Self::Request {
                other: other.0.to_string(),
                class: class.0.to_string(),
            },
            core::DraftTarget::Followup { other, request } => Self::Followup {
                other: other.0.to_string(),
                request: request.0.to_string(),
            },
        }
    }
}
#[derive(Clone, uniffi::Enum)]
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
impl TryFrom<DraftBody> for core::DraftBody {
    type Error = ClientError;
    fn try_from(value: DraftBody) -> Result<Self, ClientError> {
        Ok(match value {
            DraftBody::Native { text, sources } => Self::Native {
                text,
                sources: sources
                    .into_iter()
                    .map(|s| {
                        Ok(core::SourceDraft {
                            selection: source_selection(
                                cs_mail_primitives::MessageId(number(&s.message)?),
                                s.version,
                                s.elements,
                            )?,
                            mode: match s.mode {
                                ReuseMode::Quotation => core::ReuseMode::Quotation,
                                ReuseMode::Sharing => core::ReuseMode::Sharing,
                                ReuseMode::Reference => core::ReuseMode::Reference,
                            },
                        })
                    })
                    .collect::<Result<_, ClientError>>()?,
            },
            DraftBody::External { mailbox, text } => Self::External { mailbox, text },
            DraftBody::Request { text } => Self::Request { text },
        })
    }
}
impl From<&core::DraftBody> for DraftBody {
    fn from(value: &core::DraftBody) -> Self {
        match value {
            core::DraftBody::Native { text, sources } => Self::Native {
                text: text.clone(),
                sources: sources
                    .iter()
                    .map(|s| SourceDraft {
                        message: s.selection.message().0.to_string(),
                        version: s.selection.version().0.to_vec(),
                        elements: s
                            .selection
                            .elements()
                            .iter()
                            .map(|e| match *e {
                                cs_mail_correspondence::SelectionElement::TextRange {
                                    block,
                                    start,
                                    end,
                                } => SourceElement::TextRange { block, start, end },
                                cs_mail_correspondence::SelectionElement::ImageBlock { block } => {
                                    SourceElement::ImageBlock { block }
                                }
                            })
                            .collect(),
                        mode: match s.mode {
                            core::ReuseMode::Quotation => ReuseMode::Quotation,
                            core::ReuseMode::Sharing => ReuseMode::Sharing,
                            core::ReuseMode::Reference => ReuseMode::Reference,
                        },
                    })
                    .collect(),
            },
            core::DraftBody::External { mailbox, text } => Self::External {
                mailbox: mailbox.clone(),
                text: text.clone(),
            },
            core::DraftBody::Request { text } => Self::Request { text: text.clone() },
        }
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum DraftIssue {
    SubjectTooLong { max_bytes: u64 },
    EmptyMessage,
    TextTooLong { max_bytes: u64 },
    InvalidMailbox,
}
#[derive(Clone, uniffi::Record)]
pub struct Draft {
    pub subject: String,
    pub id: String,
    pub revision: u64,
    pub target: DraftTarget,
    pub body: DraftBody,
    pub updated_at: u64,
    pub issues: Vec<DraftIssue>,
}
impl From<core::Draft> for Draft {
    fn from(d: core::Draft) -> Self {
        Self {
            subject: d.subject().to_owned(),
            id: d.id().value().to_string(),
            revision: d.revision().value(),
            target: d.target().into(),
            body: d.body().into(),
            updated_at: d.updated_at().0,
            issues: d
                .issues()
                .into_iter()
                .map(|issue| match issue {
                    core::DraftIssue::SubjectTooLong { max_bytes } => {
                        DraftIssue::SubjectTooLong { max_bytes }
                    }
                    core::DraftIssue::EmptyMessage => DraftIssue::EmptyMessage,
                    core::DraftIssue::TextTooLong { max_bytes } => {
                        DraftIssue::TextTooLong { max_bytes }
                    }
                    core::DraftIssue::InvalidMailbox => DraftIssue::InvalidMailbox,
                })
                .collect(),
        }
    }
}
#[derive(uniffi::Enum)]
pub enum DraftSubmission {
    Native { message: String },
    External { conversation: String },
    Request { request: String },
}
impl From<core::DraftSubmission> for DraftSubmission {
    fn from(value: core::DraftSubmission) -> Self {
        match value {
            core::DraftSubmission::Native { message } => Self::Native {
                message: message.0.to_string(),
            },
            core::DraftSubmission::External { conversation } => Self::External {
                conversation: conversation.value().to_string(),
            },
            core::DraftSubmission::Request { request } => Self::Request {
                request: request.0.to_string(),
            },
        }
    }
}
#[allow(clippy::needless_pass_by_value)] // UniFFI requires owned arguments at the language boundary.
#[uniffi::export]
impl AppleClient {
    /// # Errors
    /// Unavailable custody or invalid encrypted workspace.
    pub fn drafts(&self) -> Result<Vec<Draft>, ClientError> {
        self.0
            .drafts()
            .map(|v| v.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid target, unavailable custody, pending submission, or full workspace.
    pub fn open_draft(&self, target: DraftTarget) -> Result<Draft, ClientError> {
        self.0
            .open_draft(target.try_into()?)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Invalid target, unavailable custody, or full workspace.
    pub fn create_draft(&self, target: DraftTarget) -> Result<Draft, ClientError> {
        self.0
            .create_draft(target.try_into()?)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Changed revision, invalid body, or failed durable replacement.
    pub fn save_draft(
        &self,
        id: String,
        revision: u64,
        body: DraftBody,
        subject: String,
        local_offset_seconds: i32,
    ) -> Result<Draft, ClientError> {
        self.0
            .save_draft(
                number(&id)?.try_into()?,
                revision.try_into()?,
                body.try_into()?,
                &subject,
                local_offset_seconds,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
    /// # Errors
    /// Changed revision or failed durable replacement.
    pub fn discard_draft(&self, id: String, revision: u64) -> Result<(), ClientError> {
        self.0
            .discard_draft(number(&id)?.try_into()?, revision.try_into()?)
            .map_err(Into::into)
    }
    /// # Errors
    /// Changed revision, invalid inputs, refused authority, or uncertain submission.
    pub fn submit_draft(
        &self,
        id: String,
        revision: u64,
        disclose: bool,
        reviewed_request: Option<String>,
    ) -> Result<DraftSubmission, ClientError> {
        self.0
            .submit_draft(
                number(&id)?.try_into()?,
                revision.try_into()?,
                disclose,
                reviewed_request
                    .map(|v| number(&v).map(RequestId))
                    .transpose()?,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
}
