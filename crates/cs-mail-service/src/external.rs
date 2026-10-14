//! External correspondence host boundary. The transport receives plaintext only
//! after committed dispatch authority. No public API accepts caller-asserted SMTP evidence.
use crate::members::MemberGateway;
use crate::{CanonicalClock, ServiceError};
use cs_mail_application::correspondence::external::{Dispatch, Response, Service, SignedCommand};
use cs_mail_content::{EndpointPublicKey, EndpointSecretKey};
use cs_mail_correspondence::external::delivery::ProviderReference;
use cs_mail_correspondence::{CorrespondenceError, Document, DocumentPart};
use cs_mail_primitives::{ContentKeyRef, MessageId};
use cs_mail_storage_postgres::{StorageError, external::Repository};
use std::sync::Arc;

const EXTERNAL_KEY: ContentKeyRef = ContentKeyRef(0x6578_7465_726e_616c);
pub struct ExternalGateway<C> {
    members: Arc<MemberGateway<C>>,
    content_seed: [u8; 32],
}
impl<C: CanonicalClock + Clone + 'static> ExternalGateway<C> {
    pub fn new(members: Arc<MemberGateway<C>>, content_seed: [u8; 32]) -> Self {
        Self {
            members,
            content_seed,
        }
    }
    /// # Errors
    /// Invalid host key material.
    pub fn external_key(&self) -> Result<(EndpointSecretKey, EndpointPublicKey), ServiceError> {
        EndpointSecretKey::from_secret_bytes(EXTERNAL_KEY, &self.content_seed)
            .map_err(|_| ServiceError::RecipientNotReady)
    }
    /// The same member and guest registry, with a distinct external delivery route.
    /// # Errors
    /// Invalid host key material.
    pub fn external_repository(&self) -> Result<Repository, ServiceError> {
        let clock = self.members.clock().clone();
        Ok(Repository {
            accounts: self.members.deployment().accounts(move || clock.now()),
            deployment: self.members.record().clone(),
            deriver: self.members.deriver(),
            relay_key: self.external_key()?.1,
        })
    }
    /// # Errors
    /// Signed command authorization, domain or persistence failures.
    pub fn external_command(&self, request: &SignedCommand) -> Result<Response, ServiceError> {
        let response = Service(&self.external_repository()?).handle(request)?;
        Ok(match response {
            Response::Outcome(acknowledgement) => Response::Outcome(
                self.members
                    .sign_external_acknowledgement(acknowledgement)?,
            ),
            conversation @ Response::Conversation { .. } => conversation,
        })
    }
    /// Dispatches a single queued message. Transport I/O occurs after commit.
    /// # Errors
    /// Refuses expired authority, malformed encrypted text and repeated dispatch.
    pub fn dispatch_external(
        &self,
        id: MessageId,
        transport: &mut impl Transport,
    ) -> Result<(), ServiceError> {
        let repository = self.external_repository()?;
        dispatch(&repository, &self.external_key()?.0, id, transport)
    }
}
/// # Errors
/// Dispatch is authorized once at commit, with no transport operation under database locks.
pub fn dispatch(
    repository: &Repository,
    secret: &EndpointSecretKey,
    id: MessageId,
    transport: &mut impl Transport,
) -> Result<(), ServiceError> {
    let Some(dispatch) = repository.begin_dispatch(id)? else {
        return Ok(());
    };
    let document = match open_dispatch(&dispatch, secret) {
        Ok(document) => document,
        Err(error) => {
            repository.finish_dispatch(id, None)?;
            return Err(StorageError::from(error).into());
        }
    };
    match transport.send(&dispatch, &document) {
        Handoff::Accepted(reference) => repository.finish_dispatch(id, Some(reference))?,
        Handoff::Failed => repository.finish_dispatch(id, None)?,
        Handoff::Unknown => (),
    }
    Ok(())
}

pub enum Handoff {
    Accepted(ProviderReference),
    Failed,
    Unknown,
}
pub trait Transport {
    /// This is an external plaintext disclosure. Implementations must preserve
    /// stable message/conversation identifiers and distinguish uncertainty.
    fn send(&mut self, dispatch: &Dispatch, document: &Document) -> Handoff;
}
/// # Errors
/// Content must match the committed manifest and contain only authored text.
pub fn open_dispatch(
    dispatch: &Dispatch,
    secret: &EndpointSecretKey,
) -> Result<Document, CorrespondenceError> {
    let message = dispatch.message();
    let bytes = cs_mail_content::decrypt(secret, message.sender_key, dispatch.relay_copy())
        .map_err(|_| CorrespondenceError::Invalid)?;
    let document: Document =
        serde_json::from_slice(&bytes).map_err(|_| CorrespondenceError::Invalid)?;
    if document.manifest(message.manifest.message(), message.manifest.conversation())?
        != message.manifest
        || document
            .parts()
            .iter()
            .any(|p| !matches!(p, DocumentPart::Text(_)))
    {
        return Err(CorrespondenceError::Invalid);
    }
    Ok(document)
}

/// Real carrier transport; construction occurs outside async runtimes.
pub struct Smtp2goTransport {
    pub carrier: cs_mail_smtp_gateway::smtp2go::Smtp2go,
    pub dkim: cs_mail_smtp_gateway::outbound::DkimIdentity,
    pub domain: cs_mail_adapters::DomainIdentity,
}
impl Transport for Smtp2goTransport {
    fn send(&mut self, dispatch: &Dispatch, document: &Document) -> Handoff {
        use cs_mail_smtp_gateway::{
            outbound::{MailThread, OutboundMail, render_signed},
            smtp2go::{Submission, member_local, message_id},
        };
        let id = dispatch.message().manifest.message();
        let root = match dispatch.thread_root() {
            cs_mail_application::correspondence::external::ThreadRoot::Member(root) => {
                (*root != id).then(|| message_id(*root, &self.domain))
            }
            cs_mail_application::correspondence::external::ThreadRoot::Incoming(root) => {
                Some(format!("<{root}>"))
            }
        };
        let body = document
            .parts()
            .iter()
            .filter_map(|part| match part {
                DocumentPart::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mail = OutboundMail {
            from_local: member_local(dispatch.conversation().member()),
            to: dispatch.conversation().mailbox().address(),
            subject: "cs-mail conversation".into(),
            body,
            message_id: format!("cs-{:032x}", id.0),
            date: crate::guests::rfc5322_date(dispatch.message().committed_at),
            thread: Some(MailThread {
                reply_to: cs_mail_smtp_gateway::incoming::reply_address(
                    dispatch.conversation().id(),
                    &self.domain,
                ),
                root,
            }),
        };
        if self.dkim.domain() != self.domain.as_str() {
            return Handoff::Failed;
        }
        let Ok(mime) = render_signed(&mail, &self.dkim) else {
            return Handoff::Failed;
        };
        match self.carrier.submit(&mime) {
            Submission::Accepted(reference) => Handoff::Accepted(reference),
            Submission::Rejected => Handoff::Failed,
            Submission::Unknown => Handoff::Unknown,
        }
    }
}
