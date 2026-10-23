//! Signed staging and complete publication of a request's native document.
use crate::correspondence::{
    MessageRecord, MessageRecordFields, PreparedMessage, validate_prepared,
};
use cs_mail_content::EndpointPublicKey;
use cs_mail_correspondence::{
    Conversation, ConversationId, CorrespondenceError, CorrespondenceRef,
};
use cs_mail_primitives::{CanonicalTime, ContentRef, EventRef, OperationalKeyRef, RequestId};
use cs_mail_protocol::{
    AdmissionBasis, Message, RelationshipKey, RelationshipRequest, RequestReservation,
};
use cs_mail_security::{AuthoritySnapshot, SignedContentKeyCertificate, SigningScope};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "DocumentRecord", into = "DocumentRecord")]
pub struct RequestDocument(DocumentRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DocumentRecord {
    alias: cs_mail_primitives::AliasId,
    request: RequestId,
    content: ContentRef,
    package: Box<PreparedMessage>,
}
impl TryFrom<DocumentRecord> for RequestDocument {
    type Error = CorrespondenceError;
    fn try_from(record: DocumentRecord) -> Result<Self, Self::Error> {
        if record.request.0 == 0
            || record.content.0 == 0
            || record.package.manifest.subject().is_none()
            || !record.package.manifest.sources().is_empty()
            || [
                &record.package.sender_copy,
                &record.package.recipient_copy,
                &record.package.sender_recovery.ciphertext,
                &record.package.recipient_recovery.ciphertext,
            ]
            .iter()
            .any(|copy| {
                copy.binding.content_ref != record.content
                    || copy.binding.message_id != record.package.manifest.message()
                    || copy.envelope.ciphertext.is_empty()
                    || copy.envelope.ciphertext.len()
                        > cs_mail_correspondence::MAX_ENCODED_DOCUMENT_BYTES + 16
            })
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(record))
    }
}
impl From<RequestDocument> for DocumentRecord {
    fn from(document: RequestDocument) -> Self {
        document.0
    }
}
impl RequestDocument {
    /// Initial request composition supports authored content, without source reuse.
    /// # Errors
    /// Rejects missing identifiers, source use or inconsistent encrypted copy bindings.
    pub fn new(
        request: RequestId,
        content: ContentRef,
        package: PreparedMessage,
        alias: cs_mail_primitives::AliasId,
    ) -> Result<Self, CorrespondenceError> {
        DocumentRecord {
            alias,
            request,
            content,
            package: Box::new(package),
        }
        .try_into()
    }
    pub const fn alias(&self) -> cs_mail_primitives::AliasId {
        self.0.alias
    }
    pub const fn request(&self) -> RequestId {
        self.0.request
    }
    pub const fn content(&self) -> ContentRef {
        self.0.content
    }
    pub fn package(&self) -> &PreparedMessage {
        &self.0.package
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRequestDocument {
    pub scope: SigningScope,
    pub key: OperationalKeyRef,
    pub document: RequestDocument,
    pub certificate: SignedContentKeyCertificate,
    pub signature: Vec<u8>,
}
impl SignedRequestDocument {
    fn bytes(&self) -> Result<Vec<u8>, CorrespondenceError> {
        serde_json::to_vec(&(
            "cs-mail/request-document/v3",
            self.scope,
            self.key,
            &self.document,
            &self.certificate,
        ))
        .map_err(|_| CorrespondenceError::Invalid)
    }
    /// # Errors
    /// Rejects an unencodable signed document.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, CorrespondenceError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects changed signed context, copy payload or certificate.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), CorrespondenceError> {
        VerifyingKey::from_bytes(key)
            .map_err(|_| CorrespondenceError::Unauthorized)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature)
                    .map_err(|_| CorrespondenceError::Unauthorized)?,
            )
            .map_err(|_| CorrespondenceError::Unauthorized)
    }
    /// # Errors
    /// Rejects an unencodable operation. Outer JSON field order is not signing authority.
    pub fn digest(&self) -> Result<[u8; 32], CorrespondenceError> {
        Ok(
            Sha256::digest(serde_json::to_vec(self).map_err(|_| CorrespondenceError::Invalid)?)
                .into(),
        )
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ConversationRecord", into = "ConversationRecord")]
pub struct RequestConversation(ConversationRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationRecord {
    conversation: ConversationId,
    request: RequestReservation,
    pair: CorrespondenceRef,
    sender: cs_mail_primitives::ProtocolIdentity,
    submission: EventRef,
    reverse_offer_epoch: Option<cs_mail_primitives::ReplyEpoch>,
}
impl TryFrom<ConversationRecord> for RequestConversation {
    type Error = CorrespondenceError;
    fn try_from(record: ConversationRecord) -> Result<Self, Self::Error> {
        if record.request.request.0 == 0
            || record.submission.0.0 == 0
            || !record.pair.contains(record.sender)
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(record))
    }
}
impl From<RequestConversation> for ConversationRecord {
    fn from(record: RequestConversation) -> Self {
        record.0
    }
}
impl RequestConversation {
    pub const fn conversation(&self) -> ConversationId {
        self.0.conversation
    }
    pub const fn request(&self) -> RequestReservation {
        self.0.request
    }
    pub const fn reverse_offer_epoch(&self) -> Option<cs_mail_primitives::ReplyEpoch> {
        self.0.reverse_offer_epoch
    }
    pub const fn submission(&self) -> EventRef {
        self.0.submission
    }
    pub const fn sender(&self) -> cs_mail_primitives::ProtocolIdentity {
        self.0.sender
    }
    pub const fn pair(&self) -> CorrespondenceRef {
        self.0.pair
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StageReceipt {
    pub digest: [u8; 32],
    pub at: CanonicalTime,
}
pub struct StageContext {
    pub authority: AuthoritySnapshot,
    pub relationship: RelationshipKey,
    pub scope: SigningScope,
    pub custody: EndpointPublicKey,
    pub policy: cs_mail_protocol::admission::AdmissionPolicy,
    pub request: Option<RelationshipRequest>,
    pub conversation: Option<RequestConversation>,
    pub conversation_available: bool,
    pub prior: Option<StageReceipt>,
}
pub struct StageDecision {
    receipt: StageReceipt,
    append: Option<(SignedRequestDocument, bool)>,
}
impl StageDecision {
    pub fn into_parts(self) -> (StageReceipt, Option<(SignedRequestDocument, bool)>) {
        (self.receipt, self.append)
    }
    /// # Errors
    /// Refuses wrong authority, altered retries, conflicting conversations or terminal staging.
    pub fn decide(
        signed: &SignedRequestDocument,
        c: StageContext,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        let key = c
            .authority
            .active_verifying_key(
                signed.key,
                cs_mail_protocol::ActorRef::Sender(c.relationship.sender),
                now,
            )
            .map_err(|_| CorrespondenceError::Unauthorized)?;
        if signed.scope != c.scope || signed.certificate.certificate.operational_key != signed.key {
            return Err(CorrespondenceError::Unauthorized);
        }
        signed.verify(&key)?;
        let digest = signed.digest()?;
        if let Some(prior) = c.prior {
            if prior.digest != digest {
                return Err(CorrespondenceError::Conflict);
            }
            return Ok(Self {
                receipt: prior,
                append: None,
            });
        }
        let certificate = c
            .authority
            .verify_content_key_certificate(
                &signed.certificate,
                now,
                signed.certificate.certificate.protocol_version,
                c.scope,
            )
            .map_err(|_| CorrespondenceError::Unauthorized)?;
        let package = signed.document.package();
        if signed.certificate.certificate.owner != c.relationship.sender
            || signed.certificate.certificate.key != package.sender_recovery.sender
            || package.recipient_copy.binding.sender_certificate != certificate
        {
            return Err(CorrespondenceError::Invalid);
        }
        validate_prepared(
            package,
            c.relationship.sender,
            signed.key,
            c.relationship.recipient,
            c.relationship.reference,
            Some(c.custody),
            now,
        )?;
        c.policy
            .evaluate(&package.recipient_copy.binding.declarations)
            .map_err(CorrespondenceError::AdmissionRefused)?;
        let request = c.request.ok_or(CorrespondenceError::Unauthorized)?;
        if signed.document.request() != request.id {
            return Err(CorrespondenceError::Invalid);
        }
        let initial = package.manifest.message() == request.initial_message;
        if initial {
            if !request.lifecycle.is_preparing_submission() || !c.conversation_available {
                return Err(CorrespondenceError::Conflict);
            }
        } else if !request.lifecycle.is_awaiting_recipient_decision()
            || c.conversation
                .is_none_or(|record| record.conversation() != package.manifest.conversation())
        {
            return Err(CorrespondenceError::Unauthorized);
        }
        Ok(Self {
            receipt: StageReceipt { digest, at: now },
            append: Some((signed.clone(), initial)),
        })
    }
}
pub struct SubmissionDocument {
    pub receiving_alias: cs_mail_accounts::aliases::Alias,
    pub policy_conversation: Option<Conversation>,
    pub document: SignedRequestDocument,
    pub conversation: Option<RequestConversation>,
}
pub enum ConversationWrite {
    Create(Conversation),
    Update(Conversation),
}
pub struct MailboxDelivery {
    conversation: ConversationWrite,
    link: RequestConversation,
    delivery: crate::correspondence::AdmittedDelivery,
    content: ContentRef,
}
impl MailboxDelivery {
    pub fn into_parts(
        self,
    ) -> (
        ConversationWrite,
        RequestConversation,
        crate::correspondence::AdmittedDelivery,
        ContentRef,
    ) {
        (self.conversation, self.link, self.delivery, self.content)
    }
    /// # Errors
    /// Rejects a staged document not bound to the admitted message and request.
    pub(super) fn decide(
        input: &SubmissionDocument,
        message: &Message,
        request: &RelationshipRequest,
        context: &cs_mail_protocol::TransitionContext,
        epoch: cs_mail_primitives::ReplyEpoch,
    ) -> Result<Self, CorrespondenceError> {
        let doc = &input.document.document;
        let package = doc.package();
        if input.receiving_alias.id() != request.terms.receiving_alias
            || input.receiving_alias.id() != doc.alias()
            || input.receiving_alias.owner() != request.terms.recipient
            || input
                .receiving_alias
                .check_conversation(
                    request.terms.sender,
                    package.manifest.conversation(),
                    context.now,
                )
                .is_err()
            || doc.request() != request.id
            || doc.content() != message.content
            || package.manifest.message() != message.id
        {
            return Err(CorrespondenceError::Invalid);
        }
        let initial =
            matches!(message.basis,AdmissionBasis::InitialRequest{request:id} if id==request.id);
        let (conversation, link) = if initial {
            if input.conversation.is_some() {
                return Err(CorrespondenceError::Conflict);
            }
            let pair = CorrespondenceRef::new(request.terms.sender, request.terms.recipient)?;
            let link: RequestConversation = ConversationRecord {
                conversation: package.manifest.conversation(),
                request: request.reservation(),
                pair,
                sender: request.terms.sender,
                submission: EventRef(context.journal_position),
                reverse_offer_epoch: Some(epoch),
            }
            .try_into()?;
            (
                ConversationWrite::Create(
                    Conversation::begun_by_request(link.conversation(), pair)
                        .bind_subject(package.manifest.subject())?
                        .receive(request.terms.recipient, message.id, epoch)?,
                ),
                link,
            )
        } else {
            let link = input
                .conversation
                .clone()
                .ok_or(CorrespondenceError::Invalid)?;
            if link.conversation() != package.manifest.conversation()
                || link.request() != request.reservation()
                || !matches!(message.basis,AdmissionBasis::RequestFollowup{request:id,..} if id==request.id)
            {
                return Err(CorrespondenceError::Invalid);
            }
            let current = input
                .policy_conversation
                .as_ref()
                .ok_or(CorrespondenceError::Invalid)?;
            (
                ConversationWrite::Update(
                    current.bind_subject(package.manifest.subject())?.receive(
                        request.terms.recipient,
                        message.id,
                        epoch,
                    )?,
                ),
                link,
            )
        };
        Ok(Self {
            conversation,
            link,
            delivery: crate::correspondence::AdmittedDelivery::new(
                MessageRecord::new(MessageRecordFields {
                    receiving: crate::aliases::DeliveredRoute::request(
                        &input.receiving_alias,
                        request.terms.sender,
                        package.manifest.conversation(),
                        message.id,
                        request.id,
                        context.now,
                    )?,
                    sender_key: package.sender_recovery.sender,
                    manifest: package.manifest.clone(),
                    author: request.terms.sender,
                    admission: message.basis,
                    committed_at: context.now,
                })?,
                Box::new(package.clone()),
            )?,
            content: doc.content(),
        })
    }
}
