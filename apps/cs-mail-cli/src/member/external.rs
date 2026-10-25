//! External commands compose locally, journal ciphertext, and render server decisions.
use super::{
    CONTENT_LIFETIME, ContentBinding, ContentRef, Decision, Deserialize, EndpointPublicKey,
    IdempotencyKey, Member, MessageId, MessageValidityUntil, Path, WireVersion, decode, identifier,
    json, now, print, random,
};
use cs_mail_application::correspondence::external::{
    Command, PreparedMessage, Response, SignedCommand,
};
use cs_mail_correspondence::{ConversationId, Document, DocumentPart};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{AccountId, ProtocolVersion};

#[derive(Deserialize)]
struct Preparation {
    relationship: cs_mail_application::members::RelationshipHandle,
    relay_key: EndpointPublicKey,
    member_key: EndpointPublicKey,
}
impl Member {
    pub fn external_grant_disclosure(
        &mut self,
        conversation: u128,
        operation: &str,
    ) -> Result<(), String> {
        let answer = self.journaled(
            &format!("grant-disclosure:{conversation}:{operation}"),
            "v1/external/commands",
            |member| {
                serde_json::to_value(
                    member.external_signed(conversation, Command::GrantDisclosure)?,
                )
                .map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&answer);
        Ok(())
    }
    fn external_signed(
        &self,
        conversation: u128,
        command: Command,
    ) -> Result<SignedCommand, String> {
        let deployment = self.deployment()?;
        SignedCommand {
            scope: deployment.scope,
            product: deployment.product,
            account: AccountId(self.account_id()?),
            persona: self.persona()?,
            key: self.identifiers()?.key,
            operation: IdempotencyKey(identifier()?),
            conversation: ConversationId::new(conversation).map_err(|e| e.to_string())?,
            command,
            signature: vec![],
        }
        .sign(&self.state().device_secret)
        .map_err(|e| e.to_string())
    }
    fn external_read_value(&self, conversation: u128, after: u64) -> Result<Response, String> {
        let signed = self.external_signed(conversation, Command::Read { after })?;
        decode(
            self.api
                .post("v1/external/commands", &signed)
                .map_err(|e| e.to_string())?,
            "external conversation",
        )
    }
    pub fn external_send(
        &mut self,
        conversation: u128,
        to: Option<&str>,
        message: &Path,
        disclose: bool,
        operation: &str,
    ) -> Result<(), String> {
        let step = format!("external-send:{conversation}:{operation}");
        let answer = self.journaled(
            &step,
            "v1/external/commands",
            |member| {
                let mailbox = match &to {
                    Some(address) => GuestMailbox::parse(address).map_err(|e| e.to_string())?,
                    None => match member.external_read_value(conversation, 0)? {
                        Response::Conversation { conversation, .. } => {
                            conversation.mailbox().clone()
                        }
                        Response::Outcome(_) => return Err("unexpected conversation".into()),
                    },
                };
                let preparation: Preparation = decode(
                    member
                        .api
                        .post(
                            "v1/external/prepare",
                            &json!({ "member": member.persona()?, "mailbox": mailbox }),
                        )
                        .map_err(|e| e.to_string())?,
                    "encryption information",
                )?;
                let text = std::fs::read_to_string(message)
                    .map_err(|e| format!("cannot read message: {e}"))?;
                if text.len() > 16384 {
                    return Err("external text must fit within 16 KiB".into());
                }
                let document = Document::new(vec![DocumentPart::Text(text)], random()?)
                    .map_err(|e| e.to_string())?;
                let id = MessageId(identifier()?);
                let conversation_id =
                    ConversationId::new(conversation).map_err(|e| e.to_string())?;
                let manifest = document
                    .manifest(id, conversation_id)
                    .map_err(|e| e.to_string())?;
                let (secret, sender_key) = member.content_key()?;
                let at = now();
                let expires = at
                    .checked_add(CONTENT_LIFETIME)
                    .ok_or("content expiry overflow")?;
                let binding = ContentBinding {
                    wire_version: WireVersion(1),
                    protocol_version: ProtocolVersion(2),
                    content_ref: ContentRef(identifier()?),
                    message_id: id,
                    sender: member.persona()?,
                    recipient: member.persona()?,
                    relationship: preparation.relationship.relationship,
                    content_scope: preparation.relationship.content_scope,
                    sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
                    declarations: member.declarations()?,
                    message_valid_until: MessageValidityUntil(expires),
                    capability: None,
                };
                let bytes = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
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
                    .map_err(|e| e.to_string())?,
                    relay_copy: cs_mail_content::encrypt(
                        &secret,
                        preparation.relay_key,
                        binding,
                        &bytes,
                        at,
                        expires,
                    )
                    .map_err(|e| e.to_string())?,
                });
                let command = if to.is_some() {
                    Command::Start {
                        receiving_alias: member.native_receiving_alias(member.persona()?)?,
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
                serde_json::to_value(member.external_signed(conversation, command)?)
                    .map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&answer);
        Ok(())
    }
    pub fn external_decision(&mut self, conversation: u128, withdraw: bool) -> Result<(), String> {
        let step = format!("external-decision:{conversation}:{withdraw}");
        let answer = self.journaled(
            &step,
            "v1/external/commands",
            |member| {
                serde_json::to_value(member.external_signed(
                    conversation,
                    if withdraw {
                        Command::WithdrawDisclosure
                    } else {
                        Command::Reject
                    },
                )?)
                .map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&answer);
        if withdraw {
            println!(
                "Waiting emails cancelled. Messages whose dispatch began may already have left; cancellation is too late to guarantee recall."
            );
        }
        Ok(())
    }
    pub fn external_read(&self, conversation: u128, after: u64) -> Result<(), String> {
        let response = self.external_read_value(conversation, after)?;
        let Response::Conversation {
            conversation,
            relationship,
            messages,
            next_after,
        } = response
        else {
            return Err("unexpected external response".into());
        };
        print(
            &json!({ "conversation": conversation, "relationship": relationship, "next_after": next_after }),
        );
        let (secret, _) = self.content_key()?;
        for view in messages {
            let bytes = cs_mail_content::decrypt(
                &secret,
                view.message.sender_key,
                &view.message.member_copy,
            )
            .map_err(|e| e.to_string())?;
            let document: Document = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if document
                .manifest(view.message.manifest.message(), conversation.id())
                .map_err(|e| e.to_string())?
                != view.message.manifest
            {
                return Err("message commitment mismatch".into());
            }
            print(
                &json!({ "sequence": view.sequence, "direction": view.message.direction, "delivery": view.delivery, "provider_events": view.provider_events, "document": document }),
            );
        }
        Ok(())
    }
}

impl Member {
    pub fn external_accept(
        &mut self,
        conversation: u128,
        lane_ms: Option<u64>,
    ) -> Result<(), String> {
        let Response::Conversation { conversation, .. } =
            self.external_read_value(conversation, 0)?
        else {
            return Err("unexpected conversation".into());
        };
        let from = conversation.guest().0;
        if let Some(lifetime) = lane_ms {
            let (handle, view) = self.recipient_view(from)?;
            self.grant_lane(from, handle, &view, lifetime, true)
        } else {
            self.decide(from, Decision::Accept)
        }
    }
}
