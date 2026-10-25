//! Portable member application slice. Server rules stay in their existing owners.
//! All mutable device progress is serialized and committed before external effects.
mod alias_views;
mod aliases;
pub use alias_views::{
    AddressChoice, AddressConversation, AddressMutation, AddressPermission, AddressPolicy,
    AddressState, AddressUpdateResult, AddressWorkspace, ManagedAddress, PendingAddressAction,
    PendingAddressUpdate, PermissionMutation, ReceivingLaneView,
};
mod correspondence;
mod drafts;
mod inboxes;
mod personal;
pub use inboxes::{AliasInbox, InboxScope, InboxWorkspace, MessageRoute, SenderLaneScope};
mod reading;
mod resources;
mod retained;
mod retained_index;
pub use drafts::{
    Draft, DraftBody, DraftId, DraftIssue, DraftRevision, DraftSubmission, DraftTarget,
};
pub use personal::{
    MailAction, MailFolder, MailItem, MailKey, OutgoingWork, SearchMatch, SearchPage, WorkStatus,
};
pub use reading::{
    PictureStatus, ReadingBlock, ReadingContent, ReadingEmail, ReadingPart, ReadingPicture,
};
pub use retained::RetentionProgress;
mod directory;
pub use correspondence::{ConversationSummary, ReplyRoute};
pub use directory::ComposerRecipient;
mod external;
pub use external::{ExternalAction, ExternalInbox, ExternalMessage, ExternalThread};
mod enrollment;
mod login;
mod onboarding;
mod relationships;
mod requests;
mod reuse;
pub use cs_mail_correspondence::PolicyAction;
pub use relationships::RelationshipControl;
pub use requests::{RequestProgress, RequestQuote, RequestStatus};
pub use reuse::{
    ConversationPolicy, ReferenceContent, Relaxation, ReuseMode, SourceBlock, SourceDraft,
    SourceMessage,
};
mod navigation;
mod storage;
mod transport;
mod views;
pub use enrollment::{EnrollmentDetails, EnrollmentStatus, LoginStart};
pub use login::Authorization;
pub use navigation::{NavigationContact, NavigationLane, NavigationRequest};
pub use storage::{
    DeviceStore, RetainedCommit, RetainedCopyWrite, RetainedIndexRow, RetainedRecordId,
    RetainedWrite,
};
pub use transport::{HttpTransport, Transport};
pub use views::{
    ContactState, Conversation, Inbox, LaneExpiryReminder, Message, PendingSetup, PermissionRoute,
    RelationshipControls, Request, RequestEntry,
};

use cs_mail_application::members::{MemberAnswer, RelationshipView};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, IdempotencyKey, OperationalKeyRef, ProtocolIdentity, RequestId,
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
    ExpressLane { lifetime_ms: u64 },
}

/// Historical send result, separate from current relationship authority.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SendOutcome {
    Committed(cs_mail_primitives::MessageId),
    Refused {
        operation: IdempotencyKey,
        reason: cs_mail_correspondence::CorrespondenceError,
    },
}

