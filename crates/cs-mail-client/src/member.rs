//! Portable member application slice. Server rules stay in their existing owners.
//! All mutable device progress is serialized and committed before external effects.
mod enrollment;
mod login;
mod storage;
mod transport;
mod views;
pub use enrollment::{EnrollmentDetails, EnrollmentStatus, LoginStart};
pub use login::Authorization;
pub use storage::DeviceStore;
pub use transport::{HttpTransport, Transport};
pub use views::{Conversation, Inbox, Message, Request};

use cs_mail_application::members::{MemberAnswer, RelationshipView};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, IdempotencyKey, OperationalKeyRef, ProtocolIdentity,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot, ProtocolCommand};
use cs_mail_security::{
    CommandSigner, MemberRead, RecipientSigningScope, SignedCommandBytes, SignedMemberRead,
    SignedReceipt, SigningScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const MAX_CACHE: usize = 64 * 1024 * 1024;

/// Stable, redacted errors. Never include server bodies, credentials, or message text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
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
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Decision {
    Accept,
    Reject,
}

#[derive(Clone, Deserialize, Serialize)]
struct Deployment {
    scope: RecipientSigningScope,
    policy: PolicySnapshot,
    provider_key: [u8; 32],
    product: String,
    browser_login: Option<login::LoginConfig>,
}

#[derive(Deserialize, Serialize)]
struct Profile {
    server: String,
    persona: ProtocolIdentity,
    account: cs_mail_primitives::AccountId,
    sender_key: OperationalKeyRef,
    recipient_key: OperationalKeyRef,
    content_key: ContentKeyRef,
    deployment: Deployment,
}
#[derive(Deserialize, Serialize)]
struct Pending {
    sender: ProtocolIdentity,
    decision: Decision,
    route: String,
    command: SignedCommandBytes,
    relationship: cs_mail_primitives::RelationshipRef,
}
#[derive(Deserialize, Serialize)]
struct State {
    format: u16,
    profile: Profile,
    synchronized_at: Option<CanonicalTime>,
    inbox: Vec<RelationshipView>,
    pending: Option<Pending>,
    content:
        std::collections::BTreeMap<String, Vec<cs_mail_application::members::DeliveredContent>>,
    /// Last acknowledged outcome; displayed only as evidence, never current permission.
    last_decision: Option<String>,
    setup: Option<enrollment::Setup>,
}

/// Synchronous portable use cases. Hosts must invoke them off their UI thread.
/// Each call obtains current key access; no secret material is retained in this object.
pub struct MemberClient {
    store: Arc<dyn DeviceStore>,
    transport: Arc<dyn Transport>,
    clock: Arc<dyn Fn() -> CanonicalTime + Send + Sync>,
    operation: Mutex<()>,
    login: Mutex<Option<login::Attempt>>,
}
impl MemberClient {
    pub fn new(
        store: Arc<dyn DeviceStore>,
        transport: Arc<dyn Transport>,
        clock: Arc<dyn Fn() -> CanonicalTime + Send + Sync>,
    ) -> Self {
        Self {
            store,
            transport,
            clock,
            operation: Mutex::new(()),
            login: Mutex::new(None),
        }
    }
    fn call<T: serde::de::DeserializeOwned>(
        &self,
        server: &str,
        route: &str,
        body: Option<&[u8]>,
    ) -> Result<T, Error> {
        let bytes = self.transport.request(server, route, body)?;
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidData)
    }
    fn read(
        &self,
        profile: &Profile,
        keys: &storage::Secrets,
        read: MemberRead,
    ) -> Result<MemberAnswer, Error> {
        let signed = SignedMemberRead::sign(
            profile.deployment.scope,
            profile.persona,
            profile.sender_key,
            (self.clock)(),
            read,
            &keys.device,
        )
        .map_err(|_| Error::InvalidData)?;
        self.call(
            &profile.server,
            "v1/members/reads",
            Some(&serde_json::to_vec(&signed).map_err(|_| Error::InvalidData)?),
        )
    }
    fn check_account(&self, profile: &Profile, keys: &storage::Secrets) -> Result<(), Error> {
        let MemberAnswer::Account(account) = self.read(profile, keys, MemberRead::Account)? else {
            return Err(Error::InvalidData);
        };
        if account.account != profile.account
            || !account.personas.contains(&profile.persona)
            || !account.keys.iter().any(|key| {
                key.reference == profile.recipient_key
                    && key.active
                    && key.actor == ActorRef::Recipient(profile.persona)
            })
        {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
    /// Opens locally stored state as a timestamped snapshot, not current server authority.
    /// # Errors
    /// Key access is denied or the local ciphertext cannot be authenticated.
    pub fn cached_inbox(&self) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        views::inbox(&storage::open(self.store.as_ref(), &keys)?)
    }
    fn synchronize(&self, state: &mut State, keys: &storage::Secrets) -> Result<(), Error> {
        let deployment: Deployment = self.call(&state.profile.server, "v1/deployment", None)?;
        if deployment.scope != state.profile.deployment.scope
            || deployment.provider_key != state.profile.deployment.provider_key
        {
            return Err(Error::Unauthorized);
        }
        self.check_account(&state.profile, keys)?;
        let MemberAnswer::Inbox(inbox) = self.read(&state.profile, keys, MemberRead::Inbox)? else {
            return Err(Error::InvalidData);
        };
        if inbox
            .iter()
            .any(|v| v.handle.recipient != state.profile.persona)
        {
            return Err(Error::InvalidData);
        }
        state.profile.deployment = deployment;
        state.inbox = inbox;
        state.synchronized_at = Some((self.clock)());
        storage::save(self.store.as_ref(), keys, state)
    }
    /// # Errors
    /// Offline, expired/revoked authority, or failed local durable replacement.
    pub fn sync(&self) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        self.finish_device_setup(&mut state, &keys)?;
        self.synchronize(&mut state, &keys)?;
        views::inbox(&state)
    }
    /// Fetches admitted ciphertext and decrypts only in memory. Plaintext is never cached.
    /// # Errors
    /// Failed authority, altered content binding, ciphertext authentication, or absent conversation.
    pub fn conversation(&self, sender: &str) -> Result<Conversation, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        let sender = parse_sender(sender)?;
        let MemberAnswer::Receiving {
            handle,
            view: Some(view),
            content,
        } = self.read(&state.profile, &keys, MemberRead::AsRecipient { sender })?
        else {
            return Err(Error::InvalidData);
        };
        if handle != view.handle
            || handle.sender != sender
            || handle.recipient != state.profile.persona
        {
            return Err(Error::InvalidData);
        }
        // A custody denial during the fetch must not release plaintext afterward.
        let current = storage::secrets(self.store.as_ref())?;
        if current.cache != keys.cache {
            return Err(Error::KeyUnavailable);
        }
        state.content.insert(sender.0.to_string(), content.clone());
        storage::save(self.store.as_ref(), &keys, &state)?;
        views::conversation(&state.profile, &keys, &view, &content)
    }
    /// Journals one exact signed decision before transmission. No unresolved operation is replaced.
    /// # Errors
    /// Pending operation, stale authority, no submitted request, or an uncertain/denied outcome.
    pub fn decide(&self, sender: &str, decision: Decision) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.pending.is_some() {
            return Err(Error::DecisionPending);
        }
        self.synchronize(&mut state, &keys)?;
        let sender = parse_sender(sender)?;
        let view = state
            .inbox
            .iter()
            .find(|view| view.handle.sender == sender)
            .ok_or(Error::NoPendingRequest)?;
        if !view
            .requests
            .iter()
            .any(|r| r.lifecycle.is_awaiting_recipient_decision())
        {
            return Err(Error::NoPendingRequest);
        }
        let command = match decision {
            Decision::Accept => ProtocolCommand::AcceptRelationship {
                expected_version: view.version.into(),
            },
            Decision::Reject => ProtocolCommand::RejectRelationship {
                expected_version: view.version.into(),
            },
        };
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        let signed = CommandSigner::from_secret_bytes(
            ActorRef::Recipient(state.profile.persona),
            state.profile.recipient_key,
            &keys.recipient,
        )
        .sign(
            SigningScope {
                deployment_domain: state.profile.deployment.scope.deployment_domain,
                intended_provider: state.profile.deployment.scope.provider,
                relationship: view.handle.relationship,
            },
            state.profile.deployment.policy.protocol_version,
            IdempotencyKey(u128::from_be_bytes(random) | 1),
            command,
        )
        .map_err(|_| Error::InvalidData)?;
        state.pending = Some(Pending {
            sender,
            decision,
            route: format!(
                "v1/relationships/{}/{}/commands",
                sender.0, state.profile.persona.0
            ),
            command: signed,
            relationship: view.handle.relationship,
        });
        state.last_decision = None;
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.send_pending(&mut state, &keys)?;
        self.synchronize(&mut state, &keys)?;
        views::inbox(&state)
    }
    /// Replays the existing operation bytes. Rejected transport responses cannot clear uncertainty.
    /// # Errors
    /// Failed transport/authority, invalid receipt, or a durable domain refusal.
    pub fn retry(&self) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.pending.is_some() {
            self.send_pending(&mut state, &keys)?;
        }
        self.synchronize(&mut state, &keys)?;
        views::inbox(&state)
    }
    fn send_pending(&self, state: &mut State, keys: &storage::Secrets) -> Result<(), Error> {
        #[derive(Deserialize)]
        struct Answer {
            receipt: SignedReceipt,
        }
        let pending = state.pending.as_ref().ok_or(Error::InvalidData)?;
        let answer: Answer = self.call(
            &state.profile.server,
            &pending.route,
            Some(&serde_json::to_vec(&pending.command).map_err(|_| Error::InvalidData)?),
        )?;
        let receipt = &answer.receipt;
        let scope = state.profile.deployment.scope;
        let digest: [u8; 32] = Sha256::digest(&pending.command.payload).into();
        // The signed receipt binds these exact bytes. The canonical state is fetched afterward.
        receipt
            .verify(&state.profile.deployment.provider_key)
            .map_err(|_| Error::InvalidData)?;
        if receipt.payload.command_digest.0 != digest
            || receipt.payload.deployment_domain != scope.deployment_domain
            || receipt.payload.provider != scope.provider
            || receipt.payload.relationship != pending.relationship
            || receipt.payload.protocol_version != state.profile.deployment.policy.protocol_version
        {
            return Err(Error::InvalidData);
        }
        let refused = match receipt.payload.kind {
            cs_mail_security::ReceiptKind::Refused => true,
            cs_mail_security::ReceiptKind::RelationshipDecisionCommitted
            | cs_mail_security::ReceiptKind::NoChange => false,
            _ => return Err(Error::InvalidData),
        };
        // Signed acknowledgement resolves the journal; canonical projections establish current permission.
        state.last_decision = Some(if refused {
            "The service refused the decision.".into()
        } else {
            "Decision recorded; refresh to see current permission and settlement.".into()
        });
        state.pending = None;
        storage::save(self.store.as_ref(), keys, state)?;
        if refused {
            return Err(Error::DecisionRefused);
        }
        Ok(())
    }
}
fn parse_sender(sender: &str) -> Result<ProtocolIdentity, Error> {
    let value = sender.parse::<u128>().map_err(|_| Error::InvalidData)?;
    if value == 0 {
        return Err(Error::InvalidData);
    }
    Ok(ProtocolIdentity(value))
}

#[cfg(test)]
mod tests;
