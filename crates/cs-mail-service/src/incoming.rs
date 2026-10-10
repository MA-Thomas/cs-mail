//! Receiver orchestration: authenticate outside locks, encrypt in memory, then
//! recheck admission and commit. No plaintext, transport secret or raw mail is logged.
use crate::CanonicalClock;
use cs_mail_adapters::{DmarcVerifier, DomainIdentity, SmtpAuthenticationRequest};
use cs_mail_correspondence::CorrespondenceError;
use cs_mail_smtp_gateway::{
    incoming::{Refusal, ReplyEncryption, VerifiedMail, authenticate},
    receiver::{IncomingMail, Received, Receiver},
};
use cs_mail_storage_postgres::{StorageError, external::IncomingRepository};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Semaphore;

// postgres's synchronous Client also blocks during Drop. Its last owner must
// be released outside an async executor, including aborted SMTP sessions.
struct ReceiverRepository(Option<IncomingRepository>);
impl Drop for ReceiverRepository {
    fn drop(&mut self) {
        if let Some(repository) = self.0.take() {
            std::thread::spawn(move || drop(repository));
        }
    }
}
pub struct SmtpReceiver<V, C> {
    repository: Arc<ReceiverRepository>,
    verifier: V,
    domain: DomainIdentity,
    replay_secret: [u8; 32],
    clock: C,
    commits: Arc<Semaphore>,
}
impl<V, C> SmtpReceiver<V, C> {
    /// The dedicated replay secret must survive process restarts; it holds no
    /// member signing or relay-decryption authority.
    /// # Errors
    /// Refuses missing secret or invalid concurrency bounds.
    pub fn new(
        repository: IncomingRepository,
        verifier: V,
        domain: DomainIdentity,
        replay_secret: [u8; 32],
        clock: C,
        concurrent_commits: usize,
    ) -> Result<Self, Refusal> {
        let repository = Arc::new(ReceiverRepository(Some(repository)));
        if replay_secret == [0; 32] || !(1..=64).contains(&concurrent_commits) {
            return Err(Refusal::Temporary);
        }
        Ok(Self {
            repository,
            verifier,
            domain,
            replay_secret,
            clock,
            commits: Arc::new(Semaphore::new(concurrent_commits)),
        })
    }
}
fn refusal(error: &StorageError) -> Refusal {
    match error {
        StorageError::Correspondence(
            CorrespondenceError::Unauthorized
            | CorrespondenceError::Unavailable
            | CorrespondenceError::Conflict
            | CorrespondenceError::Invalid,
        ) => Refusal::Permanent,
        _ => Refusal::Temporary,
    }
}
fn commit(
    repository: &IncomingRepository,
    mail: &VerifiedMail,
    secret: &[u8; 32],
    now: cs_mail_primitives::CanonicalTime,
) -> Result<Received, Refusal> {
    let Some(p) = repository
        .prepare_mail(mail, secret)
        .map_err(|e| refusal(&e))?
    else {
        repository
            .queue_request_link(mail, secret)
            .map_err(|e| refusal(&e))?;
        return Ok(Received::RequestLinkHandled);
    };
    let reply = mail.encrypt_reply(
        ReplyEncryption {
            conversation: p.conversation.id(),
            deployment: repository.scope.deployment_domain,
            provider: repository.scope.provider,
            member: p.relationship.recipient,
            relationship: p.relationship.relationship,
            content_scope: p.relationship.content_scope,
            member_key: p.member_key,
            lane: p.lane,
        },
        secret,
        now,
    )?;
    repository.receive(&reply).map_err(|e| refusal(&e))?;
    Ok(Received::Admitted)
}
impl<V: DmarcVerifier, C: CanonicalClock> Receiver for SmtpReceiver<V, C> {
    fn deliver(
        &self,
        mail: IncomingMail,
    ) -> Pin<Box<dyn Future<Output = Result<Received, Refusal>> + Send + '_>> {
        Box::pin(async move {
            let request = SmtpAuthenticationRequest {
                raw_message: &mail.data,
                remote_ip: mail.remote_ip,
                helo_identity: &mail.helo,
                mail_from: mail.mail_from.as_deref(),
                receiver_hostname: self.domain.as_str(),
                received_at: self.clock.now(),
            };
            let verified = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                authenticate(&self.verifier, request, &mail.recipient, &self.domain),
            )
            .await
            .map_err(|_| Refusal::Temporary)??;
            let permit = self
                .commits
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| Refusal::Temporary)?;
            let repository = self.repository.clone();
            let secret = self.replay_secret;
            let now = self.clock.now();
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                commit(
                    repository.0.as_ref().expect("live receiver"),
                    &verified,
                    &secret,
                    now,
                )
            })
            .await
            .map_err(|_| Refusal::Temporary)?
        })
    }
}
