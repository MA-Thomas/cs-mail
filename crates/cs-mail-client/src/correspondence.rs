//! Headless endpoint composition. Only the explicit document is serialized;
//! a caller's private workspace and locally opened sources never enter this API.
use crate::NativeClient;
use cs_mail_application::consent::{RecoveryCopy, ReleasedCopy};
use cs_mail_application::correspondence::{MessageView, PreparedMessage};
use cs_mail_content::{ContentBinding, EndpointPublicKey};
use cs_mail_correspondence::{ConversationId, CorrespondenceError, Document, DocumentPart};
use cs_mail_primitives::CanonicalTime;
use cs_mail_protocol::ActorRef;

impl NativeClient {
    /// Builds an explicit outgoing document with a fresh private version secret.
    /// Keep this document and its prepared request stable across retries.
    /// # Errors
    /// Rejects invalid document content or an unavailable operating-system RNG.
    pub fn compose_correspondence(
        parts: Vec<DocumentPart>,
    ) -> Result<Document, CorrespondenceError> {
        let mut secret = [0; 32];
        getrandom::fill(&mut secret).map_err(|_| CorrespondenceError::Unavailable)?;
        Document::new(parts, secret)
    }

    /// # Errors
    /// Rejects a foreign author, self-delivery, invalid content or encryption failure.
    /// Recipient keys must come from the host's authenticated key-discovery path.
    pub fn prepare_correspondence(
        &self,
        document: &Document,
        conversation: ConversationId,
        recipient: EndpointPublicKey,
        custodian: EndpointPublicKey,
        binding: ContentBinding,
        lifetime: (CanonicalTime, CanonicalTime),
    ) -> Result<PreparedMessage, CorrespondenceError> {
        let (ActorRef::Sender(author) | ActorRef::Recipient(author)) = self.actor else {
            return Err(CorrespondenceError::Unauthorized);
        };
        if binding.sender != author || binding.recipient == author {
            return Err(CorrespondenceError::Invalid);
        }
        let manifest = document.manifest(binding.message_id, conversation)?;
        let plaintext = serde_json::to_vec(document).map_err(|_| CorrespondenceError::Invalid)?;
        let mut own_binding = binding.clone();
        own_binding.recipient = author;
        let sender_copy = self
            .encrypt_for(
                self.content_public_key(),
                own_binding,
                &plaintext,
                lifetime.0,
                lifetime.1,
            )
            .map_err(|_| CorrespondenceError::Invalid)?;
        let recipient_copy = self
            .encrypt_for(
                recipient,
                binding.clone(),
                &plaintext,
                lifetime.0,
                lifetime.1,
            )
            .map_err(|_| CorrespondenceError::Invalid)?;
        let sender_recovery = RecoveryCopy {
            sender: self.content_public_key(),
            ciphertext: self
                .encrypt_for(
                    custodian,
                    sender_copy.binding.clone(),
                    &plaintext,
                    lifetime.0,
                    lifetime.1,
                )
                .map_err(|_| CorrespondenceError::Invalid)?,
        };
        let recipient_recovery = RecoveryCopy {
            sender: self.content_public_key(),
            ciphertext: self
                .encrypt_for(custodian, binding, &plaintext, lifetime.0, lifetime.1)
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
    pub fn open_correspondence(
        &self,
        view: &MessageView,
        sender: EndpointPublicKey,
    ) -> Result<Document, CorrespondenceError> {
        let (ActorRef::Sender(owner) | ActorRef::Recipient(owner)) = self.actor else {
            return Err(CorrespondenceError::Unauthorized);
        };
        let binding = &view.ciphertext.binding;
        if binding.sender != view.record.author
            || binding.recipient != owner
            || binding.message_id != view.record.manifest.message()
        {
            return Err(CorrespondenceError::Invalid);
        }
        let plaintext = self
            .decrypt(sender, &view.ciphertext)
            .map_err(|_| CorrespondenceError::Invalid)?;
        let document: Document =
            serde_json::from_slice(&plaintext).map_err(|_| CorrespondenceError::Invalid)?;
        if document.manifest(binding.message_id, view.record.manifest.conversation())?
            != view.record.manifest
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(document)
    }
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
