//! Bind authenticated text, routing, replay identity and ciphertext in one opaque
//! receiver value. Only this construction path can enter durable SMTP admission.
use super::{Refusal, VerifiedMail};
use cs_mail_adapters::VerifiedDomain;
use cs_mail_content::{
    ContentBinding, EncryptedContentRecord, EndpointPublicKey, EndpointSecretKey,
};
use cs_mail_correspondence::{ConversationId, Document, DocumentPart, MessageManifest};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentRef, DeclarationAuthority, DeclaredPurpose, Duration,
    LaneId, MessageDeclarations, MessageValidityUntil, OriginDeclaration, OriginMode, ProviderRef,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// Preparation is untrusted routing information; admission rechecks it under locks.
#[derive(Clone, Copy)]
pub struct ReplyEncryption {
    pub conversation: ConversationId,
    pub deployment: [u8; 32],
    pub provider: ProviderRef,
    pub member: cs_mail_primitives::ProtocolIdentity,
    pub relationship: cs_mail_primitives::RelationshipRef,
    pub content_scope: cs_mail_primitives::ContentScopeRef,
    pub member_key: EndpointPublicKey,
    pub lane: Option<LaneId>,
}

/// Immutable receiver proof. No constructor, mutable fields, or deserializer can
/// substitute content or routing after authentication and encryption.
pub struct AuthenticatedReply {
    destination: super::MailDestination,
    rfc_message_id: String,
    mailbox: GuestMailbox,
    conversation: ConversationId,
    manifest: MessageManifest,
    sender_key: EndpointPublicKey,
    member_copy: EncryptedContentRecord,
    arrival: super::ArrivalIdentity,
    delivery_digest: [u8; 32],
    authentication: VerifiedDomain,
    transport_digest: [u8; 32],
    raw_digest: [u8; 32],
}
impl AuthenticatedReply {
    pub const fn arrival(&self) -> super::ArrivalIdentity {
        self.arrival
    }
    pub fn destination(&self) -> super::MailDestination {
        self.destination.clone()
    }
    pub fn rfc_message_id(&self) -> &str {
        &self.rfc_message_id
    }
    pub fn mailbox(&self) -> &GuestMailbox {
        &self.mailbox
    }
    pub const fn conversation(&self) -> ConversationId {
        self.conversation
    }
    pub fn manifest(&self) -> &MessageManifest {
        &self.manifest
    }
    pub const fn sender_key(&self) -> EndpointPublicKey {
        self.sender_key
    }
    pub fn member_copy(&self) -> &EncryptedContentRecord {
        &self.member_copy
    }
    pub const fn delivery_digest(&self) -> [u8; 32] {
        self.delivery_digest
    }
    pub fn authentication(&self) -> &VerifiedDomain {
        &self.authentication
    }
    pub const fn transport_digest(&self) -> [u8; 32] {
        self.transport_digest
    }
    pub const fn raw_digest(&self) -> [u8; 32] {
        self.raw_digest
    }
}
pub(super) fn fingerprint(secret: &[u8; 32], domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("32-byte HMAC key");
    mac.update(domain);
    for part in parts {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}
impl VerifiedMail {
    /// Encrypts exactly the text and conversation authenticated by this receiver.
    /// # Errors
    /// Refuses invalid preparation, clock regression, or failed encryption.
    pub fn encrypt_reply(
        &self,
        p: ReplyEncryption,
        secret: &[u8; 32],
        now: CanonicalTime,
    ) -> Result<AuthenticatedReply, Refusal> {
        if secret == &[0; 32] || now < self.authentication.evaluated_at {
            return Err(Refusal::Temporary);
        }
        if p.conversation != self.conversation_id(&p.deployment, secret)
            || matches!(self.destination(), super::MailDestination::Member(member) if member != p.member)
        {
            return Err(Refusal::Permanent);
        }
        let (identity, delivery_digest) = self.fingerprints(&p.deployment, secret);
        let id = identity.message_id();
        let mut salt = [0; 32];
        getrandom::getrandom(&mut salt).map_err(|_| Refusal::Temporary)?;
        let document = Document::new(vec![DocumentPart::Text(self.text.clone())], salt)
            .map_err(|_| Refusal::Permanent)?;
        let manifest = document
            .manifest(id, p.conversation)
            .map_err(|_| Refusal::Permanent)?;
        let (sender_secret, sender_key) = EndpointSecretKey::generate(ContentKeyRef(id.0));
        let expires = now
            .checked_add(Duration(30 * 24 * 60 * 60 * 1000))
            .ok_or(Refusal::Temporary)?;
        let binding = ContentBinding {
            wire_version: cs_mail_primitives::WireVersion(1),
            protocol_version: cs_mail_primitives::ProtocolVersion(2),
            message_id: id,
            content_ref: ContentRef(id.0),
            sender: self.mailbox.synthetic_protocol_identity(&p.deployment),
            recipient: p.member,
            relationship: p.relationship,
            content_scope: p.content_scope,
            sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
            message_valid_until: MessageValidityUntil(expires),
            capability: p.lane,
            declarations: MessageDeclarations {
                purpose: DeclaredPurpose::Unspecified,
                origin: OriginDeclaration {
                    mode: OriginMode::LegacyOrUnspecified,
                    authority: DeclarationAuthority::GuestSmtp(p.provider),
                },
                payload_schema: None,
            },
        };
        let member_copy = cs_mail_content::encrypt(
            &sender_secret,
            p.member_key,
            binding,
            &serde_json::to_vec(&document).map_err(|_| Refusal::Temporary)?,
            now,
            expires,
        )
        .map_err(|_| Refusal::Temporary)?;
        Ok(AuthenticatedReply {
            destination: self.destination(),
            rfc_message_id: self.message_id.clone(),
            mailbox: self.mailbox.clone(),
            conversation: p.conversation,
            manifest,
            sender_key,
            member_copy,
            arrival: identity,
            delivery_digest,
            authentication: self.authentication.clone(),
            transport_digest: self.transport_digest,
            raw_digest: self.raw_digest,
        })
    }
}
