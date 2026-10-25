//! Native correspondence for the member application: mailbox metadata, the conversations
//! with one contact, and journaled sends. Plaintext is decrypted in memory, never cached.
use super::{Error, MemberClient, Profile, State, storage::Secrets};
use crate::correspondence::{EndpointKeys, compose, open, prepare, readable_text};
use cs_mail_application::correspondence::{
    Command, MessageRecord, MessageView, Response, SignedRequest,
};
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
const MESSAGE_LIFETIME: Duration = Duration(30 * 24 * 60 * 60 * 1000);
/// Largest text a member may send in one message.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

/// Mailbox metadata: headers and conversation participants, never content.
#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct Mailbox {
    after: u64,
    records: Vec<(u64, MessageRecord)>,
    /// The other participant of each conversation this address belongs to.
    participants: BTreeMap<u128, ProtocolIdentity>,
}
/// One exact send, journaled before transmission and resent unchanged until answered.
#[derive(Deserialize, Serialize)]
pub(super) struct PendingSend {
    other: ProtocolIdentity,
    create: Option<SignedRequest>,
    send: SignedRequest,
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
    RequestConversation,
    Closed,
}
/// One conversation with a contact, labelled from content decrypted on this device.
#[derive(Clone, Debug)]
pub struct ConversationSummary {
    pub id: String,
    /// The opening of its first readable message.
    pub label: String,
    /// The request this conversation began with, when it began with one.
    pub request: Option<String>,
    pub last_at: u64,
}
/// A decrypted message with its place in the correspondence.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub conversation: String,
    pub text: String,
    pub outgoing: bool,
    pub at: u64,
}
/// Everything shown for one contact's correspondence.
pub(super) struct Correspondence {
    pub threads: Vec<ConversationSummary>,
    pub items: Vec<Item>,
    pub reply: ReplyRoute,
    /// The conversation an answer to the contact's request goes in.
    pub request_conversation: Option<String>,
}

