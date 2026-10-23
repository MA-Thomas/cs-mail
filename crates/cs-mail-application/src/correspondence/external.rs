//! External text correspondence: signed member commands, trusted receiver admission,
//! and consent-gated delivery. Adapters supply protected facts, never decisions.
use crate::accounts::{AccountClock, operations::AccountState};
use cs_mail_billing::BillingAccount;
use cs_mail_content::{EncryptedContentRecord, EndpointPublicKey};
use cs_mail_correspondence::external::{ExternalConversation, delivery::DeliveryState};
use cs_mail_correspondence::{ConversationId, CorrespondenceError as Error, MessageManifest};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{
    AccountId, CanonicalTime, IdempotencyKey, MessageId, OperationalKeyRef, ProtocolIdentity,
};
use cs_mail_protocol::{Relationship, RelationshipState};
use cs_mail_security::RecipientSigningScope;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Only newly authored text is supported in this slice; native source exports are
/// refused until their policy/provenance graph can be evaluated at dispatch too.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedText {
    pub manifest: MessageManifest,
    pub sender_key: EndpointPublicKey,
    pub member_copy: EncryptedContentRecord,
    pub relay_copy: EncryptedContentRecord,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Start {
        receiving_alias: cs_mail_primitives::AliasId,
        mailbox: GuestMailbox,
        message: Box<PreparedText>,
        disclose_to_external_mail: bool,
    },
    Send(Box<PreparedText>),
    Reject,
    WithdrawDisclosure,
    GrantDisclosure,
    Read {
        after: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCommand {
    pub scope: RecipientSigningScope,
    pub product: String,
    pub account: AccountId,
    pub persona: ProtocolIdentity,
    pub key: OperationalKeyRef,
    pub operation: IdempotencyKey,
    pub conversation: ConversationId,
    pub command: Command,
    pub signature: Vec<u8>,
}
impl SignedCommand {
    /// Exact historical replay still requires live caller authority.
    /// # Errors
    /// Refuses altered retries.
    pub fn replay(&self, receipt: &Receipt) -> Result<Response, Error> {
        if receipt.digest != self.digest()? || receipt.key != self.key {
            return Err(Error::Conflict);
        }
        Ok(Response::Outcome(Acknowledgement::recorded(
            receipt.clone(),
        )))
    }
    fn bytes(&self) -> Result<Vec<u8>, Error> {
        if self.operation.0 == 0
            || self.persona.0 == 0
            || self.account.0 == 0
            || self.product.is_empty()
        {
            return Err(Error::Invalid);
        }
        serde_json::to_vec(&(
            "cs-mail/external-command/v2",
            self.scope,
            &self.product,
            self.account,
            self.persona,
            self.key,
            self.operation,
            self.conversation,
            &self.command,
        ))
        .map_err(|_| Error::Invalid)
    }
    /// # Errors
    /// Invalid signing context.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, Error> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Invalid context or signature.
    pub fn authenticate(
        &self,
        authority: &AccountState,
        scope: RecipientSigningScope,
        now: CanonicalTime,
    ) -> Result<(), Error> {
        if self.scope != scope {
            return Err(Error::Unauthorized);
        }
        let key = authority
            .persona_key(&self.product, self.persona, self.key, now)
            .map_err(|_| Error::Unauthorized)?;
        VerifyingKey::from_bytes(&key)
            .map_err(|_| Error::Unauthorized)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| Error::Unauthorized)?,
            )
            .map_err(|_| Error::Unauthorized)
    }
    /// # Errors
    /// Invalid signing context.
    pub fn digest(&self) -> Result<[u8; 32], Error> {
        Ok(Sha256::digest(self.bytes()?).into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    MemberToGuest,
    GuestToMember(Box<crate::aliases::DeliveredRoute>),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageFields {
    pub manifest: MessageManifest,
    pub sender_key: EndpointPublicKey,
    pub member_copy: EncryptedContentRecord,
    pub direction: Direction,
    pub committed_at: CanonicalTime,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "MessageFields", into = "MessageFields")]
pub struct Message(MessageFields);
impl Message {
    /// # Errors
    /// Rejects a received message without matching inbox, participant and time bindings.
    pub fn new(fields: MessageFields) -> Result<Self, Error> {
        let binding = &fields.member_copy.binding;
        let valid = match &fields.direction {
            Direction::MemberToGuest => binding.sender == binding.recipient,
            Direction::GuestToMember(route) => {
                route.transport() == crate::aliases::DeliveryTransport::External
                    && route.message() == fields.manifest.message()
                    && route.conversation() == fields.manifest.conversation()
                    && route.sender() == binding.sender
                    && route.recipient() == binding.recipient
                    && route.at() == fields.committed_at
            }
        };
        if !valid {
            return Err(Error::Invalid);
        }
        Ok(Self(fields))
    }
}
impl TryFrom<MessageFields> for Message {
    type Error = Error;
    fn try_from(fields: MessageFields) -> Result<Self, Error> {
        Self::new(fields)
    }
}
impl From<Message> for MessageFields {
    fn from(message: Message) -> Self {
        message.0
    }
}
impl std::ops::Deref for Message {
    type Target = MessageFields;
    fn deref(&self) -> &MessageFields {
        &self.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    pub sequence: u64,
    pub message: Message,
    pub delivery: Option<DeliveryState>,
    pub provider_events: Vec<cs_mail_correspondence::external::delivery::ProviderEvent>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Queued(MessageId),
    Rejected,
    DisclosureWithdrawn,
    DisclosureGranted,
    Refused(Error),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Outcome(Acknowledgement),
    Conversation {
        conversation: Box<ExternalConversation>,
        relationship: Box<Relationship>,
        messages: Vec<MessageView>,
        next_after: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub digest: [u8; 32],
    pub key: OperationalKeyRef,
    pub signature: Vec<u8>,
    pub at: CanonicalTime,
    pub outcome: Outcome,
}
/// Provider-authenticated commitment to the durable result of this exact command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgement {
    pub receipt: Receipt,
    pub signature: Vec<u8>,
}
impl Acknowledgement {
    fn recorded(receipt: Receipt) -> Self {
        Self {
            receipt,
            signature: Vec::new(),
        }
    }
    fn bytes(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = b"cs-mail/external-acknowledgement/v1".to_vec();
        bytes.extend(serde_json::to_vec(&self.receipt).map_err(|_| Error::Invalid)?);
        Ok(bytes)
    }
    /// # Errors
    /// Refuses invalid serialization before signing the recorded outcome.
    pub fn sign(mut self, provider: &[u8; 32]) -> Result<Self, Error> {
        self.signature = SigningKey::from_bytes(provider)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects altered commands, outcomes, signatures or provider authority.
    pub fn verify(&self, request: &SignedCommand, provider: &[u8; 32]) -> Result<(), Error> {
        if self.receipt.digest != request.digest()?
            || self.receipt.key != request.key
            || self.receipt.signature != request.signature
        {
            return Err(Error::Invalid);
        }
        VerifyingKey::from_bytes(provider)
            .map_err(|_| Error::Invalid)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| Error::Invalid)?,
            )
            .map_err(|_| Error::Invalid)
    }
}
pub struct Context {
    pub receiving_inbox: Option<cs_mail_accounts::aliases::Alias>,
    pub scope: RecipientSigningScope,
    pub authority: AccountState,
    pub billing: BillingAccount,
    pub relationship: Relationship,
    pub conversation: Option<ExternalConversation>,
    pub relay_key: EndpointPublicKey,
    pub member_key: EndpointPublicKey,
    pub prior: Option<Receipt>,
    pub messages: Vec<MessageView>,
}
pub struct Effects {
    pub changes: Option<Changes>,
    pub receipt: Receipt,
}
pub struct Changes {
    pub conversation: ExternalConversation,
    pub delivery: Option<Box<PreparedText>>,
    pub cancel_waiting: bool,
}
pub struct Decision {
    pub response: Response,
    pub effects: Option<Effects>,
}
pub trait Store {
    type Error: From<Error>;
    /// Hold account, relationship and conversation protection through commit;
    /// serialize with incoming blocks, dispatch, and disclosure withdrawal.
    /// # Errors
    /// Any failure rolls back conversation, content, queue, and receipt together.
    fn transact(
        &self,
        request: &SignedCommand,
        decide: impl FnOnce(Context, &dyn AccountClock) -> Result<Decision, Self::Error>,
    ) -> Result<Response, Self::Error>;
}
pub struct Service<'a, S>(pub &'a S);
impl<S: Store> Service<'_, S> {
    /// # Errors
    /// Returns authority, domain or persistence refusals.
    pub fn handle(&self, request: &SignedCommand) -> Result<Response, S::Error> {
        self.0.transact(request, |context, clock| {
            decide(request, context, clock.now()).map_err(Into::into)
        })
    }
}

