//! Signed, headless correspondence use cases. This delivery service handles only
//! ciphertext; the separate consent service owns managed decryption authority.
use crate::accounts::{AccountClock, operations::AccountState};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_correspondence::{
    Conversation, ConversationId, CorrespondenceError, CorrespondenceRef,
    MAX_ENCODED_DOCUMENT_BYTES, MessageManifest, MessageSelection, PolicyAction,
};
use cs_mail_primitives::{
    AccountId, CanonicalTime, IdempotencyKey, MessageId, OperationalKeyRef, ProtocolIdentity,
};
use cs_mail_protocol::{Relationship, RelationshipState};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_SOURCE_GRAPH: usize = 1024;

/// Explicit outgoing content only. Source portals and private draft context are absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedMessage {
    pub manifest: MessageManifest,
    pub sender_copy: EncryptedContentRecord,
    pub recipient_copy: EncryptedContentRecord,
    pub sender_recovery: crate::consent::RecoveryCopy,
    pub recipient_recovery: crate::consent::RecoveryCopy,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum Command {
    Mailbox {
        after: u64,
        limit: u16,
    },
    Create {
        id: ConversationId,
        other: ProtocolIdentity,
    },
    ChangePolicy {
        id: ConversationId,
        revision: u64,
        action: PolicyAction,
    },
    Policy {
        id: ConversationId,
    },
    Assess {
        manifest: MessageManifest,
    },
    Send(Box<PreparedMessage>),
    Fetch {
        message: MessageId,
    },
    Resolve {
        source: MessageSelection,
    },
    DeleteCopies {
        id: ConversationId,
    },
}
impl Command {
    pub fn conversation(&self) -> Option<ConversationId> {
        match self {
            Self::Create { id, .. }
            | Self::ChangePolicy { id, .. }
            | Self::Policy { id }
            | Self::DeleteCopies { id } => Some(*id),
            Self::Assess { manifest } => Some(manifest.conversation()),
            Self::Send(p) => Some(p.manifest.conversation()),
            _ => None,
        }
    }
    pub fn source_messages(&self) -> Vec<MessageId> {
        match self {
            Self::Fetch { message } => vec![*message],
            Self::Resolve { source } => vec![source.message()],
            Self::Assess { manifest } => manifest
                .sources()
                .iter()
                .map(|s| s.selection().message())
                .collect(),
            Self::Send(p) => p
                .manifest
                .sources()
                .iter()
                .map(|s| s.selection().message())
                .chain([p.manifest.message()])
                .collect(),
            _ => vec![],
        }
    }
    fn audit_action(&self) -> Option<AuditAction> {
        match self {
            Self::Create { id, .. } => Some(AuditAction::Created(*id)),
            Self::ChangePolicy {
                id,
                revision,
                action,
            } => Some(AuditAction::PolicyChanged {
                conversation: *id,
                expected_revision: *revision,
                action: *action,
            }),
            Self::Send(p) => Some(AuditAction::Delivered(p.manifest.message())),
            Self::DeleteCopies { id } => Some(AuditAction::CopiesDeleted(*id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRequest {
    pub product: String,
    pub account: AccountId,
    pub persona: ProtocolIdentity,
    pub key: OperationalKeyRef,
    pub operation: IdempotencyKey,
    pub command: Command,
    pub signature: Vec<u8>,
}
impl SignedRequest {
    fn bytes(&self) -> Result<Vec<u8>, CorrespondenceError> {
        if self.product.is_empty()
            || self.account.0 == 0
            || self.persona.0 == 0
            || self.operation.0 == 0
        {
            return Err(CorrespondenceError::Invalid);
        }
        serde_json::to_vec(&(
            "cs-mail/correspondence-command/v3",
            &self.product,
            self.account,
            self.persona,
            self.key,
            self.operation,
            &self.command,
        ))
        .map_err(|_| CorrespondenceError::Invalid)
    }
    /// # Errors
    /// Rejects invalid signing context or serialization failure.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, CorrespondenceError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects tampered commands, keys or context.
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
    /// Rejects invalid signing context. Receipts retain only this digest, not ciphertext.
    pub fn digest(&self) -> Result<[u8; 32], CorrespondenceError> {
        Ok(Sha256::digest(self.bytes()?).into())
    }
}

/// Headers survive copy deletion for provenance and current-policy evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageRecord {
    pub manifest: MessageManifest,
    pub author: ProtocolIdentity,
    pub admission: cs_mail_protocol::AdmissionBasis,
    pub committed_at: CanonicalTime,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceState {
    Available,
    Unavailable,
    AccessDenied,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    pub reuse: Result<(), CorrespondenceError>,
    /// In manifest order, without source titles, author names or previews.
    pub recipient_sources: Vec<SourceState>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    pub record: MessageRecord,
    pub ciphertext: EncryptedContentRecord,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Mailbox {
        entries: Vec<(u64, MessageRecord)>,
        next_after: u64,
    },
    Conversation(Conversation),
    Assessment(Assessment),
    Committed(MessageId),
    Message(Box<MessageView>),
    Unavailable,
    AccessDenied,
    CopiesDeleted,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditAction {
    Created(ConversationId),
    PolicyChanged {
        conversation: ConversationId,
        expected_revision: u64,
        action: PolicyAction,
    },
    Delivered(MessageId),
    CopiesDeleted(ConversationId),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub actor: ProtocolIdentity,
    pub key: OperationalKeyRef,
    pub action: AuditAction,
    pub digest: [u8; 32],
    pub outcome: Response,
    pub authorized_at: CanonicalTime,
    /// Historical evidence, never authority for a subsequent reuse operation.
    pub policy_revisions: BTreeMap<ConversationId, u64>,
}
pub struct Context {
    pub mailbox: Vec<(u64, MessageRecord, CanonicalTime)>,
    pub authority: AccountState,
    pub custody_key: Option<cs_mail_content::EndpointPublicKey>,
    pub contact: Option<Relationship>,
    pub lane: Option<cs_mail_capabilities::Lane>,
    pub conversations: BTreeMap<ConversationId, Conversation>,
    pub messages: BTreeMap<MessageId, MessageRecord>,
    pub retained: BTreeMap<(MessageId, ProtocolIdentity), CanonicalTime>,
    pub fetched_copy: Option<EncryptedContentRecord>,
    pub prior: Option<Receipt>,
}
pub struct Effects {
    pub lane: Option<cs_mail_capabilities::Lane>,
    pub conversation: Option<Conversation>,
    pub delivery: Option<(MessageRecord, Box<PreparedMessage>)>,
    pub delete_copies: Option<ConversationId>,
    pub receipt: Receipt,
}
pub struct Decision {
    response: Response,
    effects: Option<Effects>,
}
impl Decision {
    pub fn into_parts(self) -> (Response, Option<Effects>) {
        (self.response, self.effects)
    }
}
/// Hold account/key, relationship, policy, source and absent-ID protection from
/// context loading through commit. Serialize with account and contact revocation.
/// Invoke the callback exactly once. Persist all effects and the receipt atomically.
/// Read results must never enter replay receipts. No external I/O under protection.
pub trait CorrespondenceStore {
    type Error: From<CorrespondenceError>;
    /// # Errors
    /// Rolls back the whole decision on rejection or storage failure.
    fn transact(
        &self,
        request: &SignedRequest,
        decide: impl FnOnce(Context, &dyn AccountClock) -> Result<Decision, Self::Error>,
    ) -> Result<Response, Self::Error>;
}
pub struct CorrespondenceService<'a, R> {
    repository: &'a R,
}
impl<'a, R: CorrespondenceStore> CorrespondenceService<'a, R> {
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    /// # Errors
    /// Rejects invalid live authority, stale policy, altered retries or invalid effects.
    pub fn handle(&self, request: &SignedRequest) -> Result<Response, R::Error> {
        self.repository.transact(request, |context, clock| {
            decide(request, &context, clock.now()).map_err(Into::into)
        })
    }
}

fn owned(
    context: &Context,
    id: ConversationId,
    actor: ProtocolIdentity,
) -> Result<&Conversation, CorrespondenceError> {
    context
        .conversations
        .get(&id)
        .filter(|c| c.scope().contains(actor))
        .ok_or(CorrespondenceError::Unauthorized)
}
fn contact(
    context: &Context,
    actor: ProtocolIdentity,
    other: ProtocolIdentity,
    now: CanonicalTime,
) -> Result<&Relationship, CorrespondenceError> {
    context
        .contact
        .as_ref()
        .filter(|r| {
            r.key.sender == actor
                && r.key.recipient == other
                && r.state != RelationshipState::Blocked
                && (r.state == RelationshipState::Accepted
                    || context.lane.as_ref().is_some_and(|l| {
                        l.grant.sender == actor
                            && l.grant.recipient == other
                            && matches!(
                                l.grant.subject,
                                cs_mail_capabilities::LaneSubject::Native(_)
                            )
                            && l.is_current(now)
                    }))
        })
        .ok_or(CorrespondenceError::Unauthorized)
}

/// Policy inheritance is a conservative graph walk; intermediate visual grouping
/// cannot remove a source edge. Missing ancestors fail closed.
fn policies(
    context: &Context,
    manifest: &MessageManifest,
    actor: ProtocolIdentity,
) -> Result<BTreeMap<ConversationId, u64>, CorrespondenceError> {
    let destination = owned(context, manifest.conversation(), actor)?.scope();
    let mut pending = vec![];
    for source in manifest.sources() {
        let record = context
            .messages
            .get(&source.selection().message())
            .ok_or(CorrespondenceError::PolicyUnresolved)?;
        owned(context, record.manifest.conversation(), actor)?;
        source.selection().validate_target(&record.manifest)?;
        pending.push(source.selection().message());
    }
    let mut visited = BTreeSet::new();
    let mut observed = BTreeMap::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        if visited.len() > MAX_SOURCE_GRAPH {
            return Err(CorrespondenceError::Invalid);
        }
        let record = context
            .messages
            .get(&id)
            .ok_or(CorrespondenceError::PolicyUnresolved)?;
        let conversation = context
            .conversations
            .get(&record.manifest.conversation())
            .ok_or(CorrespondenceError::PolicyUnresolved)?;
        if !conversation.permits(destination) {
            return Err(CorrespondenceError::PolicyRestricted);
        }
        observed.insert(conversation.id(), conversation.revision());
        for source in record.manifest.sources() {
            let ancestor = context
                .messages
                .get(&source.selection().message())
                .ok_or(CorrespondenceError::PolicyUnresolved)?;
            source.selection().validate_target(&ancestor.manifest)?;
            pending.push(source.selection().message());
        }
    }
    let destination = owned(context, manifest.conversation(), actor)?;
    observed.insert(destination.id(), destination.revision());
    Ok(observed)
}
fn source_state(
    context: &Context,
    source: &MessageSelection,
    actor: ProtocolIdentity,
    now: CanonicalTime,
) -> SourceState {
    let Some(record) = context.messages.get(&source.message()) else {
        return SourceState::AccessDenied;
    };
    if owned(context, record.manifest.conversation(), actor).is_err() {
        return SourceState::AccessDenied;
    }
    if context
        .retained
        .get(&(source.message(), actor))
        .is_some_and(|expires| now < *expires)
    {
        SourceState::Available
    } else {
        SourceState::Unavailable
    }
}

fn validate_payload(
    package: &PreparedMessage,
    request: &SignedRequest,
    other: ProtocolIdentity,
    relationship: &Relationship,
    custody_key: Option<cs_mail_content::EndpointPublicKey>,
    now: CanonicalTime,
) -> Result<(), CorrespondenceError> {
    let custody_key = custody_key.ok_or(CorrespondenceError::Unavailable)?;
    let actor = request.persona;
    let mut binding = package.sender_copy.binding.clone();
    binding.recipient = other;
    if binding != package.recipient_copy.binding
        || package.sender_copy.envelope.sender_key != package.recipient_copy.envelope.sender_key
    {
        return Err(CorrespondenceError::Invalid);
    }
    for (copy, recovery) in [
        (&package.sender_copy, &package.sender_recovery),
        (&package.recipient_copy, &package.recipient_recovery),
    ] {
        if recovery.ciphertext.binding != copy.binding
            || recovery.ciphertext.created_at != copy.created_at
            || recovery.ciphertext.expires_at != copy.expires_at
            || recovery.sender.reference != copy.envelope.sender_key
            || recovery.sender.bytes == [0; 32]
            || recovery.ciphertext.envelope.sender_key != recovery.sender.reference
            || recovery.ciphertext.envelope.recipient_key != custody_key.reference
            || recovery.ciphertext.envelope.ciphertext.is_empty()
            || recovery.ciphertext.envelope.ciphertext.len() > MAX_ENCODED_DOCUMENT_BYTES + 16
        {
            return Err(CorrespondenceError::Invalid);
        }
    }
    if package.sender_recovery.sender != package.recipient_recovery.sender {
        return Err(CorrespondenceError::Invalid);
    }
    for (owner, copy) in [
        (actor, &package.sender_copy),
        (other, &package.recipient_copy),
    ] {
        let binding = &copy.binding;
        if binding.sender != actor
            || binding.recipient != owner
            || binding.message_id != package.manifest.message()
            || binding.relationship != relationship.key.reference
            || binding.wire_version != cs_mail_primitives::WireVersion(1)
            || binding.protocol_version != cs_mail_primitives::ProtocolVersion(2)
            || binding.declarations.origin.authority
                != cs_mail_primitives::DeclarationAuthority::NativeSender(request.key)
            || copy.created_at > now
            || copy.expires_at <= now
            || binding.message_valid_until.0 <= now
            || copy.envelope.ciphertext.is_empty()
            || copy.envelope.ciphertext.len() > MAX_ENCODED_DOCUMENT_BYTES + 16
            || binding.declarations.validate().is_err()
        {
            return Err(CorrespondenceError::Invalid);
        }
    }
    Ok(())
}

/// Authenticate before reading private source state, and again after all protected reads.
/// # Errors
/// Rejects inactive accounts, foreign personas/keys, product mismatch or bad signatures.
pub fn authenticate_request(
    request: &SignedRequest,
    state: &AccountState,
    now: CanonicalTime,
) -> Result<(), CorrespondenceError> {
    let key = state
        .persona_key(&request.product, request.persona, request.key, now)
        .map_err(|_| CorrespondenceError::Unauthorized)?;
    request.verify(&key)?;
    Ok(())
}
fn replay(
    request: &SignedRequest,
    context: &Context,
    digest: [u8; 32],
) -> Result<Option<Decision>, CorrespondenceError> {
    if let Some(prior) = &context.prior {
        if prior.digest != digest
            || Some(&prior.action) != request.command.audit_action().as_ref()
            || prior.actor != request.persona
            || prior.key != request.key
        {
            return Err(CorrespondenceError::Conflict);
        }
        let valid = match (&request.command, &prior.outcome) {
            (
                Command::Create { id, .. } | Command::ChangePolicy { id, .. },
                Response::Conversation(c),
            ) => c.id() == *id && c.scope().contains(request.persona),
            (Command::Send(p), Response::Committed(id)) => p.manifest.message() == *id,
            (Command::DeleteCopies { .. }, Response::CopiesDeleted) => true,
            _ => false,
        };
        if !valid {
            return Err(CorrespondenceError::Invalid);
        }
        return Ok(Some(Decision {
            response: prior.outcome.clone(),
            effects: None,
        }));
    }
    Ok(None)
}

fn admit_message(
    context: &Context,
    relationship: &Relationship,
    actor: ProtocolIdentity,
    destination: ProtocolIdentity,
    package: &PreparedMessage,
    now: CanonicalTime,
) -> Result<
    (
        cs_mail_protocol::AdmissionBasis,
        Option<cs_mail_capabilities::Lane>,
    ),
    CorrespondenceError,
> {
    let mut next_lane = None;
    let admission = if relationship.state == RelationshipState::Accepted {
        cs_mail_protocol::AdmissionBasis::AcceptedRelationship {
            version: relationship.version,
        }
    } else {
        let mut lane = context
            .lane
            .clone()
            .ok_or(CorrespondenceError::Unauthorized)?;
        let cs_mail_capabilities::LaneSubject::Native(subject) = lane.grant.subject else {
            return Err(CorrespondenceError::Unauthorized);
        };
        // The authenticated persona must equal the grant's native sender; the
        // recipient signed the subject binding. No sender-provided evidence is trusted.
        let binding = &package.recipient_copy.binding;
        let evidence = cs_mail_capabilities::LaneEvidence::Native(subject);
        lane.authorizes(
            actor,
            destination,
            binding.capability,
            &binding.declarations,
            &evidence,
            now,
        )
        .map_err(|_| CorrespondenceError::Unauthorized)?;
        if lane
            .consume(
                package.manifest.message(),
                binding.capability,
                &binding.declarations,
                &evidence,
                now,
            )
            .map_err(|_| CorrespondenceError::Unauthorized)?
        {
            return Err(CorrespondenceError::Conflict);
        }
        let basis = cs_mail_protocol::AdmissionBasis::ExpressLane {
            lane: lane.grant.id,
            version: lane.version,
        };
        next_lane = Some(lane);
        basis
    };
    Ok((admission, next_lane))
}

#[allow(clippy::too_many_lines)] // One atomic dispatch for the signed correspondence command.
fn decide(
    request: &SignedRequest,
    context: &Context,
    now: CanonicalTime,
) -> Result<Decision, CorrespondenceError> {
    authenticate_request(request, &context.authority, now)?;
    let digest = request.digest()?;
    if let Some(prior) = replay(request, context, digest)? {
        return Ok(prior);
    }
    let actor = request.persona;
    let mut conversation_write = None;
    let mut delivery = None;
    let mut lane_write = None;
    let mut delete_copies = None;
    let mut policy_revisions = BTreeMap::new();
    let response = match &request.command {
        Command::Mailbox { after, .. } => {
            let next_after = context
                .mailbox
                .last()
                .map_or(*after, |(sequence, _, _)| *sequence);
            let entries = context
                .mailbox
                .iter()
                .filter(|(_, _, expiry)| now < *expiry)
                .map(|(sequence, record, _)| (*sequence, record.clone()))
                .collect();
            Response::Mailbox {
                entries,
                next_after,
            }
        }
        Command::Create { id, other } => {
            contact(context, actor, *other, now)?;
            if context.conversations.contains_key(id) {
                return Err(CorrespondenceError::Conflict);
            }
            let conversation = Conversation::new(*id, CorrespondenceRef::new(actor, *other)?);
            conversation_write = Some(conversation.clone());
            Response::Conversation(conversation)
        }
        Command::ChangePolicy {
            id,
            revision,
            action,
        } => {
            let next = owned(context, *id, actor)?.change(actor, *revision, *action)?;
            policy_revisions.insert(*id, next.revision());
            conversation_write = Some(next.clone());
            Response::Conversation(next)
        }
        Command::Policy { id } => Response::Conversation(owned(context, *id, actor)?.clone()),
        Command::Assess { manifest } => assess(context, manifest, actor, now)?,
        Command::Send(package) => {
            let manifest = &package.manifest;
            if context.messages.contains_key(&manifest.message()) {
                return Err(CorrespondenceError::Conflict);
            }
            let destination = owned(context, manifest.conversation(), actor)?
                .scope()
                .other(actor)?;
            let relationship = contact(context, actor, destination, now)?;
            validate_payload(
                package,
                request,
                destination,
                relationship,
                context.custody_key,
                now,
            )?;
            let (admission, next_lane) =
                admit_message(context, relationship, actor, destination, package, now)?;
            lane_write = next_lane;
            policy_revisions = policies(context, manifest, actor)?;
            delivery = Some((
                MessageRecord {
                    manifest: manifest.clone(),
                    author: actor,
                    admission,
                    committed_at: now,
                },
                package.clone(),
            ));
            Response::Committed(manifest.message())
        }
        Command::Fetch { message } => fetch(context, *message, actor, now),
        Command::Resolve { source } => resolve(context, source, actor, now)?,
        Command::DeleteCopies { id } => {
            owned(context, *id, actor)?;
            delete_copies = Some(*id);
            Response::CopiesDeleted
        }
    };
    let effects = request.command.audit_action().map(|action| Effects {
        lane: lane_write,
        conversation: conversation_write,
        delivery,
        delete_copies,
        receipt: Receipt {
            actor,
            key: request.key,
            action,
            digest,
            outcome: response.clone(),
            authorized_at: now,
            policy_revisions,
        },
    });
    Ok(Decision { response, effects })
}
fn fetch(
    context: &Context,
    message: MessageId,
    actor: ProtocolIdentity,
    now: CanonicalTime,
) -> Response {
    let Some(record) = context.messages.get(&message) else {
        return Response::AccessDenied;
    };
    if owned(context, record.manifest.conversation(), actor).is_err() {
        return Response::AccessDenied;
    }
    match &context.fetched_copy {
        Some(ciphertext)
            if context
                .retained
                .get(&(message, actor))
                .is_some_and(|expires| now < *expires) =>
        {
            Response::Message(Box::new(MessageView {
                record: record.clone(),
                ciphertext: ciphertext.clone(),
            }))
        }
        _ => Response::Unavailable,
    }
}

fn assess(
    context: &Context,
    manifest: &MessageManifest,
    actor: ProtocolIdentity,
    now: CanonicalTime,
) -> Result<Response, CorrespondenceError> {
    let destination = owned(context, manifest.conversation(), actor)?
        .scope()
        .other(actor)?;
    // Do not reveal target access/availability when the actor cannot reuse it.
    let result = policies(context, manifest, actor);
    let recipient_sources = if result.is_ok() {
        manifest
            .sources()
            .iter()
            .map(|s| source_state(context, s.selection(), destination, now))
            .collect()
    } else {
        vec![]
    };
    Ok(Response::Assessment(Assessment {
        reuse: result.map(|_| ()),
        recipient_sources,
    }))
}

fn resolve(
    context: &Context,
    source: &MessageSelection,
    actor: ProtocolIdentity,
    now: CanonicalTime,
) -> Result<Response, CorrespondenceError> {
    match source_state(context, source, actor, now) {
        SourceState::AccessDenied => Ok(Response::AccessDenied),
        SourceState::Unavailable => Ok(Response::Unavailable),
        SourceState::Available => {
            source.validate_target(
                &context
                    .messages
                    .get(&source.message())
                    .ok_or(CorrespondenceError::Unavailable)?
                    .manifest,
            )?;
            Ok(fetch(context, source.message(), actor, now))
        }
    }
}