impl MemberClient {
    fn correspondence_call(
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
    fn signed(profile: &Profile, keys: &Secrets, command: Command) -> Result<SignedRequest, Error> {
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
                state.mailbox.records.push((sequence, record));
            }
            if next_after <= after {
                break;
            }
            state.mailbox.after = next_after;
        }
        Ok(())
    }
    fn endpoint_key(
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
    fn sending(
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
        let (secret, public) = own_keys(&state.profile, keys)?;
        let endpoint = EndpointKeys {
            owner: me,
            secret: &secret,
            public,
        };
        let mut other_key = None;
        let mut items = Vec::new();
        let mut threads: BTreeMap<u128, ConversationSummary> = BTreeMap::new();
        let mut request_conversation = None;
        for (_, record) in &state.mailbox.records {
            let conversation = record.manifest.conversation().value();
            if state.mailbox.participants.get(&conversation) != Some(&other) {
                continue;
            }
            let request = match record.admission {
                AdmissionBasis::InitialRequest { request } => Some(request),
                _ => None,
            };
            if record.author == other && request.is_some() {
                request_conversation = Some(conversation.to_string());
            }
            let message = record.manifest.message();
            let signed = Self::signed(&state.profile, keys, Command::Fetch { message })?;
            let text = match self.correspondence_call(&state.profile, &signed)? {
                Response::Message(view) => {
                    let view: MessageView = *view;
                    let author_key = if view.record.author == me {
                        public
                    } else if let Some(key) = other_key {
                        key
                    } else {
                        let key = self.endpoint_key(&state.profile, keys, other)?;
                        other_key = Some(key);
                        key
                    };
                    readable_text(
                        &open(&endpoint, &view, author_key).map_err(|_| Error::InvalidData)?,
                    )
                }
                Response::Unavailable | Response::AccessDenied => "(deleted or expired)".into(),
                _ => return Err(Error::InvalidData),
            };
            let thread = threads
                .entry(conversation)
                .or_insert_with(|| ConversationSummary {
                    id: conversation.to_string(),
                    label: opening(&text),
                    request: request.map(|r: RequestId| r.0.to_string()),
                    last_at: 0,
                });
            thread.last_at = thread.last_at.max(record.committed_at.0);
            items.push(Item {
                id: message.0.to_string(),
                conversation: conversation.to_string(),
                text,
                outgoing: record.author == me,
                at: record.committed_at.0,
            });
        }
        let mut threads: Vec<ConversationSummary> = threads.into_values().collect();
        threads.sort_by_key(|thread| std::cmp::Reverse(thread.last_at));
        Ok(Correspondence {
            threads,
            items,
            reply: ReplyRoute::Closed,
            request_conversation,
        })
    }
    /// How this address may write to `other` now, from the sending direction's view.
    pub(super) fn reply_route(
        &self,
        state: &State,
        keys: &Secrets,
        other: ProtocolIdentity,
        request_conversation: bool,
    ) -> Result<ReplyRoute, Error> {
        let (_, view) = self.sending(&state.profile, keys, other)?;
        let now = (self.clock)();
        Ok(match view {
            Some(view) if view.state == RelationshipState::Accepted => ReplyRoute::Standing,
            Some(view) if view.state == RelationshipState::Blocked => ReplyRoute::Closed,
            Some(view) if view.lane.as_ref().is_some_and(|lane| lane.is_current(now)) => {
                ReplyRoute::Lane
            }
            Some(view) if view.state == RelationshipState::Revoked => ReplyRoute::Closed,
            _ if request_conversation => ReplyRoute::RequestConversation,
            _ => ReplyRoute::Closed,
        })
    }
    /// Composes, journals and sends `text` to `other`: in `conversation`, or in a new one.
    pub(super) fn prepare_send(
        &self,
        state: &State,
        keys: &Secrets,
        other: ProtocolIdentity,
        conversation: Option<u128>,
        text: &str,
    ) -> Result<PendingSend, Error> {
        if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(Error::InvalidData);
        }
        let profile = &state.profile;
        let (handle, view) = self.sending(profile, keys, other)?;
        let now = (self.clock)();
        let (conversation, create) = if let Some(id) = conversation {
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
            let create = Self::signed(profile, keys, Command::Create { id, other })?;
            (id, Some(create))
        };
        // A lane is presented only when it is the authority: standing permission and
        // answers in the contact's request conversation need none.
        let answering = create.is_none()
            && state.mailbox.records.iter().any(|(_, record)| {
                record.manifest.conversation() == conversation
                    && record.author == other
                    && matches!(record.admission, AdmissionBasis::InitialRequest { .. })
            });
        let capability = match &view {
            Some(view) if view.state == RelationshipState::Accepted || answering => None,
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
        let document =
            compose(vec![DocumentPart::Text(text.to_owned())]).map_err(|_| Error::InvalidData)?;
        let package = prepare(
            &EndpointKeys {
                owner: profile.persona,
                secret: &secret,
                public,
            },
            &document,
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
                declarations: MessageDeclarations {
                    purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
                    origin: OriginDeclaration {
                        mode: OriginMode::HumanInitiated,
                        authority: DeclarationAuthority::NativeSender(profile.sender_key),
                    },
                    payload_schema: None,
                },
                message_valid_until: MessageValidityUntil(expires),
                capability,
            },
            (now, expires),
        )
        .map_err(|_| Error::InvalidData)?;
        Ok(PendingSend {
            other,
            create,
            send: Self::signed(profile, keys, Command::Send(Box::new(package)))?,
        })
    }
    /// Sends the journaled operation's exact bytes. A definite refusal resolves it.
    pub(super) fn transmit_send(&self, state: &mut State, keys: &Secrets) -> Result<(), Error> {
        let pending = state.sending.as_ref().ok_or(Error::InvalidData)?;
        let result = (|| {
            if let Some(create) = &pending.create {
                let Command::Create { id, .. } = create.command else {
                    return Err(Error::InvalidData);
                };
                match self.correspondence_call(&state.profile, create)? {
                    Response::Conversation(conversation) if conversation.id() == id => {}
                    _ => return Err(Error::InvalidData),
                }
            }
            let Command::Send(package) = &pending.send.command else {
                return Err(Error::InvalidData);
            };
            match self.correspondence_call(&state.profile, &pending.send)? {
                Response::Committed(id) if id == package.manifest.message() => Ok(()),
                _ => Err(Error::InvalidData),
            }
        })();
        match result {
            Ok(()) | Err(Error::Refused) => {
                state.sending = None;
                super::storage::save(self.store.as_ref(), keys, state)?;
                result.map_err(|_| Error::SendRefused)
            }
            Err(error) => Err(error),
        }
    }
    pub(super) fn pending_send_other(state: &State) -> Option<ProtocolIdentity> {
        state.sending.as_ref().map(|pending| pending.other)
    }
}

fn own_keys(
    profile: &Profile,
    keys: &Secrets,
) -> Result<(EndpointSecretKey, EndpointPublicKey), Error> {
    EndpointSecretKey::from_secret_bytes(profile.content_key, &keys.content)
        .map_err(|_| Error::KeyUnavailable)
}
fn opening(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut label: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        label.push('…');
    }
    label
}
