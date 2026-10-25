//! External email is orchestrated here; permission and disclosure decisions stay server-owned.
use super::{
    Error, MemberClient, Profile, State,
    storage::{self, Secrets},
};
use cs_mail_application::correspondence::external::{
    Command, Direction, MessageView, Outcome, PreparedMessage, Response, SignedCommand,
};
use cs_mail_content::{ContentBinding, ContentCertificateDigest, EndpointPublicKey};
use cs_mail_correspondence::{
    ConversationId, Document, DocumentPart,
    external::{ExternalConversation, delivery::DeliveryState},
};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{
    ContentRef, DeclarationAuthority, DeclaredPurpose, IdempotencyKey, KnownPurpose,
    MessageDeclarations, MessageId, MessageValidityUntil, OriginDeclaration, OriginMode,
    WireVersion,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) const MAX_TEXT_BYTES: usize = 16384;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Workspace {
    pub(super) pending: Option<SignedCommand>,
    pub(super) records: BTreeMap<u128, Record>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Record {
    pub(super) conversation: ExternalConversation,
    pub(super) relationship: cs_mail_protocol::Relationship,
    pub(super) messages: Vec<MessageView>,
}
#[derive(Clone, Debug)]
pub struct ExternalInbox {
    pub pending: Option<String>,
    pub threads: Vec<ExternalThread>,
}
#[derive(Clone, Debug)]
pub struct ExternalThread {
    pub id: String,
    pub mailbox: String,
    pub guest: String,
    pub permission: String,
    pub permission_route: super::PermissionRoute,
    pub pending_invitation: bool,
    pub disclosure: bool,
    pub blocked: bool,
    pub messages: Vec<ExternalMessage>,
}
#[derive(Clone, Debug)]
pub struct ExternalMessage {
    pub id: String,
    pub text: String,
    pub outgoing: bool,
    pub committed_at: u64,
    pub sequence: u64,
    pub delivery: Option<DeliveryState>,
    pub provider_events: Vec<cs_mail_correspondence::external::delivery::ProviderEvent>,
    pub content: super::ReadingContent,
}
#[derive(Clone, Copy, Debug)]
pub enum ExternalAction {
    Reject,
    WithdrawDisclosure,
    GrantDisclosure,
}
#[derive(Deserialize)]
struct Preparation {
    relationship: cs_mail_application::members::RelationshipHandle,
    member_key: EndpointPublicKey,
    relay_key: EndpointPublicKey,
}
fn identifier() -> Result<u128, Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(u128::from_be_bytes(bytes) | 1)
}
fn conversation_id(value: &str) -> Result<ConversationId, Error> {
    ConversationId::new(value.parse().map_err(|_| Error::InvalidData)?)
        .map_err(|_| Error::InvalidData)
}
impl MemberClient {
    fn external_signed(
        profile: &Profile,
        keys: &Secrets,
        conversation: ConversationId,
        command: Command,
    ) -> Result<SignedCommand, Error> {
        SignedCommand {
            scope: profile.deployment.scope,
            product: profile.deployment.product.clone(),
            account: profile.account,
            persona: profile.persona,
            key: profile.sender_key,
            operation: IdempotencyKey(identifier()?),
            conversation,
            command,
            signature: vec![],
        }
        .sign(&keys.device)
        .map_err(|_| Error::InvalidData)
    }
    fn external_call(&self, profile: &Profile, command: &SignedCommand) -> Result<Response, Error> {
        self.call(
            &profile.server,
            "v1/external/commands",
            Some(&serde_json::to_vec(command).map_err(|_| Error::InvalidData)?),
        )
    }
    pub(super) fn transmit_external(&self, state: &mut State, keys: &Secrets) -> Result<(), Error> {
        let Some(pending) = state.external.as_ref().and_then(|w| w.pending.as_ref()) else {
            return Ok(());
        };
        let response = self.external_call(&state.profile, pending);
        let evidence;
        match response {
            Ok(Response::Outcome(acknowledgement)) => {
                acknowledgement
                    .verify(pending, &state.profile.deployment.provider_key)
                    .map_err(|_| Error::InvalidData)?;
                evidence = Some(super::personal::WorkEvidence::External(
                    acknowledgement.clone(),
                ));
                let outcome = acknowledgement.receipt.outcome;
                if matches!(outcome, Outcome::Refused(_)) {
                    state.external.as_mut().ok_or(Error::InvalidData)?.pending = None;
                    self.finish_draft(
                        keys,
                        state,
                        super::drafts::DraftResolution::RestoreEditable,
                        evidence,
                    )?;
                    return Err(Error::ExternalRefused);
                }
                let matched = match (&pending.command, &outcome) {
                    (
                        Command::Start { message, .. } | Command::Send { message, .. },
                        Outcome::Queued(id),
                    ) => *id == message.manifest.message(),
                    (Command::Reject, Outcome::Rejected)
                    | (Command::WithdrawDisclosure, Outcome::DisclosureWithdrawn)
                    | (Command::GrantDisclosure, Outcome::DisclosureGranted) => true,
                    _ => false,
                };
                if !matched {
                    return Err(Error::InvalidData);
                }
            }
            Err(error) => return Err(error),
            _ => return Err(Error::InvalidData),
        }
        state.external.as_mut().ok_or(Error::InvalidData)?.pending = None;
        self.finish_draft(
            keys,
            state,
            super::drafts::DraftResolution::RemoveSubmitted,
            evidence,
        )
    }
    pub(super) fn sync_external(&self, state: &mut State, keys: &Secrets) -> Result<(), Error> {
        let ids: std::collections::BTreeSet<_> = state
            .inbox
            .iter()
            .flat_map(|v| v.external_conversations.iter().copied())
            .collect();
        for id in ids {
            self.fetch_external(state, keys, id)?;
        }
        Ok(())
    }
    fn fetch_external(
        &self,
        state: &mut State,
        keys: &Secrets,
        id: ConversationId,
    ) -> Result<(), Error> {
        let mut messages = Vec::new();
        let mut after = 0;
        let record = loop {
            let command = Self::external_signed(&state.profile, keys, id, Command::Read { after })?;
            let Response::Conversation {
                conversation,
                relationship,
                messages: page,
                next_after,
            } = self.external_call(&state.profile, &command)?
            else {
                return Err(Error::InvalidData);
            };
            if conversation.id() != id
                || conversation.member() != state.profile.persona
                || relationship.key.sender != conversation.guest()
                || relationship.key.recipient != state.profile.persona
            {
                return Err(Error::InvalidData);
            }
            for entry in &page {
                entry
                    .message
                    .validate(&conversation, &relationship)
                    .map_err(|_| Error::InvalidData)?;
            }
            if page.is_empty() && next_after == after {
                break Record {
                    conversation: *conversation,
                    relationship: *relationship,
                    messages,
                };
            }
            if next_after <= after
                || page
                    .iter()
                    .any(|v| v.sequence <= after || v.sequence > next_after)
            {
                return Err(Error::InvalidData);
            }
            messages.extend(page);
            if messages.len() > 10000 {
                return Err(Error::InvalidData);
            }
            after = next_after;
        };
        state
            .external
            .get_or_insert_with(Workspace::default)
            .records
            .insert(id.value(), record);
        storage::save(self.store.as_ref(), keys, state)
    }
    /// Opens encrypted copies in memory; a saved snapshot establishes no current authority.
    /// # Errors
    /// Unavailable custody or invalid content binding.
    pub fn external_inbox(&self) -> Result<ExternalInbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let (secret, _) = super::correspondence::own_keys(&state.profile, &keys)?;
        let mut threads = Vec::new();
        let now = (self.clock)();
        let personal = super::personal::Personal::load(self, &keys, &state)?;
        if let Some(workspace) = &state.external {
            for record in workspace.records.values() {
                let c = &record.conversation;
                let r = &record.relationship;
                let mut messages = Vec::new();
                for entry in &record.messages {
                    if entry.message.member_copy.expires_at <= now {
                        messages.push(ExternalMessage {
                            id: entry.message.manifest.message().0.to_string(),
                            text: "(expired)".into(),
                            outgoing: entry.message.direction == Direction::MemberToGuest,
                            committed_at: entry.message.committed_at.0,
                            sequence: entry.sequence,
                            delivery: entry.delivery.clone(),
                            provider_events: entry.provider_events.clone(),
                            content: super::ReadingContent::Unavailable,
                        });
                        continue;
                    }
                    entry
                        .message
                        .validate(c, r)
                        .map_err(|_| Error::InvalidData)?;
                    let bytes = cs_mail_content::decrypt(
                        &secret,
                        entry.message.sender_key,
                        &entry.message.member_copy,
                    )
                    .map_err(|_| Error::InvalidData)?;
                    let doc: Document =
                        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidData)?;
                    if doc
                        .manifest(entry.message.manifest.message(), c.id())
                        .map_err(|_| Error::InvalidData)?
                        != entry.message.manifest
                        || !entry.message.manifest.sources().is_empty()
                    {
                        return Err(Error::InvalidData);
                    }
                    let mut content = super::ReadingContent::from_external_document(&doc);
                    Self::apply_resources(
                        &personal,
                        &mut content,
                        super::MailKey::External(c.id()),
                        entry.message.manifest.message(),
                        entry.message.manifest.version(),
                        now,
                    );
                    messages.push(ExternalMessage {
                        id: entry.message.manifest.message().0.to_string(),
                        text: content.search_text()?,
                        outgoing: entry.message.direction == Direction::MemberToGuest,
                        committed_at: entry.message.committed_at.0,
                        sequence: entry.sequence,
                        delivery: entry.delivery.clone(),
                        provider_events: entry.provider_events.clone(),
                        content,
                    });
                }
                let pending = c.pending_access(r);
                let (permission, permission_route) = permission_for(record, &state.inbox, now);
                threads.push(ExternalThread {
                    id: c.id().value().to_string(),
                    mailbox: c.mailbox().address(),
                    guest: c.guest().0.to_string(),
                    permission: if pending {
                        "Pending invitation · replies only in this conversation"
                    } else {
                        permission
                    }
                    .into(),
                    permission_route,
                    pending_invitation: pending,
                    disclosure: c.disclosure().permits_dispatch(),
                    blocked: r.state == cs_mail_protocol::RelationshipState::Blocked,
                    messages,
                });
            }
        }
        let current = storage::secrets(self.store.as_ref())?;
        if current.cache != keys.cache {
            return Err(Error::KeyUnavailable);
        }
        Ok(ExternalInbox {
            pending: state
                .external
                .as_ref()
                .and_then(|w| w.pending.as_ref())
                .map(|p| p.conversation.value().to_string()),
            threads,
        })
    }
    /// Journals the exact ciphertext and explicit consent before the first send.
    /// # Errors
    /// Invalid destination/text, unresolved work, unavailable authority or transport.
    pub fn send_external(
        &self,
        conversation: Option<&str>,
        mailbox: &str,
        text: &str,
        disclose: bool,
    ) -> Result<String, Error> {
        self.send_external_with_draft(conversation, mailbox, text, disclose, None)
            .map(|id| id.value().to_string())
    }
    #[allow(clippy::too_many_lines)] // Keep preparation, encryption and the atomic draft/signed-send handoff together.
    pub(super) fn send_external_with_draft(
        &self,
        conversation: Option<&str>,
        mailbox: &str,
        text: &str,
        disclose: bool,
        draft: Option<&super::Draft>,
    ) -> Result<ConversationId, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        let composed = draft.and_then(|d| {
            if let super::DraftBody::Composed { content, .. } = d.body() {
                Some(content)
            } else {
                None
            }
        });
        if (text.trim().is_empty() && composed.is_none_or(|c| !c.has_content()))
            || (composed.is_none() && text.len() > MAX_TEXT_BYTES)
            || (conversation.is_none() && !disclose)
        {
            return Err(Error::InvalidData);
        }
        let mailbox = GuestMailbox::parse(mailbox).map_err(|_| Error::InvalidData)?;
        let id = match conversation {
            Some(value) => conversation_id(value)?,
            None => ConversationId::new(identifier()?).map_err(|_| Error::InvalidData)?,
        };
        if conversation.is_some() {
            self.fetch_external(&mut state, &keys, id)?;
            let record = state
                .external
                .as_ref()
                .and_then(|w| w.records.get(&id.value()))
                .ok_or(Error::InvalidData)?;
            if record.conversation.mailbox() != &mailbox {
                return Err(Error::InvalidData);
            }
        }
        let preparation: Preparation = self.call(
            &state.profile.server,
            "v1/external/prepare",
            Some(
                &serde_json::to_vec(
                    &serde_json::json!({"member":state.profile.persona,"mailbox":mailbox}),
                )
                .map_err(|_| Error::InvalidData)?,
            ),
        )?;
        if preparation.relationship.recipient != state.profile.persona
            || preparation.relationship.sender
                != mailbox
                    .synthetic_protocol_identity(&state.profile.deployment.scope.deployment_domain)
        {
            return Err(Error::InvalidData);
        }
        let mut salt = [0; 32];
        getrandom::fill(&mut salt).map_err(|_| Error::Unavailable)?;
        let part = composed.map_or_else(
            || DocumentPart::Text(text.into()),
            |c| DocumentPart::Composed(c.clone()),
        );
        let mut doc = Document::new(vec![part], salt).map_err(|_| Error::InvalidData)?;
        if let Some(draft) = draft {
            if !draft.subject().trim().is_empty() {
                doc = doc.with_subject(self.new_subject(Some(draft))?);
            }
        }
        let message = MessageId(identifier()?);
        let manifest = doc.manifest(message, id).map_err(|_| Error::InvalidData)?;
        let (secret, sender_key) = super::correspondence::own_keys(&state.profile, &keys)?;
        if preparation.member_key != sender_key {
            return Err(Error::InvalidData);
        }
        let at = (self.clock)();
        let expires = at
            .checked_add(super::correspondence::MESSAGE_LIFETIME)
            .ok_or(Error::InvalidData)?;
        let binding = ContentBinding {
            wire_version: WireVersion(1),
            protocol_version: state.profile.deployment.policy.protocol_version,
            content_ref: ContentRef(identifier()?),
            message_id: message,
            sender: state.profile.persona,
            recipient: state.profile.persona,
            relationship: preparation.relationship.relationship,
            content_scope: preparation.relationship.content_scope,
            sender_certificate: ContentCertificateDigest([0; 32]),
            declarations: MessageDeclarations {
                purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
                origin: OriginDeclaration {
                    mode: OriginMode::HumanInitiated,
                    authority: DeclarationAuthority::NativeSender(state.profile.sender_key),
                },
                payload_schema: None,
            },
            message_valid_until: MessageValidityUntil(expires),
            capability: None,
        };
        let bytes = serde_json::to_vec(&doc).map_err(|_| Error::InvalidData)?;
        let package = Box::new(PreparedMessage {
            manifest,
            sender_key,
            member_copy: cs_mail_content::encrypt(
                &secret,
                preparation.member_key,
                binding.clone(),
                &bytes,
                at,
                expires,
            )
            .map_err(|_| Error::InvalidData)?,
            relay_copy: cs_mail_content::encrypt(
                &secret,
                preparation.relay_key,
                binding,
                &bytes,
                at,
                expires,
            )
            .map_err(|_| Error::InvalidData)?,
        });
        let command = if conversation.is_none() {
            Command::Start {
                receiving_alias: self.receiving_alias(&state.profile, state.profile.persona)?,
                mailbox,
                message: package,
                disclose_to_external_mail: disclose,
            }
        } else {
            Command::Send {
                message: package,
                disclose_to_external_mail: disclose,
            }
        };
        state
            .external
            .get_or_insert_with(Workspace::default)
            .pending = Some(Self::external_signed(&state.profile, &keys, id, command)?);
        self.commit_draft(&keys, &state, draft)?;
        self.transmit_external(&mut state, &keys)?;
        Ok(id)
    }
    /// # Errors
    /// Unresolved work, invalid conversation, or unknown/refused command outcome.
    pub fn control_external(
        &self,
        conversation: &str,
        action: ExternalAction,
    ) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        let id = conversation_id(conversation)?;
        let command = match action {
            ExternalAction::Reject => Command::Reject,
            ExternalAction::WithdrawDisclosure => Command::WithdrawDisclosure,
            ExternalAction::GrantDisclosure => Command::GrantDisclosure,
        };
        state
            .external
            .get_or_insert_with(Workspace::default)
            .pending = Some(Self::external_signed(&state.profile, &keys, id, command)?);
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.transmit_external(&mut state, &keys)?;
        Ok(())
    }
}

fn permission_for(
    record: &Record,
    views: &[cs_mail_application::members::RelationshipView],
    now: cs_mail_primitives::CanonicalTime,
) -> (&'static str, super::PermissionRoute) {
    let c = &record.conversation;
    let r = &record.relationship;
    let lane = views
        .iter()
        .find(|view| {
            view.handle.sender == c.guest()
                && view.handle.recipient == c.member()
                && view.version == r.version
                && view.state == r.state
        })
        .and_then(|view| view.lane.as_ref());
    let (permission, permission_route) = super::views::permission(r.state, lane, Some(now));
    (permission, permission_route)
}