/// Structural checks apply to incoming packages and restored database records alike.
/// # Errors
/// Rejects nontext manifests, malformed envelopes and inconsistent content bindings.
pub fn validate_copy(
    manifest: &MessageManifest,
    sender_key: EndpointPublicKey,
    copy: &EncryptedContentRecord,
) -> Result<(), Error> {
    use cs_mail_correspondence::BlockDescriptor;
    use cs_mail_primitives::{ProtocolVersion, WireVersion};
    if !manifest.sources().is_empty()
        || manifest
            .blocks()
            .iter()
            .any(|b| !matches!(b, BlockDescriptor::Text { .. }))
        || manifest
            .blocks()
            .iter()
            .map(|b| match b {
                BlockDescriptor::Text { bytes } => u64::from(*bytes),
                _ => 0,
            })
            .sum::<u64>()
            > 16384
        || copy.binding.message_id != manifest.message()
        || copy.binding.content_ref.0 == 0
        || copy.binding.wire_version != WireVersion(1)
        || copy.binding.protocol_version != ProtocolVersion(2)
        || copy.binding.declarations.validate().is_err()
        || sender_key.bytes == [0; 32]
        || sender_key.reference.0 == 0
        || copy.envelope.sender_key != sender_key.reference
        || copy.envelope.recipient_key.0 == 0
        || copy.envelope.encapsulated_key.len() != 32
        || !(16..=65536).contains(&copy.envelope.ciphertext.len())
        || copy.created_at >= copy.expires_at
        || copy.created_at >= copy.binding.message_valid_until.0
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
impl Message {
    /// # Errors
    /// Rejects a stored message bound to a different conversation or relationship.
    pub fn validate(&self, c: &ExternalConversation, r: &Relationship) -> Result<(), Error> {
        validate_copy(&self.manifest, self.sender_key, &self.member_copy)?;
        let sender = match &self.direction {
            Direction::MemberToGuest => c.member(),
            Direction::GuestToMember(_) => c.guest(),
        };
        if self.manifest.conversation() != c.id()
            || r.key.sender != c.guest()
            || r.key.recipient != c.member()
            || self.member_copy.binding.sender != sender
            || self.member_copy.binding.recipient != c.member()
            || self.member_copy.binding.relationship != r.key.reference
            || self.member_copy.created_at > self.committed_at
        {
            return Err(Error::Invalid);
        }
        if let Direction::GuestToMember(route) = &self.direction {
            if route.transport() != crate::aliases::DeliveryTransport::External
                || route.message() != self.manifest.message()
                || route.conversation() != c.id()
                || route.sender() != c.guest()
                || route.recipient() != c.member()
                || route.at() != self.committed_at
            {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
}

fn validate_message(
    p: &PreparedText,
    request: &SignedCommand,
    c: &Context,
    now: CanonicalTime,
) -> Result<(), Error> {
    if p.manifest.conversation() != request.conversation
        || !p.manifest.sources().is_empty()
        || p.manifest.message().0 == 0
        || p.sender_key.bytes == [0; 32]
        || p.member_copy.envelope.recipient_key != c.member_key.reference
        || p.relay_copy.envelope.recipient_key != c.relay_key.reference
        || p.member_copy.binding != p.relay_copy.binding
    {
        return Err(Error::Invalid);
    }
    if p.member_copy.binding.declarations.origin.authority
        != cs_mail_primitives::DeclarationAuthority::NativeSender(request.key)
        || p.member_copy.binding.capability.is_some()
    {
        return Err(Error::Invalid);
    }
    for copy in [&p.member_copy, &p.relay_copy] {
        validate_copy(&p.manifest, p.sender_key, copy)?;
        if copy.binding.message_id != p.manifest.message()
            || copy.binding.recipient != request.persona
            || copy.binding.sender != request.persona
            || copy.binding.relationship != c.relationship.key.reference
            || copy.envelope.sender_key != p.sender_key.reference
            || copy.envelope.ciphertext.is_empty()
            || copy.envelope.ciphertext.len() > 65536
            || copy.created_at > now
            || copy.expires_at <= now
            || copy.binding.message_valid_until.0 <= now
        {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
#[allow(clippy::too_many_lines)] // One signed command decision and its complete effects.
fn decide(request: &SignedCommand, c: Context, now: CanonicalTime) -> Result<Decision, Error> {
    request.authenticate(&c.authority, c.scope, now)?;
    if let Some(prior) = c.prior {
        return Ok(Decision {
            response: request.replay(&prior)?,
            effects: None,
        });
    }
    if c.conversation
        .as_ref()
        .is_some_and(|conversation| conversation.member() != request.persona)
    {
        return Err(Error::Unauthorized);
    }
    if let Command::Read { after } = request.command {
        let conversation = c.conversation.as_ref().ok_or(Error::Unavailable)?;
        for view in &c.messages {
            view.message.validate(conversation, &c.relationship)?;
        }
        let next_after = c.messages.last().map_or(after, |m| m.sequence);
        return Ok(Decision {
            response: Response::Conversation {
                conversation: Box::new(c.conversation.ok_or(Error::Unavailable)?),
                relationship: Box::new(c.relationship),
                messages: c
                    .messages
                    .into_iter()
                    .filter(|m| m.message.member_copy.expires_at > now)
                    .collect(),
                next_after,
            },
            effects: None,
        });
    }
    let (changes, outcome) = match mutation(request, c, now) {
        Ok((changes, outcome)) => (Some(changes), outcome),
        // Missing facts and unresolved policy cannot establish a terminal refusal.
        Err(error @ (Error::Unavailable | Error::PolicyUnresolved)) => return Err(error),
        Err(error) => (None, Outcome::Refused(error)),
    };
    let receipt = Receipt {
        digest: request.digest()?,
        key: request.key,
        signature: request.signature.clone(),
        at: now,
        outcome,
    };
    Ok(Decision {
        response: Response::Outcome(Acknowledgement::recorded(receipt.clone())),
        effects: Some(Effects { changes, receipt }),
    })
}
fn mutation(
    request: &SignedCommand,
    c: Context,
    now: CanonicalTime,
) -> Result<(Changes, Outcome), Error> {
    let (conversation, delivery, cancel_waiting, outcome) = match &request.command {
        Command::Start {
            receiving_alias,
            mailbox,
            message,
            disclose_to_external_mail,
        } => {
            let inbox = c
                .receiving_inbox
                .as_ref()
                .filter(|a| a.id() == *receiving_alias && a.owner() == request.persona)
                .ok_or(Error::Unavailable)?;
            inbox
                .check_conversation(c.relationship.key.sender, request.conversation, now)
                .map_err(|_| Error::Unauthorized)?;
            if !disclose_to_external_mail || !c.billing.covers(now) {
                return Err(Error::Unauthorized);
            }
            if c.conversation.is_some() {
                return Err(Error::Conflict);
            }
            validate_message(message, request, &c, now)?;
            (
                ExternalConversation::start(
                    request.conversation,
                    request.persona,
                    *receiving_alias,
                    mailbox.clone(),
                    c.scope.deployment_domain,
                    &c.relationship,
                    now,
                )?,
                Some(message.clone()),
                false,
                Outcome::Queued(message.manifest.message()),
            )
        }
        Command::Send(message) => {
            let conversation = c.conversation.clone().ok_or(Error::Unavailable)?;
            if !c.billing.covers(now)
                || !conversation.disclosure().permits_dispatch()
                || c.relationship.state == RelationshipState::Blocked
            {
                return Err(Error::Unauthorized);
            }
            validate_message(message, request, &c, now)?;
            (
                conversation,
                Some(message.clone()),
                false,
                Outcome::Queued(message.manifest.message()),
            )
        }
        Command::Reject => (
            c.conversation.ok_or(Error::Unavailable)?.reject(
                request.persona,
                &c.relationship,
                now,
            )?,
            None,
            false,
            Outcome::Rejected,
        ),
        Command::WithdrawDisclosure => (
            c.conversation
                .ok_or(Error::Unavailable)?
                .withdraw(request.persona, now)?,
            None,
            true,
            Outcome::DisclosureWithdrawn,
        ),
        Command::GrantDisclosure => {
            if !c.billing.covers(now) || c.relationship.state == RelationshipState::Blocked {
                return Err(Error::Unauthorized);
            }
            (
                c.conversation
                    .ok_or(Error::Unavailable)?
                    .grant_disclosure(request.persona, now)?,
                None,
                false,
                Outcome::DisclosureGranted,
            )
        }
        Command::Read { .. } => unreachable!(),
    };
    Ok((
        Changes {
            conversation,
            delivery,
            cancel_waiting,
        },
        outcome,
    ))
}

/// Recorded provenance is evidence, not an authorization credential.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncomingEvidence {
    pub authentication: cs_mail_adapters::VerifiedDomain,
    pub transport_digest: [u8; 32],
    pub message_digest: [u8; 32],
}
/// Admission input facts supplied by a trusted adapter, not an authentication proof.
/// The durable receiver entry point requires the gateway's sealed authenticated reply.
#[derive(Clone, Debug)]
pub struct IncomingMessage {
    pub manifest: MessageManifest,
    pub sender_key: EndpointPublicKey,
    pub member_copy: EncryptedContentRecord,
}
pub struct Reply {
    pub mailbox: GuestMailbox,
    pub conversation: ConversationId,
    pub message: IncomingMessage,
    /// Stable keyed fingerprint of the logical email, independent of ciphertext and retry time.
    pub delivery_digest: [u8; 32],
    pub evidence: IncomingEvidence,
}
/// # Errors
/// Blocks take precedence; pending access is limited to this exact conversation.
pub fn pending_or_standing(
    c: &ExternalConversation,
    r: &Relationship,
    reply: &Reply,
    covered: bool,
) -> Result<bool, Error> {
    if !covered
        || c.mailbox() != &reply.mailbox
        || c.id() != reply.conversation
        || r.key.sender != c.guest()
        || r.key.recipient != c.member()
        || r.state == RelationshipState::Blocked
        || reply.message.manifest.conversation() != c.id()
        || !reply.message.manifest.sources().is_empty()
    {
        return Err(Error::Unauthorized);
    }
    Ok(r.state == RelationshipState::Accepted || c.pending_access(r))
}

pub enum ReplyDestination<'a> {
    Address(crate::aliases::ReceivingAuthorization),
    Conversation(&'a cs_mail_accounts::aliases::Alias),
}
pub struct ReplyContext<'a> {
    pub destination: ReplyDestination<'a>,
    pub conversation: &'a ExternalConversation,
    pub relationship: &'a Relationship,
    pub lane: Option<cs_mail_capabilities::SenderScopedLane>,
    pub member_key: EndpointPublicKey,
    pub billing: &'a BillingAccount,
    pub provider: cs_mail_primitives::ProviderRef,
}

/// # Errors
/// Checks exact mailbox, conversation and content binding. Consumes a lane only
/// when neither standing nor pending access authorizes this message.
/// A received external message has no representation without its inbox assignment.
pub struct AdmittedReply {
    message: IncomingMessage,
    receiving: crate::aliases::DeliveredRoute,
    lane: Option<cs_mail_capabilities::SenderScopedLane>,
}
impl AdmittedReply {
    pub fn receiving(&self) -> &crate::aliases::DeliveredRoute {
        &self.receiving
    }
    pub fn into_parts(self) -> (Message, Option<cs_mail_capabilities::SenderScopedLane>) {
        (
            Message(MessageFields {
                manifest: self.message.manifest,
                sender_key: self.message.sender_key,
                member_copy: self.message.member_copy,
                committed_at: self.receiving.at(),
                direction: Direction::GuestToMember(Box::new(self.receiving)),
            }),
            self.lane,
        )
    }
}
/// # Errors
/// Refuses incomplete destination evidence or permission; publishes only a complete delivery.
pub fn admit_reply(
    context: ReplyContext<'_>,
    reply: &Reply,
    now: CanonicalTime,
) -> Result<AdmittedReply, Error> {
    let lane = authorize_reply(&context, reply, now)?;
    let receiving = match context.destination {
        ReplyDestination::Address(route) => route.delivered(
            reply.message.manifest.message(),
            crate::aliases::DeliveryTransport::External,
        )?,
        ReplyDestination::Conversation(alias) => {
            if alias.id() != context.conversation.receiving_alias()
                || alias.owner() != context.conversation.member()
            {
                return Err(Error::Unauthorized);
            }
            alias
                .check_conversation(context.conversation.guest(), context.conversation.id(), now)
                .map_err(|_| Error::Unauthorized)?;
            crate::aliases::DeliveredRoute::external_reply(
                alias,
                context.conversation.guest(),
                context.conversation.id(),
                reply.message.manifest.message(),
                now,
            )?
        }
    };
    Ok(AdmittedReply {
        message: reply.message.clone(),
        receiving,
        lane,
    })
}

fn authorize_reply(
    context: &ReplyContext<'_>,
    reply: &Reply,
    now: CanonicalTime,
) -> Result<Option<cs_mail_capabilities::SenderScopedLane>, Error> {
    let ReplyContext {
        destination,
        conversation: c,
        relationship: r,
        lane,
        member_key,
        billing,
        provider,
    } = context;
    let ordinary = pending_or_standing(c, r, reply, billing.covers(now))?;
    let authentication = &reply.evidence.authentication;
    if authentication.author_domain != *reply.mailbox.domain()
        || (authentication.dkim.is_none() && authentication.spf.is_none())
        || authentication.evaluated_at > now
        || now.0.saturating_sub(authentication.evaluated_at.0) > 300_000
    {
        return Err(Error::Unauthorized);
    }
    if reply
        .message
        .member_copy
        .binding
        .declarations
        .origin
        .authority
        != cs_mail_primitives::DeclarationAuthority::GuestSmtp(*provider)
    {
        return Err(Error::Unauthorized);
    }
    let m = &reply.message;
    validate_copy(&m.manifest, m.sender_key, &m.member_copy)?;
    let copy = &m.member_copy;
    if m.manifest.message().0 == 0
        || copy.binding.message_id != m.manifest.message()
        || copy.binding.sender != c.guest()
        || copy.binding.recipient != c.member()
        || copy.binding.relationship != r.key.reference
        || copy.envelope.recipient_key != member_key.reference
        || copy.envelope.sender_key != m.sender_key.reference
        || m.sender_key.bytes == [0; 32]
        || copy.created_at > now
        || copy.expires_at <= now
        || copy.binding.message_valid_until.0 <= now
        || copy.envelope.ciphertext.is_empty()
        || copy.envelope.ciphertext.len() > 65536
        || reply.delivery_digest == [0; 32]
    {
        return Err(Error::Invalid);
    }
    if let ReplyDestination::Address(route) = destination {
        if route.sender() != c.guest()
            || route.recipient() != c.member()
            || route.conversation() != c.id()
            || route.authorized_at() != now
        {
            return Err(Error::Unauthorized);
        }
        match route.basis() {
            crate::aliases::RouteBasis::AliasLane { .. }
            | crate::aliases::RouteBasis::Standing { .. } => return Ok(None),
            crate::aliases::RouteBasis::PairedSender { .. } => {
                return route
                    .consume_paired(m.manifest.message(), &copy.binding.declarations)
                    .map_err(|_| Error::Unauthorized);
            }
            crate::aliases::RouteBasis::SenderLane { .. } => (),
        }
    } else if ordinary {
        return Ok(None);
    }
    let mut lane = lane.clone().ok_or(Error::Unauthorized)?;
    let evidence = cs_mail_capabilities::LaneEvidence::GuestMailbox(c.guest());
    lane.authorizes(
        c.guest(),
        c.member(),
        copy.binding.capability,
        &copy.binding.declarations,
        &evidence,
        now,
    )
    .map_err(|_| Error::Unauthorized)?;
    if lane
        .consume(
            m.manifest.message(),
            copy.binding.capability,
            &copy.binding.declarations,
            &evidence,
            now,
        )
        .map_err(|_| Error::Unauthorized)?
    {
        return Err(Error::Conflict);
    }
    Ok(Some(lane))
}

/// This value can only be constructed after the application authorizes dispatch.
/// The storage adapter releases it only after the dispatch boundary commits.
pub enum ThreadRoot {
    Member(MessageId),
    Incoming(String),
}
pub struct Dispatch {
    thread_root: ThreadRoot,
    conversation: ExternalConversation,
    message: Message,
    relay_copy: EncryptedContentRecord,
}
impl Dispatch {
    pub const fn thread_root(&self) -> &ThreadRoot {
        &self.thread_root
    }
    pub fn conversation(&self) -> &ExternalConversation {
        &self.conversation
    }
    pub fn message(&self) -> &Message {
        &self.message
    }
    pub fn relay_copy(&self) -> &EncryptedContentRecord {
        &self.relay_copy
    }
}
pub struct DispatchContext {
    pub thread_root: ThreadRoot,
    pub conversation: ExternalConversation,
    pub relationship: Relationship,
    pub authority: AccountState,
    pub key: OperationalKeyRef,
    pub billing: BillingAccount,
    pub message: Message,
    pub relay_copy: EncryptedContentRecord,
    pub relay_key: EndpointPublicKey,
    pub state: DeliveryState,
}
/// # Errors
/// Already begun or terminal work cannot be dispatched again.
pub fn begin_dispatch(
    c: DispatchContext,
    now: CanonicalTime,
) -> Result<(DeliveryState, Option<Dispatch>), Error> {
    if c.state != DeliveryState::Waiting {
        return Err(Error::Conflict);
    }
    c.message.validate(&c.conversation, &c.relationship)?;
    validate_copy(&c.message.manifest, c.message.sender_key, &c.relay_copy)?;
    if c.relay_copy.envelope.recipient_key != c.relay_key.reference
        || c.message.direction != Direction::MemberToGuest
        || c.message.member_copy.binding != c.relay_copy.binding
    {
        return Err(Error::Invalid);
    }
    if !c.billing.covers(now)
        || !c.conversation.disclosure().permits_dispatch()
        || c.relationship.state == RelationshipState::Blocked
        || c.authority
            .persona_key(&c.authority.product, c.conversation.member(), c.key, now)
            .is_err()
        || c.relay_copy.expires_at <= now
        || c.relay_copy.binding.message_valid_until.0 <= now
    {
        return Ok((c.state.cancel(now), None));
    }
    Ok((
        c.state.begin(now)?,
        Some(Dispatch {
            thread_root: c.thread_root,
            conversation: c.conversation,
            message: c.message,
            relay_copy: c.relay_copy,
        }),
    ))
}

/// Authenticate provider evidence at the adapter, then bind it to protected state.
/// A callback can reconcile an uncertain handoff, never grant communication permission.
/// # Errors
/// Refuses mismatched routes/references or work that was never dispatched.
pub fn provider_feedback(
    conversation: &ExternalConversation,
    state: &DeliveryState,
    event: &cs_mail_correspondence::external::delivery::ProviderEvent,
    now: CanonicalTime,
) -> Result<DeliveryState, Error> {
    if event.sender != conversation.member() || &event.recipient != conversation.mailbox() {
        return Err(Error::Unauthorized);
    }
    state.finish(Some(event.reference.clone()), now)
}
