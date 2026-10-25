//! Parse a bounded email and bind automatic authentication to its single author.
//! No client-supplied Authentication-Results header is trusted.
use cs_mail_adapters::{
    DmarcErrorKind, DmarcVerifier, DomainIdentity, SmtpAuthenticationRequest, VerifiedDomain,
};
use cs_mail_correspondence::ConversationId;
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::ProtocolIdentity;
use mail_parser::{Address, HeaderName, MessageParser};
use sha2::{Digest, Sha256};

mod encrypted;
mod mime;
pub use encrypted::{AuthenticatedReply, ReplyEncryption};

pub const MAX_EMAIL_BYTES: usize = cs_mail_correspondence::email::MAX_EMAIL_BYTES;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    Permanent,
    Temporary,
    TooLarge,
    Unsupported,
}

/// Routing is not authority: the author and current permission are checked later.
/// # Errors
/// Only member or conversation addresses on this receiver's domain are recognized.
pub fn destination_address(
    address: &str,
    domain: &DomainIdentity,
) -> Result<MailDestination, Refusal> {
    let (local, host) = address.rsplit_once('@').ok_or(Refusal::Permanent)?;
    if !host.eq_ignore_ascii_case(domain.as_str()) {
        return Err(Refusal::Permanent);
    }
    let (id, is_member) = if let Some(id) = local.strip_prefix("reply+") {
        (id, false)
    } else if let Some(id) = local.strip_prefix("member-") {
        (id, true)
    } else {
        return cs_mail_accounts::directory::MemberAddress::try_from(address.to_owned())
            .map(MailDestination::Address)
            .map_err(|_| Refusal::Permanent);
    };
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Refusal::Permanent);
    }
    let value = u128::from_str_radix(id, 16).map_err(|_| Refusal::Permanent)?;
    if value == 0 {
        return Err(Refusal::Permanent);
    }
    if is_member {
        Ok(MailDestination::Member(ProtocolIdentity(value)))
    } else {
        ConversationId::new(value)
            .map(MailDestination::Conversation)
            .map_err(|_| Refusal::Permanent)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MailDestination {
    Address(cs_mail_accounts::directory::MemberAddress),
    Conversation(ConversationId),
    Member(ProtocolIdentity),
}
pub fn reply_address(id: ConversationId, domain: &DomainIdentity) -> String {
    format!("reply+{:032x}@{}", id.value(), domain.as_str())
}

/// Keyed identity of one addressed SMTP arrival, shared by admission and prompts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrivalIdentity([u8; 32]);
impl ArrivalIdentity {
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
    pub fn message_id(self) -> cs_mail_primitives::MessageId {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&self.0[..16]);
        cs_mail_primitives::MessageId(u128::from_be_bytes(bytes) | 1)
    }
}

