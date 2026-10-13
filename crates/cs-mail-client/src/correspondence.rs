//! Headless endpoint composition. Only the explicit document is serialized;
//! a caller's private workspace and locally opened sources never enter this API.
use crate::NativeClient;
use cs_mail_application::consent::{RecoveryCopy, ReleasedCopy};
use cs_mail_application::correspondence::{MessageView, PreparedMessage};
use cs_mail_content::{ContentBinding, EndpointPublicKey};
use cs_mail_correspondence::{ConversationId, CorrespondenceError, Document, DocumentPart};
use cs_mail_primitives::CanonicalTime;
use cs_mail_protocol::ActorRef;

pub struct EndpointKeys<'a> {
    pub owner: cs_mail_primitives::ProtocolIdentity,
    pub secret: &'a cs_mail_content::EndpointSecretKey,
    pub public: EndpointPublicKey,
}
impl NativeClient {
    /// # Errors
    /// Requires a member communication identity.
    pub fn endpoint_keys(&self) -> Result<EndpointKeys<'_>, CorrespondenceError> {
        let (ActorRef::Sender(owner) | ActorRef::Recipient(owner)) = self.actor else {
            return Err(CorrespondenceError::Unauthorized);
        };
        Ok(EndpointKeys {
            owner,
            secret: &self.content_secret,
            public: self.content_public,
        })
    }
}
/// Builds an explicit outgoing document with a fresh private version secret.
/// Keep this document and its prepared request stable across retries.
/// # Errors
/// Rejects invalid document content or an unavailable operating-system RNG.
pub fn compose(parts: Vec<DocumentPart>) -> Result<Document, CorrespondenceError> {
    let mut secret = [0; 32];
    getrandom::fill(&mut secret).map_err(|_| CorrespondenceError::Unavailable)?;
    Document::new(parts, secret)
}

/// # Errors
/// Rejects a foreign author, self-delivery, invalid content or encryption failure.
/// Recipient keys must come from the host's authenticated key-discovery path.
pub fn prepare(
    keys: &EndpointKeys<'_>,
    document: &Document,
    conversation: ConversationId,
    recipient: EndpointPublicKey,
    custodian: EndpointPublicKey,
    binding: ContentBinding,
    lifetime: (CanonicalTime, CanonicalTime),
) -> Result<PreparedMessage, CorrespondenceError> {
    let author = keys.owner;
    if binding.sender != author || binding.recipient == author {
        return Err(CorrespondenceError::Invalid);
    }
    let manifest = document.manifest(binding.message_id, conversation)?;
    let plaintext = zeroize::Zeroizing::new(
        serde_json::to_vec(document).map_err(|_| CorrespondenceError::Invalid)?,
    );
    let mut own_binding = binding.clone();
    own_binding.recipient = author;
    let sender_copy = cs_mail_content::encrypt(
        keys.secret,
        keys.public,
        own_binding,
        &plaintext,
        lifetime.0,
        lifetime.1,
    )
    .map_err(|_| CorrespondenceError::Invalid)?;
    let recipient_copy = cs_mail_content::encrypt(
        keys.secret,
        recipient,
        binding.clone(),
        &plaintext,
        lifetime.0,
        lifetime.1,
    )
    .map_err(|_| CorrespondenceError::Invalid)?;
    let sender_recovery = RecoveryCopy {
        sender: keys.public,
        ciphertext: cs_mail_content::encrypt(
            keys.secret,
            custodian,
            sender_copy.binding.clone(),
            &plaintext,
            lifetime.0,
            lifetime.1,
        )
        .map_err(|_| CorrespondenceError::Invalid)?,
    };
    let recipient_recovery = RecoveryCopy {
        sender: keys.public,
        ciphertext: cs_mail_content::encrypt(
            keys.secret,
            custodian,
            binding,
            &plaintext,
            lifetime.0,
            lifetime.1,
        )
        .map_err(|_| CorrespondenceError::Invalid)?,
    };
    Ok(PreparedMessage {
        manifest,
        sender_copy,
        recipient_copy,
        sender_recovery,
        recipient_recovery,
    })
}

/// # Errors
/// Rejects substituted source metadata, plaintext, author or version. The supplied
/// sender key must be pinned/authenticated by the endpoint's key-discovery path.
pub fn open(
    keys: &EndpointKeys<'_>,
    view: &MessageView,
    sender: EndpointPublicKey,
) -> Result<Document, CorrespondenceError> {
    let owner = keys.owner;
    let binding = &view.ciphertext.binding;
    if binding.sender != view.record.author
        || binding.recipient != owner
        || binding.message_id != view.record.manifest.message()
    {
        return Err(CorrespondenceError::Invalid);
    }
    let plaintext = zeroize::Zeroizing::new(
        cs_mail_content::decrypt(keys.secret, sender, &view.ciphertext)
            .map_err(|_| CorrespondenceError::Invalid)?,
    );
    let document: Document =
        serde_json::from_slice(&plaintext).map_err(|_| CorrespondenceError::Invalid)?;
    if document.manifest(binding.message_id, view.record.manifest.conversation())?
        != view.record.manifest
    {
        return Err(CorrespondenceError::Invalid);
    }
    Ok(document)
}

impl NativeClient {
    /// Opens only a sealed, scoped response from the authenticated custody service.
    /// # Errors
    /// Rejects another device's response, substituted custody key or document manifest.
    pub fn open_released(
        &self,
        released: &ReleasedCopy,
        custodian: EndpointPublicKey,
    ) -> Result<Document, CorrespondenceError> {
        if released.custodian != custodian
            || released.ciphertext.binding.message_id != released.manifest.message()
        {
            return Err(CorrespondenceError::Invalid);
        }
        let plaintext = self
            .decrypt(custodian, &released.ciphertext)
            .map_err(|_| CorrespondenceError::Invalid)?;
        let document: Document =
            serde_json::from_slice(&plaintext).map_err(|_| CorrespondenceError::Invalid)?;
        if document.manifest(
            released.manifest.message(),
            released.manifest.conversation(),
        )? != released.manifest
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(document)
    }
}

/// Local presentation only; references never fetch another person's content.
pub fn readable_text(document: &Document) -> String {
    use cs_mail_correspondence::{ContentBlock, DocumentPart};
    let blocks = |blocks: &[ContentBlock]| {
        blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Text(text) => text.clone(),
                ContentBlock::Image(_) => "[Image]".into(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    document
        .parts()
        .iter()
        .map(|part| match part {
            DocumentPart::Text(text) => text.clone(),
            DocumentPart::Image(_) => "[Image]".into(),
            DocumentPart::Reference(_) => "[Reference]".into(),
            DocumentPart::Quotation(quote) => format!("Quotation\n{}", blocks(quote.blocks())),
            DocumentPart::Share(copy) => format!("Shared copy\n{}", blocks(copy.0.blocks())),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
