//! Native CLI commands use the shared endpoint composer and journal only signed ciphertext.
use super::{
    CONTENT_LIFETIME, ContentBinding, ContentRef, IdempotencyKey, Member, MessageId,
    MessageValidityUntil, ProtocolIdentity, WireVersion, decode, identifier, json, now,
};
use cs_mail_application::correspondence::{Command, Response, SignedRequest};
use cs_mail_client::correspondence::{EndpointKeys, compose, open, prepare, readable_text};
use cs_mail_correspondence::{ConversationId, DocumentPart};
use cs_mail_primitives::AccountId;

const ROUTE: &str = "v1/correspondence/commands";
impl Member {
    pub(super) fn native_receiving_alias(
        &self,
        owner: cs_mail_primitives::ProtocolIdentity,
    ) -> Result<cs_mail_primitives::AliasId, String> {
        let published: cs_mail_application::aliases::PublicReceivingAlias = serde_json::from_value(
            self.api
                .get(&format!("v1/addresses/{}/receiving-alias", owner.0))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if published.owner != owner {
            return Err("receiving owner mismatch".into());
        }
        Ok(published.id)
    }

    fn native_signed(&self, command: Command) -> Result<SignedRequest, String> {
        let deployment = self.deployment()?;
        SignedRequest {
            product: deployment.product,
            account: AccountId(self.account_id()?),
            persona: self.persona()?,
            key: self.identifiers()?.key,
            operation: IdempotencyKey(identifier()?),
            command,
            signature: vec![],
        }
        .sign(&self.state().device_secret)
        .map_err(|e| e.to_string())
    }
    fn native_call(&self, command: Command) -> Result<Response, String> {
        decode(
            self.api
                .post(ROUTE, &self.native_signed(command)?)
                .map_err(|e| e.to_string())?,
            "native correspondence",
        )
    }
    pub fn native_send(
        &mut self,
        to: u128,
        conversation: Option<u128>,
        text: &str,
        operation: &str,
    ) -> Result<(), String> {
        if text.trim().is_empty() || text.len() > 64 * 1024 {
            return Err("message must contain 1–65536 bytes".into());
        }
        let step = format!("native:{to}:{operation}");
        let id = if let Some(id) = conversation {
            id
        } else {
            let name = format!("{step}:conversation");
            if !self.state().values.contains_key(&name) {
                self.store
                    .state
                    .values
                    .insert(name.clone(), json!(identifier()?.to_string()));
                self.store.save()?;
            }
            self.state().values[&name]
                .as_str()
                .ok_or("invalid conversation plan")?
                .parse()
                .map_err(|_| "invalid conversation identifier")?
        };
        let id = ConversationId::new(id).map_err(|e| e.to_string())?;
        let packet_key = format!("{step}:prepared-packet");
        if !self.state().values.contains_key(&packet_key) {
            let packet = self.native_packet(to, id, text)?;
            self.store.state.values.insert(packet_key.clone(), packet);
            self.store.save()?;
        }
        let packet = self.state().values[&packet_key].clone();
        let prepared: SignedRequest =
            serde_json::from_value(packet.clone()).map_err(|e| e.to_string())?;
        let Command::Send {
            package: ref message,
            ..
        } = prepared.command
        else {
            return Err("invalid prepared send".into());
        };
        let subject = message.manifest.subject().ok_or("missing shared subject")?;
        if conversation.is_none() {
            let answer = self.journaled(
                &format!("{step}:create"),
                ROUTE,
                |member| {
                    serde_json::to_value(member.native_signed(Command::Create {
                        subject,
                        id,
                        other: ProtocolIdentity(to),
                    })?)
                    .map_err(|e| e.to_string())
                },
                false,
            )?;
            if !matches!(decode::<Response>(answer,"conversation")?,Response::Conversation(found) if found.id()==id)
            {
                return Err("conversation creation was not confirmed".into());
            }
        }
        let answer = self.journaled(
            &format!("{step}:send"),
            ROUTE,
            |_| Ok(packet.clone()),
            false,
        )?;
        match decode::<Response>(answer, "send")? {
            Response::Committed(message) => {
                println!(
                    "message {} delivered in conversation {}",
                    message.0,
                    id.value()
                );
                Ok(())
            }
            _ => Err("message delivery was not confirmed".into()),
        }
    }
    pub(super) fn native_subject(
        &self,
        id: ConversationId,
    ) -> Result<cs_mail_correspondence::ConversationSubject, String> {
        let (secret, public) = self.content_key()?;
        let keys = EndpointKeys {
            owner: self.persona()?,
            secret: &secret,
            public,
        };
        let mut after = 0;
        loop {
            let Response::Mailbox {
                entries,
                next_after,
            } = self.native_call(Command::Mailbox { after, limit: 100 })?
            else {
                return Err("invalid mailbox".into());
            };
            for (_, record) in entries {
                if record.manifest.conversation() != id {
                    continue;
                }
                if let Response::Message(view) = self.native_call(Command::Fetch {
                    message: record.manifest.message(),
                })? {
                    return open(&keys, &view, record.sender_key)
                        .map_err(|e| e.to_string())?
                        .subject()
                        .cloned()
                        .ok_or("missing shared subject".into());
                }
            }
            if next_after <= after {
                break;
            }
            after = next_after;
        }
        let mut entropy = [0; 32];
        entropy[..16].copy_from_slice(&identifier()?.to_be_bytes());
        entropy[16..].copy_from_slice(&identifier()?.to_be_bytes());
        cs_mail_correspondence::ConversationSubject::new("", now(), 0, entropy)
            .map_err(|e| e.to_string())
    }
    fn native_packet(
        &self,
        to: u128,
        conversation: ConversationId,
        text: &str,
    ) -> Result<serde_json::Value, String> {
        let deployment = self.deployment()?;
        let (handle, view) = self.sending(to)?;
        let capability = view.as_ref().and_then(|view| {
            if view.state == cs_mail_protocol::RelationshipState::Accepted {
                None
            } else {
                view.lane
                    .as_ref()
                    .filter(|lane| lane.is_current(now()))
                    .map(|lane| lane.grant().id)
            }
        });
        let recipient = self
            .api
            .get(&format!("v1/addresses/{to}/endpoint-key"))
            .map_err(|e| e.to_string())?;
        let recipient: cs_mail_security::EndpointKeyPublication =
            decode(recipient, "endpoint key")?;
        if recipient.persona != ProtocolIdentity(to) {
            return Err("foreign endpoint key".into());
        }
        let (secret, public) = self.content_key()?;
        let document = compose(vec![DocumentPart::Text(text.to_owned())])
            .map_err(|e| e.to_string())?
            .with_subject(self.native_subject(conversation)?);
        let at = now();
        let until = at
            .checked_add(CONTENT_LIFETIME)
            .ok_or("message expiry overflow")?;
        let package = prepare(
            &EndpointKeys {
                owner: self.persona()?,
                secret: &secret,
                public,
            },
            &document,
            conversation,
            recipient.key,
            deployment.custody_key,
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(identifier()?),
                message_id: MessageId(identifier()?),
                sender: self.persona()?,
                recipient: ProtocolIdentity(to),
                protocol_version: deployment.policy.protocol_version,
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
                declarations: self.declarations()?,
                message_valid_until: MessageValidityUntil(until),
                capability,
            },
            (at, until),
        )
        .map_err(|e| e.to_string())?;
        serde_json::to_value(self.native_signed(Command::Send {
            alias: self.native_receiving_alias(cs_mail_primitives::ProtocolIdentity(to))?,
            package: Box::new(package),
        })?)
        .map_err(|e| e.to_string())
    }
    pub fn native_read(&self, other: u128) -> Result<(), String> {
        let mut after = 0;
        let me = self.persona()?;
        let (secret, public) = self.content_key()?;
        loop {
            let Response::Mailbox {
                entries,
                next_after,
            } = self.native_call(Command::Mailbox { after, limit: 100 })?
            else {
                return Err("unexpected mailbox answer".into());
            };
            for (_, record) in entries {
                let id = record.manifest.conversation();
                let Response::Conversation(conversation) =
                    self.native_call(Command::Policy { id })?
                else {
                    continue;
                };
                if conversation.scope().other(me).map_err(|e| e.to_string())?
                    != ProtocolIdentity(other)
                {
                    continue;
                }
                let text = match self.native_call(Command::Fetch {
                    message: record.manifest.message(),
                })? {
                    Response::Message(view) => readable_text(
                        &open(
                            &EndpointKeys {
                                owner: me,
                                secret: &secret,
                                public,
                            },
                            &view,
                            view.record.sender_key,
                        )
                        .map_err(|e| e.to_string())?,
                    ),
                    Response::Unavailable | Response::AccessDenied => "(deleted or expired)".into(),
                    _ => return Err("unexpected message answer".into()),
                };
                println!(
                    "conversation {} · {} · {}: {}",
                    id.value(),
                    record.committed_at.0,
                    if record.author == me {
                        "You"
                    } else {
                        "Contact"
                    },
                    text
                );
            }
            if next_after <= after {
                break;
            }
            after = next_after;
        }
        Ok(())
    }
}
