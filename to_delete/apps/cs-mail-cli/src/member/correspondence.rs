//! Native correspondence: the mailbox, request conversations and journaled sends.
use super::{
    CONTENT_LIFETIME, ContentBinding, ContentRef, EndpointKeyPublication, IdempotencyKey, Member,
    MessageId, MessageValidityUntil, ProtocolIdentity, RequestId, WireVersion, decode, identifier,
    json, now, print,
};
use cs_mail_application::correspondence::{
    Command, MessageRecord, MessageView, Response, SignedRequest,
};
use cs_mail_client::correspondence::{EndpointKeys, compose, open, prepare, readable_text};
use cs_mail_content::{ContentCertificateDigest, EndpointPublicKey};
use cs_mail_correspondence::{ConversationId, DocumentPart};
use cs_mail_primitives::AccountId;
use cs_mail_protocol::{AdmissionBasis, RelationshipState};

/// Largest mailbox page the service returns.
const PAGE: u16 = 100;

impl Member {
    fn correspondence_signed(&self, command: Command) -> Result<SignedRequest, String> {
        let deployment = self.deployment()?;
        let ids = self.identifiers()?;
        SignedRequest {
            product: deployment.product,
            account: AccountId(self.account_id()?),
            persona: ids.persona,
            key: ids.key,
            operation: IdempotencyKey(identifier()?),
            command,
            signature: Vec::new(),
        }
        .sign(&self.state().device_secret)
        .map_err(|e| format!("cannot sign: {e}"))
    }
    fn correspondence(&self, command: Command) -> Result<Response, String> {
        let signed = self.correspondence_signed(command)?;
        decode(
            self.api
                .post("v1/correspondence/commands", &signed)
                .map_err(|e| e.to_string())?,
            "correspondence answer",
        )
    }
    /// Every message currently in this address's mailbox, in mailbox order.
    fn mailbox_records(&self) -> Result<Vec<MessageRecord>, String> {
        let mut after = 0;
        let mut records = Vec::new();
        loop {
            match self.correspondence(Command::Mailbox { after, limit: PAGE })? {
                Response::Mailbox {
                    entries,
                    next_after,
                } => {
                    records.extend(entries.into_iter().map(|(_, record)| record));
                    if next_after == after {
                        return Ok(records);
                    }
                    after = next_after;
                }
                other => return Err(format!("unexpected mailbox answer: {other:?}")),
            }
        }
    }
    /// The conversation a request began, from this address's own mailbox.
    pub(super) fn request_conversation(
        &self,
        request: RequestId,
    ) -> Result<ConversationId, String> {
        self.mailbox_records()?
            .into_iter()
            .find(|record| {
                matches!(record.admission, AdmissionBasis::InitialRequest { request: r } if r == request)
            })
            .map(|record| record.manifest.conversation())
            .ok_or_else(|| "the request's conversation is not in this mailbox".to_owned())
    }
    fn endpoint_key(&self, persona: ProtocolIdentity) -> Result<EndpointPublicKey, String> {
        if persona == self.persona()? {
            return Ok(self.content_key()?.1);
        }
        let publication: EndpointKeyPublication = decode(
            self.api
                .get(&format!("v1/addresses/{}/endpoint-key", persona.0))
                .map_err(|e| format!("content key of {}: {e}", persona.0))?,
            "endpoint key",
        )?;
        Ok(publication.key)
    }
    /// Every document `author` wrote to this address, decrypted, in mailbox order.
    pub(super) fn received_documents(
        &self,
        author: ProtocolIdentity,
    ) -> Result<Vec<(MessageId, String)>, String> {
        let (secret, public) = self.content_key()?;
        let keys = EndpointKeys {
            owner: self.persona()?,
            secret: &secret,
            public,
        };
        let mut documents = Vec::new();
        for record in self.mailbox_records()? {
            if record.author != author {
                continue;
            }
            let message = record.manifest.message();
            let text = match self.correspondence(Command::Fetch { message })? {
                Response::Message(view) => open(&keys, &view, self.endpoint_key(author)?)
                    .map_or_else(
                        |e| format!("(cannot decrypt: {e})"),
                        |document| readable_text(&document),
                    ),
                _ => "(deleted or expired)".to_owned(),
            };
            documents.push((message, text));
        }
        Ok(documents)
    }
    /// Prints every readable message, grouped by conversation in mailbox order.
    pub fn mailbox(&self) -> Result<(), String> {
        let persona = self.persona()?;
        let (secret, public) = self.content_key()?;
        let keys = EndpointKeys {
            owner: persona,
            secret: &secret,
            public,
        };
        let mut rows = Vec::new();
        for record in self.mailbox_records()? {
            let message = record.manifest.message();
            let text = match self.correspondence(Command::Fetch { message })? {
                Response::Message(view) => {
                    let view: MessageView = *view;
                    let author_key = self.endpoint_key(view.record.author)?;
                    open(&keys, &view, author_key).map_or_else(
                        |e| format!("[unreadable: {e}]"),
                        |document| readable_text(&document),
                    )
                }
                Response::Unavailable => "[deleted or expired]".to_owned(),
                other => format!("[{other:?}]"),
            };
            rows.push(json!({
                "conversation": record.manifest.conversation(),
                "message": message,
                "author": record.author,
                "admission": record.admission,
                "committed_at": record.committed_at,
                "text": text,
            }));
        }
        print(&json!({ "messages": rows }));
        Ok(())
    }
    /// Sends `message` to `to`: in `conversation` when given, otherwise in a new one.
    /// Every step is journaled under `operation`, so a retry sends the same bytes.
    pub fn send(
        &mut self,
        to: u128,
        conversation: Option<u128>,
        message: &str,
        operation: &str,
    ) -> Result<(), String> {
        let recipient = ProtocolIdentity(to);
        let conversation = if let Some(id) = conversation {
            ConversationId::new(id).map_err(|e| e.to_string())?
        } else {
            let step = format!("conversation:{to}:{operation}");
            let answer = self.journaled(
                &step,
                "v1/correspondence/commands",
                |member| {
                    let id = ConversationId::new(identifier()?).map_err(|e| e.to_string())?;
                    serde_json::to_value(member.correspondence_signed(Command::Create {
                        id,
                        other: recipient,
                    })?)
                    .map_err(|e| e.to_string())
                },
                false,
            )?;
            let response: Response = decode(answer, "conversation")?;
            match response {
                Response::Conversation(conversation) => conversation.id(),
                other => return Err(format!("conversation refused: {other:?}")),
            }
        };
        let (handle, view) = self.sending(to)?;
        // A lane is presented only when it is the authority: standing permission and
        // answers in a request's own conversation need none.
        let request_reply = self.mailbox_records()?.into_iter().any(|record| {
            record.manifest.conversation() == conversation
                && record.author == recipient
                && matches!(record.admission, AdmissionBasis::InitialRequest { .. })
        });
        let capability = match &view {
            Some(view) if view.state == RelationshipState::Accepted || request_reply => None,
            Some(view) => view.lane.as_ref().map(|lane| lane.grant().id),
            None => None,
        };
        let step = format!("send:{to}:{operation}");
        let answer = self.journaled(
            &step,
            "v1/correspondence/commands",
            |member| {
                let deployment = member.deployment()?;
                let persona = member.persona()?;
                let (secret, public) = member.content_key()?;
                let created = now();
                let valid_until = MessageValidityUntil(
                    created
                        .checked_add(CONTENT_LIFETIME)
                        .ok_or("clock overflow")?,
                );
                let document = compose(vec![DocumentPart::Text(message.to_owned())])
                    .map_err(|e| format!("cannot compose: {e}"))?;
                let package = prepare(
                    &EndpointKeys {
                        owner: persona,
                        secret: &secret,
                        public,
                    },
                    &document,
                    conversation,
                    member.endpoint_key(recipient)?,
                    deployment.custody_key,
                    ContentBinding {
                        wire_version: WireVersion(1),
                        content_ref: ContentRef(identifier()?),
                        message_id: MessageId(identifier()?),
                        sender: persona,
                        recipient,
                        protocol_version: deployment.policy.protocol_version,
                        relationship: handle.relationship,
                        content_scope: handle.content_scope,
                        // Native sends are authenticated by the signed command; no
                        // relationship content certificate is bound.
                        sender_certificate: ContentCertificateDigest([0; 32]),
                        declarations: member.declarations()?,
                        message_valid_until: valid_until,
                        capability,
                    },
                    (created, valid_until.0),
                )
                .map_err(|e| format!("cannot encrypt: {e}"))?;
                serde_json::to_value(
                    member.correspondence_signed(Command::Send(Box::new(package)))?,
                )
                .map_err(|e| e.to_string())
            },
            false,
        )?;
        let response: Response = decode(answer, "send")?;
        match response {
            Response::Committed(id) => {
                print(&json!({ "sent": id, "conversation": conversation }));
                Ok(())
            }
            other => Err(format!("send refused: {other:?}")),
        }
    }
}