#[derive(Clone, Deserialize, Serialize)]
struct Deployment {
    custody_key: cs_mail_content::EndpointPublicKey,
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
    route: String,
    command: PendingCommand,
    relationship: cs_mail_primitives::RelationshipRef,
}
#[derive(Deserialize, Serialize)]
enum PendingCommand {
    Relationship(SignedCommandBytes),
    Lane {
        grant: Box<cs_mail_capabilities::SignedLaneGrant>,
        idempotency_key: IdempotencyKey,
    },
}
impl PendingCommand {
    fn body(&self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Relationship(signed) => serde_json::to_vec(signed),
            Self::Lane {
                grant,
                idempotency_key,
            } => serde_json::to_vec(
                &serde_json::json!({"grant":grant, "idempotency_key":idempotency_key}),
            ),
        }
        .map_err(|_| Error::InvalidData)
    }
    fn digest(&self) -> Result<[u8; 32], Error> {
        match self {
            Self::Relationship(signed) => Ok(Sha256::digest(&signed.payload).into()),
            Self::Lane { grant, .. } => Ok(Sha256::digest(
                grant
                    .grant
                    .signing_bytes()
                    .map_err(|_| Error::InvalidData)?,
            )
            .into()),
        }
    }
}
#[derive(Deserialize, Serialize)]
struct State {
    external: Option<external::Workspace>,
    requesting: Option<requests::RequestJourney>,
    last_request: Option<requests::RequestResult>,
    editing_conversation: Option<cs_mail_application::correspondence::SignedRequest>,
    last_send: Option<SendOutcome>,
    mailbox: correspondence::Mailbox,
    sending: Option<correspondence::PendingSend>,
    format: u16,
    profile: Profile,
    synchronized_at: Option<CanonicalTime>,
    inbox: Vec<RelationshipView>,
    pending: Option<Pending>,
    content: std::collections::BTreeMap<
        String,
        Vec<cs_mail_application::members::GuestDeliveredContent>,
    >,
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
    workspace: Mutex<()>,
    search: Mutex<Option<personal::SearchSession>>,
    receiving: Mutex<Option<inboxes::InboxCatalog>>,
    retained: Mutex<Option<retained_index::Retained>>,
    retained_epoch: std::sync::atomic::AtomicU64,
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
            workspace: Mutex::new(()),
            search: Mutex::new(None),
            receiving: Mutex::new(None),
            retained: Mutex::new(None),
            retained_epoch: std::sync::atomic::AtomicU64::new(0),
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
    fn account_view(
        &self,
        profile: &Profile,
        keys: &storage::Secrets,
    ) -> Result<cs_mail_application::members::AccountView, Error> {
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
        Ok(account)
    }
    /// Opens locally stored state as a timestamped snapshot, not current server authority.
    /// # Errors
    /// Key access is denied or the local ciphertext cannot be authenticated.
    pub fn cached_inbox(&self) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        views::inbox(&storage::open(self.store.as_ref(), &keys)?)
    }
    fn observe_inbox(&self, state: &mut State, keys: &storage::Secrets) -> Result<(), Error> {
        let deployment: Deployment = self.call(&state.profile.server, "v1/deployment", None)?;
        if deployment.scope != state.profile.deployment.scope
            || deployment.provider_key != state.profile.deployment.provider_key
        {
            return Err(Error::Unauthorized);
        }
        self.account_view(&state.profile, keys)?;
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
        Ok(())
    }
    fn synchronize(&self, state: &mut State, keys: &storage::Secrets) -> Result<(), Error> {
        self.observe_inbox(state, keys)?;
        self.sync_mailbox(state, keys)?;
        self.sync_external(state, keys)?;
        state.synchronized_at = Some((self.clock)());
        storage::save(self.store.as_ref(), keys, state)
    }
    /// # Errors
    /// Offline, expired/revoked authority, or failed local durable replacement.
    pub fn sync(&self) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        self.finish_alias_setup(&mut state, &keys)?;
        match self.finish_device_setup(&mut state, &keys) {
            Ok(()) | Err(Error::SetupRefused) => {}
            Err(error) => return Err(error),
        }
        match self.advance_request(&mut state, &keys) {
            Ok(()) | Err(Error::RequestRefused) => {}
            Err(error) => return Err(error),
        }
        match self.transmit_conversation_edit(&mut state, &keys) {
            Ok(()) | Err(Error::PolicyRefused | Error::CopyDeletionRefused) => {}
            Err(error) => return Err(error),
        }
        if state.sending.is_some() {
            match self.transmit_send(&mut state, &keys) {
                Ok(_) | Err(Error::SendRefused) => {}
                Err(error) => return Err(error),
            }
        }
        self.transmit_external(&mut state, &keys)?;
        self.synchronize(&mut state, &keys)?;
        views::inbox(&state)
    }
    /// Refreshes observations without resuming any mutation or financial operation.
    /// # Errors
    /// Failed authentication, transport or durable snapshot replacement.
    pub fn refresh_mail(&self) -> Result<MailRefresh, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        self.observe_inbox(&mut state, &keys)?;
        let before = state.mailbox.clone();
        let native = self.sync_mailbox(&mut state, &keys);
        if native.is_err() {
            state.mailbox = before;
        }
        let native_current = observation(native)?;
        let external_current = observation(self.sync_external(&mut state, &keys))?;
        if native_current && external_current {
            state.synchronized_at = Some((self.clock)());
        }
        storage::save(self.store.as_ref(), &keys, &state)?;
        Ok(MailRefresh {
            inbox: views::inbox(&state)?,
            native_current,
            external_current,
        })
    }
    /// Opens device-retained native correspondence without network access.
    /// # Errors
    /// Unavailable custody, invalid local evidence or unknown participant.
    pub fn cached_conversation(&self, sender: &str) -> Result<Conversation, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let sender = parse_sender(sender)?;
        let result = self.cached_native_conversation(&state, &keys, sender)?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        Ok(result)
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
            view,
            content,
        } = (match self.read(&state.profile, &keys, MemberRead::AsRecipient { sender }) {
            Ok(value) => value,
            Err(Error::Offline) => return self.cached_native_conversation(&state, &keys, sender),
            Err(error) => return Err(error),
        })
        else {
            return Err(Error::InvalidData);
        };
        if view.as_ref().is_some_and(|view| handle != view.handle)
            || handle.sender != sender
            || handle.recipient != state.profile.persona
        {
            return Err(Error::InvalidData);
        }
        if view.as_ref().is_none_or(|view| view.guest.is_none()) {
            return self.native_conversation(&mut state, &keys, sender);
        }
        let view = view.ok_or(Error::InvalidData)?;
        // A custody denial during the fetch must not release plaintext afterward.
        let current = storage::secrets(self.store.as_ref())?;
        if current.cache != keys.cache {
            return Err(Error::KeyUnavailable);
        }
        state.content.insert(sender.0.to_string(), content.clone());
        storage::save(self.store.as_ref(), &keys, &state)?;
        views::conversation(&state.profile, &keys, &view, &content, (self.clock)())
    }
    /// Journals one exact signed decision before transmission. No unresolved operation is replaced.
    /// # Errors
    /// Pending operation, stale authority, no submitted request, or an uncertain/denied outcome.
    pub fn decide(
        &self,
        sender: &str,
        request: RequestId,
        decision: Decision,
    ) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.requesting.is_some() {
            return Err(Error::RequestPending);
        }
        if state.editing_conversation.is_some() {
            return Err(Error::DecisionPending);
        }
        if state.sending.is_some() {
            return Err(Error::SendPending);
        }
        if state.pending.is_some() {
            return Err(Error::DecisionPending);
        }
        if state.setup.is_some() {
            return Err(Error::SetupPending);
        }
        require_idle(&state)?;
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
            .any(|r| r.id == request && r.lifecycle.is_awaiting_recipient_decision())
        {
            return Err(Error::NoPendingRequest);
        }
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        let operation = IdempotencyKey(u128::from_be_bytes(random) | 1);
        let command = match decision {
            Decision::Accept => ProtocolCommand::AcceptRelationship {
                expected_version: view.version.into(),
            },
            Decision::Reject => ProtocolCommand::RejectRelationship {
                expected_version: view.version.into(),
            },
            Decision::ExpressLane { lifetime_ms } => {
                let grant =
                    lane_grant(&state, &keys, view, lifetime_ms, (self.clock)(), operation)?;
                state.pending = Some(Pending {
                    sender,
                    route: "v1/lanes".into(),
                    command: grant,
                    relationship: view.handle.relationship,
                });
                state.last_decision = None;
                storage::save(self.store.as_ref(), &keys, &state)?;
                self.send_pending(&mut state, &keys)?;
                self.synchronize(&mut state, &keys)?;
                return views::inbox(&state);
            }
        };
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
            operation,
            command,
        )
        .map_err(|_| Error::InvalidData)?;
        state.pending = Some(Pending {
            sender,
            route: format!(
                "v1/relationships/{}/{}/commands",
                sender.0, state.profile.persona.0
            ),
            command: PendingCommand::Relationship(signed),
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
        self.finish_alias_setup(&mut state, &keys)?;
        match self.finish_device_setup(&mut state, &keys) {
            Ok(()) | Err(Error::SetupRefused) => {}
            Err(error) => return Err(error),
        }
        match self.advance_request(&mut state, &keys) {
            Ok(()) | Err(Error::RequestRefused) => {}
            Err(error) => return Err(error),
        }
        match self.transmit_conversation_edit(&mut state, &keys) {
            Ok(()) | Err(Error::PolicyRefused | Error::CopyDeletionRefused) => {}
            Err(error) => return Err(error),
        }
        if state.sending.is_some() {
            match self.transmit_send(&mut state, &keys) {
                Ok(_) | Err(Error::SendRefused) => {}
                Err(error) => return Err(error),
            }
        }
        self.transmit_external(&mut state, &keys)?;
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
            Some(&pending.command.body()?),
        )?;
        let receipt = &answer.receipt;
        let scope = state.profile.deployment.scope;
        let digest = pending.command.digest()?;
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

/// Constructs recipient evidence; canonical lane validation and settlement remain server-owned.
fn lane_grant(
    state: &State,
    keys: &storage::Secrets,
    view: &RelationshipView,
    lifetime_ms: u64,
    issued: CanonicalTime,
    operation: IdempotencyKey,
) -> Result<PendingCommand, Error> {
    use cs_mail_capabilities::{
        DeclarationAuthorityConstraint, LaneMode, LaneSubject, RateLimit, SenderScopedGrant,
        SignedLaneGrant,
    };
    use cs_mail_primitives::{
        DeclaredPurpose, Duration, KnownPurpose, LaneId, OriginMode, Version,
    };
    use ed25519_dalek::Signer;
    if lifetime_ms == 0 {
        return Err(Error::InvalidData);
    }
    let guest = view.guest.is_some();
    let grant = SenderScopedGrant {
        id: view
            .lane
            .as_ref()
            .map_or(LaneId(operation.0), |lane| lane.grant().id),
        subject: if guest {
            LaneSubject::GuestMailbox(view.handle.sender)
        } else {
            LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                *view.handle.relationship.as_bytes(),
            ))
        },
        sender: view.handle.sender,
        recipient: state.profile.persona,
        protocol_version: state.profile.deployment.policy.protocol_version,
        deployment_domain: state.profile.deployment.scope.deployment_domain,
        intended_provider: state.profile.deployment.scope.provider,
        recipient_operational_key: state.profile.recipient_key,
        purpose: if guest {
            DeclaredPurpose::Unspecified
        } else {
            DeclaredPurpose::Known(KnownPurpose::Personal)
        },
        origin: if guest {
            None
        } else {
            Some(OriginMode::HumanInitiated)
        },
        declaration_authority: if guest {
            DeclarationAuthorityConstraint::GuestGateway
        } else {
            DeclarationAuthorityConstraint::NativeSender
        },
        lifetime: Duration(lifetime_ms),
        rate_limit: RateLimit {
            max_messages: 20,
            interval: Duration(86_400_000),
        },
        mode: LaneMode::Expiring,
        issued_at: issued,
        not_before: issued,
        not_after: issued
            .checked_add(Duration(lifetime_ms))
            .ok_or(Error::InvalidData)?,
        version: Version(view.lane.as_ref().map_or(Ok(1), |l| {
            l.grant().version.0.checked_add(1).ok_or(Error::InvalidData)
        })?),
    };
    let signature = ed25519_dalek::SigningKey::from_bytes(&keys.recipient)
        .sign(&grant.signing_bytes().map_err(|_| Error::InvalidData)?)
        .to_bytes();
    Ok(PendingCommand::Lane {
        grant: Box::new(SignedLaneGrant { grant, signature }),
        idempotency_key: operation,
    })
}

