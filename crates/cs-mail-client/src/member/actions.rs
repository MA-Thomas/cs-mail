//! Purpose-specific presentation over existing permission projections; never send authority.
use super::{Conversation, Error, ReplyRoute, RequestEntry};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{CanonicalTime, ContentKeyRef, RequestId};

#[derive(Clone, Copy, Debug)]
pub enum InitiationAction {
    ComposeStanding,
    ComposeWithinLane,
    ReviewApproaches,
    ReviewIncoming(RequestId),
    AwaitRequest(RequestId),
    RefreshRequired,
    Unavailable,
}
#[derive(Clone, Copy, Debug)]
pub enum ContactObservation {
    Current,
    Saved,
}
#[derive(Clone, Debug)]
pub struct ContactActions {
    pub observation: ContactObservation,
    pub observed_at: Option<CanonicalTime>,
    pub initiation: InitiationAction,
    pub reply: ReplyRoute,
    /// Permission to reply here does not establish permission to start another conversation.
    pub reply_conversations: Vec<ConversationId>,
}
impl Conversation {
    /// Derives actions from the same Rust projections already used to explain permission.
    /// # Errors
    /// A malformed conversation identity cannot become a reply destination.
    pub fn actions(&self) -> Result<ContactActions, Error> {
        let initiation = match self.request_entry {
            RequestEntry::IncomingPending { request } => InitiationAction::ReviewIncoming(request),
            RequestEntry::OutgoingPending { request } => InitiationAction::AwaitRequest(request),
            RequestEntry::Unverified => InitiationAction::RefreshRequired,
            RequestEntry::OwnAddress | RequestEntry::Unavailable => InitiationAction::Unavailable,
            RequestEntry::ReviewApproaches | RequestEntry::AlreadyPermitted => match self.reply {
                ReplyRoute::Standing => InitiationAction::ComposeStanding,
                ReplyRoute::Lane => InitiationAction::ComposeWithinLane,
                ReplyRoute::Conversation | ReplyRoute::Closed => match self.request_entry {
                    RequestEntry::ReviewApproaches => InitiationAction::ReviewApproaches,
                    _ => InitiationAction::RefreshRequired,
                },
            },
        };
        let reply_conversations = self
            .threads
            .iter()
            .filter(|thread| {
                matches!(self.reply, ReplyRoute::Standing | ReplyRoute::Lane)
                    || thread.reply_allowed
            })
            .map(|thread| {
                ConversationId::new(thread.id.parse().map_err(|_| Error::InvalidData)?)
                    .map_err(|_| Error::InvalidData)
            })
            .collect::<Result<_, _>>()?;
        Ok(ContactActions {
            observation: if matches!(self.request_entry, RequestEntry::Unverified) {
                ContactObservation::Saved
            } else {
                ContactObservation::Current
            },
            observed_at: self.observed_at,
            initiation,
            reply: self.reply,
            reply_conversations,
        })
    }
}
/// Published recovery configuration; it is neither a decryption grant nor account recovery.
#[derive(Clone, Copy, Debug)]
pub enum RecoveryCustody {
    CsqdHostManaged { key: ContentKeyRef },
}
impl super::MemberClient {
    /// Inspect the configured trust boundary without disclosing or releasing any copy.
    /// # Errors
    /// Local custody is unavailable or stored configuration cannot be authenticated.
    pub fn recovery_custody(&self) -> Result<RecoveryCustody, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let state = super::storage::open(self.store.as_ref(), &keys)?;
        Ok(RecoveryCustody::CsqdHostManaged {
            key: state.profile.deployment.custody_key.reference,
        })
    }
}