/// Constructed only after the adapter has checked MIME shape, author and DMARC.
/// Contains plaintext briefly in memory and deliberately has no Debug or serde.
pub struct VerifiedMail {
    mailbox: GuestMailbox,
    destination: MailDestination,
    may_notify: bool,
    message_id: String,
    text: String,
    content: cs_mail_correspondence::email::EmailContent,
    authentication: VerifiedDomain,
    transport_digest: [u8; 32],
    raw_digest: [u8; 32],
}
impl VerifiedMail {
    pub fn mailbox(&self) -> &GuestMailbox {
        &self.mailbox
    }
    pub fn destination(&self) -> MailDestination {
        self.destination.clone()
    }
    pub const fn may_notify(&self) -> bool {
        self.may_notify
    }
    /// Stable keyed identities for route+author+RFC identifier and delivered text.
    pub fn fingerprints(
        &self,
        deployment: &[u8; 32],
        secret: &[u8; 32],
    ) -> (ArrivalIdentity, [u8; 32]) {
        let route = match &self.destination {
            MailDestination::Conversation(id) => {
                let mut r = vec![1];
                r.extend_from_slice(&id.value().to_be_bytes());
                r
            }
            MailDestination::Member(id) => {
                let mut r = vec![2];
                r.extend_from_slice(&id.0.to_be_bytes());
                r
            }
            MailDestination::Address(address) => {
                let mut r = vec![3];
                r.extend_from_slice(address.as_str().as_bytes());
                r
            }
        };
        let identity = encrypted::fingerprint(
            secret,
            b"cs-mail/smtp-arrival/v2",
            &[
                deployment,
                &route,
                self.mailbox.address().as_bytes(),
                self.message_id.as_bytes(),
            ],
        );
        let content = encrypted::fingerprint(
            secret,
            b"cs-mail/smtp-content/v2",
            &[&identity, &self.content.delivery_commitment()],
        );
        (ArrivalIdentity(identity), content)
    }
    #[allow(clippy::missing_panics_doc)] // Setting the low bit guarantees the constructor accepts it.
    pub fn conversation_id(&self, deployment: &[u8; 32], secret: &[u8; 32]) -> ConversationId {
        if let MailDestination::Conversation(id) = self.destination {
            return id;
        }
        let identity = self.fingerprints(deployment, secret).0;
        let hash = encrypted::fingerprint(
            secret,
            b"cs-mail/incoming-conversation/v1",
            &[&identity.bytes()],
        );
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        ConversationId::new(u128::from_be_bytes(bytes) | 1).expect("nonzero derived identity")
    }
    pub fn message_id(&self) -> &str {
        &self.message_id
    }
    pub fn text(&self) -> &str {
        &self.text
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
/// # Errors
/// Refuses ambiguous authors/IDs, unsupported MIME, failed authentication and DNS failures.
pub async fn authenticate(
    verifier: &impl DmarcVerifier,
    request: SmtpAuthenticationRequest<'_>,
    recipient: &str,
    domain: &DomainIdentity,
) -> Result<VerifiedMail, Refusal> {
    if request.raw_message.len() > MAX_EMAIL_BYTES {
        return Err(Refusal::TooLarge);
    }
    let destination = destination_address(recipient, domain)?;
    let message = MessageParser::default()
        .parse(request.raw_message)
        .ok_or(Refusal::Permanent)?;
    for header in [HeaderName::From, HeaderName::MessageId] {
        if message.header_values(header).count() != 1 {
            return Err(Refusal::Permanent);
        }
    }
    for header in [
        HeaderName::Subject,
        HeaderName::ContentType,
        HeaderName::ContentTransferEncoding,
    ] {
        if message.header_values(header).count() > 1 {
            return Err(Refusal::Permanent);
        }
    }
    let Some(Address::List(authors)) = message.from() else {
        return Err(Refusal::Permanent);
    };
    if authors.len() != 1 {
        return Err(Refusal::Permanent);
    }
    let mailbox = GuestMailbox::parse(authors[0].address().ok_or(Refusal::Permanent)?)
        .map_err(|_| Refusal::Permanent)?;
    let id = message.message_id().ok_or(Refusal::Permanent)?;
    if id.len() > 998 || !id.contains('@') || id.bytes().any(|b| b <= 32 || b >= 127) {
        return Err(Refusal::Permanent);
    }
    let content = mime::content(&message)?;
    let text = content
        .presentation()
        .map_err(|_| Refusal::Unsupported)?
        .text;
    let verification = verifier.verify(request).await.map_err(|e| match e.kind {
        DmarcErrorKind::Temporary => Refusal::Temporary,
        _ => Refusal::Permanent,
    })?;
    if verification.evaluated_at != request.received_at
        || verification.author_domain != *mailbox.domain()
        || (verification.dkim.is_none() && verification.spf.is_none())
    {
        return Err(Refusal::Permanent);
    }
    let transport = serde_json::to_vec(&(
        request.remote_ip,
        request.helo_identity,
        request.mail_from,
        recipient,
        request.receiver_hostname,
        request.received_at,
    ))
    .map_err(|_| Refusal::Temporary)?;
    Ok(VerifiedMail {
        mailbox,
        destination,
        may_notify: request.mail_from.is_some()
            && message
                .header_values("Auto-Submitted")
                .all(|v| v.as_text().is_some_and(|s| s.eq_ignore_ascii_case("no"))),
        message_id: id.to_owned(),
        text,
        content,
        authentication: verification,
        transport_digest: Sha256::digest(transport).into(),
        raw_digest: Sha256::digest(request.raw_message).into(),
    })
}
