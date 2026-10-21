//! Native correspondence for the member application: mailbox metadata, the conversations
//! with one contact, and journaled sends. Plaintext is decrypted in memory, never cached.
use super::{Error, MemberClient, Profile, State, storage::Secrets};
use crate::correspondence::{EndpointKeys, compose, prepare};
use cs_mail_application::correspondence::{Command, MessageRecord, Response, SignedRequest};
use cs_mail_application::members::{MemberAnswer, RelationshipHandle, RelationshipView};
use cs_mail_content::{
    ContentBinding, ContentCertificateDigest, EndpointPublicKey, EndpointSecretKey,
};
use cs_mail_correspondence::{ConversationId, DocumentPart};
use cs_mail_primitives::{
    ContentRef, DeclarationAuthority, DeclaredPurpose, Duration, IdempotencyKey, KnownPurpose,
    MessageDeclarations, MessageId, MessageValidityUntil, OriginDeclaration, OriginMode,
    ProtocolIdentity, RequestId, WireVersion,
};
use cs_mail_protocol::{AdmissionBasis, RelationshipState};
use cs_mail_security::{EndpointKeyPublication, MemberRead};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const ROUTE: &str = "v1/correspondence/commands";
/// Largest mailbox page the service returns.
const PAGE: u16 = 100;
/// How long a sent message's copies and validity last.
pub(super) const MESSAGE_LIFETIME: Duration = Duration(30 * 24 * 60 * 60 * 1000);
/// Largest text a member may send in one message.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

/// Mailbox metadata: headers and conversation participants, never content.
#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct Mailbox {
    after: u64,
    pub(super) records: Vec<(u64, MessageRecord)>,
    /// The other participant of each conversation this address belongs to.
    pub(super) participants: BTreeMap<u128, ProtocolIdentity>,
}
/// One exact send, journaled before transmission and resent unchanged until answered.
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct PendingSend {
    pub(super) other: ProtocolIdentity,
    pub(super) create: Option<SignedRequest>,
    pub(super) send: SignedRequest,
}

#[derive(Clone, Copy)]
pub(super) struct SendWriting<'a> {
    pub alias: Option<cs_mail_primitives::AliasId>,
    pub other: ProtocolIdentity,
    pub conversation: Option<u128>,
    pub text: &'a str,
    pub sources: &'a [super::SourceDraft],
    pub draft: Option<&'a super::Draft>,
}

/// How this address may write to a contact now. Presentation only: the service decides
/// each send against current permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyRoute {
    /// Standing permission: any conversation, including new ones.
    Standing,
    /// A current express lane: any conversation, within its scope and allowance.
    Lane,
    /// Only an answer in the conversation the contact's request began.
    Conversation,
    Closed,
}
/// One conversation with a contact, labelled from content decrypted on this device.
#[derive(Clone, Debug)]
pub struct ConversationSummary {
    pub id: String,
    /// The opening of its first readable message.
    pub label: String,
    /// The received request this conversation began with, if any.
    pub request: Option<String>,
    pub sent_request: Option<String>,
    /// Confined answer permission for this conversation, projected from Rust facts.
    pub reply_allowed: bool,
    pub last_at: u64,
    pub policy: Option<super::ConversationPolicy>,
}
/// A decrypted message with its place in the correspondence.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub conversation: String,
    pub text: String,
    pub references: Vec<u32>,
    pub outgoing: bool,
    pub committed_at: u64,
    pub sequence: u64,
    pub content: super::ReadingContent,
}
/// Everything shown for one contact's correspondence.
pub(super) struct Correspondence {
    pub threads: Vec<ConversationSummary>,
    pub items: Vec<Item>,
}