impl MemberClient {
    /// Journals encrypted content and both operation identities before network work.
    /// # Errors
    /// Refuses replacing pending work, unavailable custody or a conflicting selection.
    pub fn send_message(
        &self,
        other: &str,
        conversation: Option<u128>,
        text: &str,
        sources: &[SourceDraft],
    ) -> Result<cs_mail_primitives::MessageId, Error> {
        self.send_message_with_draft(other, conversation, text, sources, None, None)
    }
    /// Send to an explicitly selected receiving alias. The host checks that the
    /// alias belongs to `other`; this does not discover or disclose sibling aliases.
    /// # Errors
    /// Rejects pending work, invalid targets or a refused delivery; uncertain sends retain their journal.
    pub fn send_message_to_alias(
        &self,
        other: &str,
        alias: cs_mail_primitives::AliasId,
        conversation: Option<u128>,
        text: &str,
    ) -> Result<cs_mail_primitives::MessageId, Error> {
        self.send_message_with_draft(other, conversation, text, &[], None, Some(alias))
    }
    pub(super) fn send_message_with_draft(
        &self,
        other: &str,
        conversation: Option<u128>,
        text: &str,
        sources: &[SourceDraft],
        draft: Option<&Draft>,
        alias: Option<cs_mail_primitives::AliasId>,
    ) -> Result<cs_mail_primitives::MessageId, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.requesting.is_some() {
            return Err(Error::RequestPending);
        }
        if state.editing_conversation.is_some() {
            return Err(Error::DecisionPending);
        }
        if state.sending.is_some() {
            return Err(Error::SendPending);
        }
        if state.pending.is_some() {
            return Err(Error::DecisionPending);
        }
        if state.setup.is_some() {
            return Err(Error::SetupPending);
        }
        require_idle(&state)?;
        self.synchronize(&mut state, &keys)?;
        let other = parse_sender(other)?;
        state.last_send = None;
        let mut pending = self.prepare_send(
            &state,
            &keys,
            correspondence::SendWriting {
                alias,
                other,
                conversation,
                text,
                sources,
                draft,
            },
        )?;
        if let Some(alias) = alias {
            let cs_mail_application::correspondence::Command::Send { package, .. } =
                pending.send.command
            else {
                return Err(Error::InvalidData);
            };
            pending.create = None;
            pending.send = Self::signed(
                &state.profile,
                &keys,
                cs_mail_application::correspondence::Command::SendToAlias { alias, package },
            )?;
        }
        state.sending = Some(pending);
        self.commit_draft(&keys, &state, draft)?;
        // A known commit is a send outcome. A later view-refresh failure must not
        // turn it into an apparently failed send and invite a fresh duplicate.
        self.transmit_send(&mut state, &keys)
    }
}

fn require_idle(state: &State) -> Result<(), Error> {
    if state.external.as_ref().is_some_and(|w| w.pending.is_some()) {
        return Err(Error::ExternalPending);
    }
    if state.requesting.is_some() {
        return Err(Error::RequestPending);
    }
    if state.pending.is_some() || state.editing_conversation.is_some() {
        return Err(Error::DecisionPending);
    }
    if state.sending.is_some() {
        return Err(Error::SendPending);
    }
    if state.setup.is_some() {
        return Err(Error::SetupPending);
    }
    Ok(())
}

/// A partial observation is useful mail, never evidence that pending work failed.
pub struct MailRefresh {
    pub inbox: Inbox,
    pub native_current: bool,
    pub external_current: bool,
}
fn observation(result: Result<(), Error>) -> Result<bool, Error> {
    match result {
        Ok(()) => Ok(true),
        Err(Error::Offline | Error::Unavailable) => Ok(false),
        Err(error) => Err(error),
    }
}