impl MemberClient {
    pub(super) fn correspondence_call(
        &self,
        profile: &Profile,
        signed: &SignedRequest,
    ) -> Result<Response, Error> {
        self.call(
            &profile.server,
            ROUTE,
            Some(&serde_json::to_vec(signed).map_err(|_| Error::InvalidData)?),
        )
    }
    pub(super) fn signed(
        profile: &Profile,
        keys: &Secrets,
        command: Command,
    ) -> Result<SignedRequest, Error> {
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        SignedRequest {
            product: profile.deployment.product.clone(),
            account: profile.account,
            persona: profile.persona,
            key: profile.sender_key,
            operation: IdempotencyKey(u128::from_be_bytes(random) | 1),
            command,
            signature: Vec::new(),
        }
        .sign(&keys.device)
        .map_err(|_| Error::InvalidData)
    }
    /// Appends new mailbox headers and learns each conversation's other participant.
    pub(super) fn sync_mailbox(&self, state: &mut State, keys: &Secrets) -> Result<(), Error> {
        let me = state.profile.persona;
        loop {
            let after = state.mailbox.after;
            let signed = Self::signed(
                &state.profile,
                keys,
                Command::Mailbox { after, limit: PAGE },
            )?;
            let Response::Mailbox {
                entries,
                next_after,
            } = self.correspondence_call(&state.profile, &signed)?
            else {
                return Err(Error::InvalidData);
            };
            for (sequence, record) in entries {
                if record.author != me && record.receiving.recipient() != me {
                    return Err(Error::InvalidData);
                }
                let conversation = record.manifest.conversation().value();
                if !state.mailbox.participants.contains_key(&conversation) {
                    let other = if record.author == me {
                        let signed = Self::signed(
                            &state.profile,
                            keys,
                            Command::Policy {
                                id: record.manifest.conversation(),
                            },
                        )?;
                        let Response::Conversation(found) =
                            self.correspondence_call(&state.profile, &signed)?
                        else {
                            return Err(Error::InvalidData);
                        };
                        found.scope().other(me).map_err(|_| Error::InvalidData)?
                    } else {
                        record.author
                    };
                    state.mailbox.participants.insert(conversation, other);
                }
                let other = state
                    .mailbox
                    .participants
                    .get(&conversation)
                    .ok_or(Error::InvalidData)?;
                if (record.author == me && record.receiving.recipient() != *other)
                    || (record.author != me && record.author != *other)
                {
                    return Err(Error::InvalidData);
                }
                state.mailbox.records.push((sequence, record));
            }
            if next_after <= after {
                break;
            }
            state.mailbox.after = next_after;
        }
        Ok(())
    }
    pub(super) fn endpoint_key(
        &self,
        profile: &Profile,
        keys: &Secrets,
        persona: ProtocolIdentity,
    ) -> Result<EndpointPublicKey, Error> {
        if persona == profile.persona {
            return Ok(own_keys(profile, keys)?.1);
        }
        let publication: EndpointKeyPublication = self.call(
            &profile.server,
            &format!("v1/addresses/{}/endpoint-key", persona.0),
            None,
        )?;
        if publication.persona != persona {
            return Err(Error::InvalidData);
        }
        Ok(publication.key)
    }
    pub(super) fn receiving_alias(
        &self,
        profile: &Profile,
        owner: ProtocolIdentity,
    ) -> Result<cs_mail_primitives::AliasId, Error> {
        let published: cs_mail_application::aliases::PublicReceivingAlias = self.call(
            &profile.server,
            &format!("v1/addresses/{}/receiving-alias", owner.0),
            None,
        )?;
        if published.owner != owner {
            return Err(Error::InvalidData);
        }
        Ok(published.id)
    }
    pub(super) fn sending(
        &self,
        profile: &Profile,
        keys: &Secrets,
        other: ProtocolIdentity,
    ) -> Result<(RelationshipHandle, Option<RelationshipView>), Error> {
        let MemberAnswer::Sending { handle, view } =
            self.read(profile, keys, MemberRead::AsSender { recipient: other })?
        else {
            return Err(Error::InvalidData);
        };
        if handle.sender != profile.persona || handle.recipient != other {
            return Err(Error::InvalidData);
        }
        Ok((handle, view))
    }
    /// Decrypts, in memory, every message exchanged with `other`.
    pub(super) fn correspondence_with(
        &self,
        state: &State,
        keys: &Secrets,
        other: ProtocolIdentity,
    ) -> Result<Correspondence, Error> {
        let me = state.profile.persona;
        let mut items = Vec::new();
        let mut threads: BTreeMap<u128, ConversationSummary> = BTreeMap::new();
        for (sequence, record) in &state.mailbox.records {
            let conversation = record.manifest.conversation().value();
            if state.mailbox.participants.get(&conversation) != Some(&other) {
                continue;
            }
            let request = match record.admission {
                AdmissionBasis::InitialRequest { request } if record.author == other => {
                    Some(request)
                }
                _ => None,
            };
            let message = record.manifest.message();
            let content = self.read_native(state, keys, record)?;
            let text = content.search_text().unwrap_or_else(|_| match content {
                super::ReadingContent::NotRetained => "(not downloaded on this device)".into(),
                super::ReadingContent::AccessDenied => "(access denied)".into(),
                _ => "(deleted or expired)".into(),
            });
            let references = if let super::ReadingContent::Readable(parts) = &content {
                parts
                    .iter()
                    .filter_map(|p| {
                        if let super::ReadingPart::Reference { part, .. } = p {
                            Some(*part)
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let thread = threads
                .entry(conversation)
                .or_insert_with(|| ConversationSummary {
                    id: conversation.to_string(),
                    label: super::retained::Retained::load(self, keys, state)
                        .ok()
                        .and_then(|index| {
                            index
                                .subject(
                                    super::MailKey::Native(record.manifest.conversation()),
                                    (self.clock)(),
                                )
                                .map(|subject| subject.text().to_owned())
                        })
                        .unwrap_or_else(|| "Subject unavailable".into()),
                    request: request.map(|r: RequestId| r.0.to_string()),
                    sent_request: match record.admission {
                        AdmissionBasis::InitialRequest { request } if record.author == me => {
                            Some(request.0.to_string())
                        }
                        _ => None,
                    },
                    reply_allowed: false,
                    last_at: 0,
                    policy: None,
                });
            thread.last_at = thread.last_at.max(record.committed_at.0);
            items.push(Item {
                id: message.0.to_string(),
                conversation: conversation.to_string(),
                text,
                references,
                outgoing: record.author == me,
                committed_at: record.committed_at.0,
                sequence: *sequence,
                content,
            });
        }
        let mut threads: Vec<ConversationSummary> = threads.into_values().collect();
        threads.sort_by_key(|thread| std::cmp::Reverse(thread.last_at));
        Ok(Correspondence { threads, items })
    }
    /// Presentation derived from the sending direction's current view.
    fn reply_route(
        view: Option<&RelationshipView>,
        now: super::CanonicalTime,
        has_answer: bool,
    ) -> ReplyRoute {
        match view {
            Some(view) if view.state == RelationshipState::Accepted => ReplyRoute::Standing,
            Some(view)
                if matches!(
                    view.state,
                    RelationshipState::Blocked | RelationshipState::Revoked
                ) =>
            {
                ReplyRoute::Closed
            }
            Some(view) if view.lane.as_ref().is_some_and(|lane| lane.is_current(now)) => {
                ReplyRoute::Lane
            }
            Some(_) if has_answer => ReplyRoute::Conversation,
            _ => ReplyRoute::Closed,
        }
    }
    /// Composes, journals and sends `text` to `other`: in `conversation`, or in a new one.
    #[allow(clippy::too_many_lines)] // Bind subject, ciphertext and receiving destination before journaling one immutable send.
    pub(super) fn prepare_send(
        &self,
        state: &State,
        keys: &Secrets,
        writing: SendWriting<'_>,
    ) -> Result<PendingSend, Error> {
        let SendWriting {
            alias,
            other,
            conversation,
            text,
            sources,
            draft,
        } = writing;
        if (text.trim().is_empty() && sources.is_empty()) || text.len() > MAX_TEXT_BYTES {
            return Err(Error::InvalidData);
        }
        let subject = if let Some(id) = conversation {
            self.conversation_subject(
                state,
                keys,
                ConversationId::new(id).map_err(|_| Error::InvalidData)?,
            )?
        } else {
            self.new_subject(draft)?
        };
        let profile = &state.profile;
        let (handle, view) = self.sending(profile, keys, other)?;
        let now = (self.clock)();
        let (conversation, create) =
            Self::plan_conversation(state, keys, other, conversation, subject.commitment())?;
        let replying = if create.is_none() {
            let signed = Self::signed(profile, keys, Command::Policy { id: conversation })?;
            let Response::Conversation(policy) = self.correspondence_call(profile, &signed)? else {
                return Err(Error::InvalidData);
            };
            view.as_ref().is_some_and(|view| {
                policy
                    .reply_source(profile.persona, view.reply_epoch)
                    .is_some()
                    && view.state.sender_reply_restriction().is_none()
            })
        } else {
            false
        };
        let capability = match &view {
            Some(view) if view.state == RelationshipState::Accepted || replying => None,
            Some(view) => view
                .lane
                .as_ref()
                .filter(|lane| lane.is_current(now))
                .map(|lane| lane.grant().id),
            None => None,
        };
        let (secret, public) = own_keys(profile, keys)?;
        let mut ids = [0; 32];
        getrandom::fill(&mut ids).map_err(|_| Error::Unavailable)?;
        let mut content = [0; 16];
        let mut message = [0; 16];
        content.copy_from_slice(&ids[..16]);
        message.copy_from_slice(&ids[16..]);
        let expires = now
            .checked_add(MESSAGE_LIFETIME)
            .ok_or(Error::InvalidData)?;
        let mut parts = Vec::new();
        if !text.is_empty() {
            parts.push(DocumentPart::Text(text.to_owned()));
        }
        parts.extend(self.source_parts(state, keys, sources)?);
        let package = prepare(
            &EndpointKeys {
                owner: profile.persona,
                secret: &secret,
                public,
            },
            &compose(parts)
                .map_err(|_| Error::InvalidData)?
                .with_subject(subject),
            conversation,
            self.endpoint_key(profile, keys, other)?,
            profile.deployment.custody_key,
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(u128::from_be_bytes(content) | 1),
                message_id: MessageId(u128::from_be_bytes(message) | 1),
                sender: profile.persona,
                recipient: other,
                protocol_version: profile.deployment.policy.protocol_version,
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                // A native send is authenticated by its signed command.
                sender_certificate: ContentCertificateDigest([0; 32]),
                declarations: native_declarations(profile),
                message_valid_until: MessageValidityUntil(expires),
                capability,
            },
            (now, expires),
        )
        .map_err(|_| Error::InvalidData)?;
        Ok(PendingSend {
            other,
            create,
            send: Self::signed(
                profile,
                keys,
                Command::Send {
                    alias: match alias {
                        Some(id) => id,
                        None => self.receiving_alias(profile, other)?,
                    },
                    package: Box::new(package),
                },
            )?,
        })
    }
    fn plan_conversation(
        state: &State,
        keys: &Secrets,
        other: ProtocolIdentity,
        conversation: Option<u128>,
        subject: cs_mail_correspondence::SubjectCommitment,
    ) -> Result<(ConversationId, Option<SignedRequest>), Error> {
        let profile = &state.profile;
        Ok(if let Some(id) = conversation {
            if state.mailbox.participants.get(&id) != Some(&other) {
                return Err(Error::InvalidData);
            }
            (
                ConversationId::new(id).map_err(|_| Error::InvalidData)?,
                None,
            )
        } else {
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
            let id = ConversationId::new(u128::from_be_bytes(random) | 1)
                .map_err(|_| Error::InvalidData)?;
            let create = Self::signed(profile, keys, Command::Create { id, other, subject })?;
            (id, Some(create))
        })
    }
    /// Resolves only a matching durable commit/refusal; transport uncertainty retains exact bytes.
    pub(super) fn transmit_send(
        &self,
        state: &mut State,
        keys: &Secrets,
    ) -> Result<MessageId, Error> {
        let pending = state.sending.as_ref().ok_or(Error::InvalidData)?;
        let mut refusal = None;
        let result = (|| {
            if let Some(create) = &pending.create {
                let Command::Create { id, .. } = create.command else {
                    return Err(Error::InvalidData);
                };
                match self.correspondence_call(&state.profile, create)? {
                    Response::Conversation(conversation) if conversation.id() == id => {}
                    Response::Refused {
                        operation,
                        digest,
                        reason,
                    } if operation == create.operation
                        && digest == create.digest().map_err(|_| Error::InvalidData)? =>
                    {
                        refusal = Some((operation, reason));
                        return Err(Error::SendRefused);
                    }
                    _ => return Err(Error::InvalidData),
                }
            }
            let (Command::Send { package, .. } | Command::SendToAlias { package, .. }) =
                &pending.send.command
            else {
                return Err(Error::InvalidData);
            };
            match self.correspondence_call(&state.profile, &pending.send)? {
                Response::Committed(id) if id == package.manifest.message() => Ok(id),
                Response::Refused {
                    operation,
                    digest,
                    reason,
                } if operation == pending.send.operation
                    && digest == pending.send.digest().map_err(|_| Error::InvalidData)? =>
                {
                    refusal = Some((operation, reason));
                    Err(Error::SendRefused)
                }
                _ => Err(Error::InvalidData),
            }
        })();
        match result {
            Ok(id) => {
                state.sending = None;
                state.last_send = Some(super::SendOutcome::Committed(id));
                self.finish_draft(
                    keys,
                    state,
                    super::drafts::DraftResolution::RemoveSubmitted,
                    state
                        .last_send
                        .clone()
                        .map(super::personal::WorkEvidence::Native),
                )?;
                Ok(id)
            }
            Err(Error::SendRefused) => {
                state.sending = None;
                let (operation, reason) = refusal.ok_or(Error::InvalidData)?;
                state.last_send = Some(super::SendOutcome::Refused { operation, reason });
                self.finish_draft(
                    keys,
                    state,
                    super::drafts::DraftResolution::RestoreEditable,
                    state
                        .last_send
                        .clone()
                        .map(super::personal::WorkEvidence::Native),
                )?;
                Err(Error::SendRefused)
            }
            Err(error) => Err(error),
        }
    }
    /// Reads the viewer's relationship-request entry independently of public invitations.
    /// # Errors
    /// Refuses unavailable custody, invalid pair evidence or an unavailable service.
    pub fn relationship_request_entry(
        &self,
        other: ProtocolIdentity,
    ) -> Result<super::RequestEntry, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let state = super::storage::open(self.store.as_ref(), &keys)?;
        if other == state.profile.persona {
            return Ok(super::RequestEntry::OwnAddress);
        }
        let MemberAnswer::Receiving { handle, view, .. } = self.read(
            &state.profile,
            &keys,
            MemberRead::AsRecipient { sender: other },
        )?
        else {
            return Err(Error::InvalidData);
        };
        if handle.sender != other
            || handle.recipient != state.profile.persona
            || view.as_ref().is_some_and(|view| view.handle != handle)
        {
            return Err(Error::InvalidData);
        }
        let (_, sending) = self.sending(&state.profile, &keys, other)?;
        Ok(super::views::request_entry(
            view.as_ref(),
            sending.as_ref(),
            (self.clock)(),
        ))
    }

    pub(super) fn native_conversation(
        &self,
        state: &mut State,
        keys: &Secrets,
        other: ProtocolIdentity,
    ) -> Result<super::Conversation, Error> {
        self.sync_mailbox(state, keys)?;
        self.retain_batch(state, keys, 100, Some(other))?;
        let MemberAnswer::Receiving { handle, view, .. } = self.read(
            &state.profile,
            keys,
            MemberRead::AsRecipient { sender: other },
        )?
        else {
            return Err(Error::InvalidData);
        };
        if handle.sender != other
            || handle.recipient != state.profile.persona
            || view.as_ref().is_some_and(|view| view.handle != handle)
        {
            return Err(Error::InvalidData);
        }
        let mut data = self.correspondence_with(state, keys, other)?;
        let mut result = view.as_ref().map_or_else(
            || super::views::unestablished(other),
            |view| super::views::summary(view, Some((self.clock)())),
        );
        let (_, sending) = self.sending(&state.profile, keys, other)?;
        let now = (self.clock)();
        result.request_entry = super::views::request_entry(view.as_ref(), sending.as_ref(), now);
        result.sent_requests = sending.as_ref().map_or_else(Vec::new, |view| {
            super::views::project_requests(view, Some(now))
        });
        for thread in &mut data.threads {
            let id = ConversationId::new(thread.id.parse().map_err(|_| Error::InvalidData)?)
                .map_err(|_| Error::InvalidData)?;
            let signed = Self::signed(&state.profile, keys, Command::Policy { id })?;
            let Response::Conversation(policy) =
                self.correspondence_call(&state.profile, &signed)?
            else {
                return Err(Error::InvalidData);
            };
            if policy.id() != id || !policy.scope().contains(state.profile.persona) {
                return Err(Error::InvalidData);
            }
            thread.policy = Some(super::ConversationPolicy {
                restricted: policy.policy()
                    == cs_mail_correspondence::ReusePolicy::OriginatingRelationshipOnly,
                revision: policy.revision(),
                relaxation: match policy.relaxation_proposer() {
                    None => super::Relaxation::None,
                    Some(me) if me == state.profile.persona => super::Relaxation::ProposedByMe,
                    Some(_) => super::Relaxation::AwaitingMyApproval,
                },
            });
            thread.reply_allowed = sending.as_ref().is_some_and(|view| {
                view.state.sender_reply_restriction().is_none()
                    && policy
                        .reply_source(state.profile.persona, view.reply_epoch)
                        .is_some()
            });
        }
        result.reply = Self::reply_route(
            sending.as_ref(),
            (self.clock)(),
            data.threads.iter().any(|thread| thread.reply_allowed),
        );
        result.threads = data.threads;
        result.messages = data
            .items
            .into_iter()
            .map(|item| super::Message {
                id: item.id,
                text: item.text,
                references: item.references,
                conversation: Some(item.conversation),
                outgoing: item.outgoing,
                committed_at: Some(item.committed_at),
                sequence: Some(item.sequence),
                content: item.content,
            })
            .collect();
        super::storage::save(self.store.as_ref(), keys, state)?;
        Ok(result)
    }
    pub(super) fn cached_native_conversation(
        &self,
        state: &State,
        keys: &Secrets,
        other: ProtocolIdentity,
    ) -> Result<super::Conversation, Error> {
        let data = self.correspondence_with(state, keys, other)?;
        let mut result = state
            .inbox
            .iter()
            .find(|view| view.handle.sender == other)
            .map_or_else(
                || super::views::unestablished(other),
                |view| super::views::summary(view, Some((self.clock)())),
            );
        result.threads = data.threads;
        result.messages = data
            .items
            .into_iter()
            .map(|item| super::Message {
                id: item.id,
                text: item.text,
                references: item.references,
                conversation: Some(item.conversation),
                outgoing: item.outgoing,
                committed_at: Some(item.committed_at),
                sequence: Some(item.sequence),
                content: item.content,
            })
            .collect();
        Ok(result)
    }
    pub(super) fn new_subject(
        &self,
        draft: Option<&super::Draft>,
    ) -> Result<cs_mail_correspondence::ConversationSubject, Error> {
        let mut secret = [0; 32];
        getrandom::fill(&mut secret).map_err(|_| Error::Unavailable)?;
        cs_mail_correspondence::ConversationSubject::new(
            draft.map_or("", super::Draft::subject),
            (self.clock)(),
            draft.map_or(0, super::Draft::local_offset_seconds),
            secret,
        )
        .map_err(|_| Error::InvalidData)
    }
    pub(super) fn conversation_subject(
        &self,
        state: &State,
        keys: &Secrets,
        id: ConversationId,
    ) -> Result<cs_mail_correspondence::ConversationSubject, Error> {
        self.retain_batch(
            state,
            keys,
            100,
            state.mailbox.participants.get(&id.value()).copied(),
        )?;
        for (_, record) in &state.mailbox.records {
            if record.manifest.conversation() == id {
                if let Some(doc) = self.retained_document(state, keys, record)? {
                    return doc.subject().cloned().ok_or(Error::InvalidData);
                }
            }
        }
        Err(Error::SourceUnavailable)
    }
    pub(super) fn mailbox_contacts(state: &State) -> std::collections::BTreeSet<ProtocolIdentity> {
        state.mailbox.participants.values().copied().collect()
    }
    pub(super) fn pending_send_other(state: &State) -> Option<ProtocolIdentity> {
        state.sending.as_ref().map(|pending| pending.other)
    }
}

pub(super) fn own_keys(
    profile: &Profile,
    keys: &Secrets,
) -> Result<(EndpointSecretKey, EndpointPublicKey), Error> {
    EndpointSecretKey::from_secret_bytes(profile.content_key, &keys.content)
        .map_err(|_| Error::KeyUnavailable)
}
pub(super) fn request_conversation(
    state: &State,
    other: ProtocolIdentity,
    request: RequestId,
) -> Option<ConversationId> {
    state.mailbox.records.iter().find_map(|(_, record)| {
        (record.author == state.profile.persona
            && matches!(record.admission, AdmissionBasis::InitialRequest { request: id } if id == request)
            && state.mailbox.participants.get(&record.manifest.conversation().value()) == Some(&other))
            .then_some(record.manifest.conversation())
    })
}

fn native_declarations(profile: &Profile) -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(profile.sender_key),
        },
        payload_schema: None,
    }
}
