//! Authenticated alias management and deterministic receiving-route admission.
//! Management never invokes sender-lane acceptance or modifies financial state.
use crate::accounts::{AccountClock, operations::AccountState};
use cs_mail_accounts::alias_permission::{PermissionRevision, PermissionState};
use cs_mail_accounts::{
    aliases::{
        AdmittedConversation, Alias, AliasDescription, AliasError, AliasKind, AliasPortfolio,
        AliasUsage,
    },
    directory::MemberAddress,
};
use cs_mail_capabilities::{LaneEvidence, SenderScopedLane, routes::ReceivingScope};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{
    AccountId, AliasId, CanonicalTime, IdempotencyKey, MessageDeclarations, OperationalKeyRef,
    ProtocolIdentity,
};
use cs_mail_protocol::{Relationship, RelationshipState};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// A private request contains context metadata, never a member-chosen mailbox.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum AliasCreation {
    Private { description: AliasDescription },
    Public { address: MemberAddress },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ReplacementAddress {
    GeneratePrivate,
    Public { address: MemberAddress },
}
/// Infrastructure supplies entropy before the protected transaction; domain rules
/// receive the resulting candidate explicitly and never obtain randomness themselves.
pub trait PrivateAliasEntropy {
    /// # Errors
    /// Refuses unavailable secure randomness.
    fn sample(&self) -> Result<[u8; 16], AliasError>;
}
impl<F: Fn() -> Result<[u8; 16], AliasError>> PrivateAliasEntropy for F {
    fn sample(&self) -> Result<[u8; 16], AliasError> {
        self()
    }
}
fn private_address(domain: &str, entropy: [u8; 16]) -> Result<MemberAddress, AliasError> {
    let token = u128::from_be_bytes(entropy);
    format!("p{token:032x}@{domain}")
        .try_into()
        .map_err(|_| AliasError::Invalid)
}
impl AliasCreation {
    fn matches(&self, alias: &Alias) -> bool {
        match self {
            Self::Private { description } => {
                alias.description() == Some(description) && alias.kind() == AliasKind::Private
            }
            Self::Public { address } => {
                alias.kind() == AliasKind::Public && alias.address() == address
            }
        }
    }
}
impl ReplacementAddress {
    fn matches(&self, alias: &Alias) -> bool {
        match self {
            Self::GeneratePrivate => alias.kind() == AliasKind::Private,
            Self::Public { address } => {
                alias.kind() == AliasKind::Public && alias.address() == address
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Inspect,
    Permission {
        id: AliasId,
        expected_address: u64,
        expected_permission: PermissionRevision,
        action: PermissionAction,
    },
    Create {
        id: AliasId,
        creation: AliasCreation,
    },
    Restrict {
        id: AliasId,
        expected: u64,
    },
    Replace {
        id: AliasId,
        expected: u64,
        replacement: AliasId,
        address: ReplacementAddress,
        until: CanonicalTime,
    },
    Retire {
        id: AliasId,
        expected: u64,
    },
    ApproveConversation {
        id: AliasId,
        expected: u64,
        sender: ProtocolIdentity,
        conversation: ConversationId,
    },
    /// Explicit receiving scope for already established sender permission. This
    /// changes neither the relationship nor any pending-request settlement.
    ScopeSender {
        sender: ProtocolIdentity,
        expected: u64,
        receiving: ReceivingScope,
    },
}
/// The outer signed command binds the named receiving alias and exact permission change.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum PermissionAction {
    Open,
    Restrict,
    Remove,
    Senders(Vec<cs_mail_capabilities::SignedLaneGrant>),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCommand {
    pub scope: cs_mail_security::RecipientSigningScope,
    pub product: String,
    pub account: AccountId,
    pub owner: ProtocolIdentity,
    pub key: OperationalKeyRef,
    pub operation: IdempotencyKey,
    pub command: Command,
    pub signature: Vec<u8>,
}
impl SignedCommand {
    fn bytes(&self) -> Result<Vec<u8>, AliasError> {
        if self.product.is_empty()
            || self.account.0 == 0
            || self.owner.0 == 0
            || self.key.0 == 0
            || self.operation.0 == 0
        {
            return Err(AliasError::Invalid);
        }
        serde_json::to_vec(&(
            "cs-mail/alias-command/v3",
            self.scope,
            &self.product,
            self.account,
            self.owner,
            self.key,
            self.operation,
            &self.command,
        ))
        .map_err(|_| AliasError::Invalid)
    }
    /// # Errors
    /// Rejects invalid command context.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, AliasError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects changed signatures, scope or targets.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), AliasError> {
        VerifyingKey::from_bytes(key)
            .map_err(|_| AliasError::Permission)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| AliasError::Permission)?,
            )
            .map_err(|_| AliasError::Permission)
    }
    /// # Errors
    /// Rejects invalid command serialization.
    pub fn digest(&self) -> Result<[u8; 32], AliasError> {
        Ok(Sha256::digest(serde_json::to_vec(self).map_err(|_| AliasError::Invalid)?).into())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "SenderScopeRecord", into = "SenderScopeRecord")]
pub struct SenderRouteScope(SenderScopeRecord);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SenderScopeRecord {
    pub revision: u64,
    pub established_at: CanonicalTime,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub relationship_version: cs_mail_primitives::RelationshipVersion,
    pub receiving: ReceivingScope,
    pub lane: Option<(cs_mail_primitives::LaneId, cs_mail_primitives::Version)>,
}
impl TryFrom<SenderScopeRecord> for SenderRouteScope {
    type Error = AliasError;
    fn try_from(value: SenderScopeRecord) -> Result<Self, Self::Error> {
        if value.revision == 0
            || value.sender.0 == 0
            || value.recipient.0 == 0
            || value.sender == value.recipient
            || value.lane.is_some_and(|(id, _)| id.0 == 0)
        {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<SenderRouteScope> for SenderScopeRecord {
    fn from(value: SenderRouteScope) -> Self {
        value.0
    }
}
impl SenderRouteScope {
    pub const fn sender(&self) -> ProtocolIdentity {
        self.0.sender
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.0.recipient
    }
    pub const fn revision(&self) -> u64 {
        self.0.revision
    }
    pub const fn receiving(&self) -> &ReceivingScope {
        &self.0.receiving
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Outcome {
    Aliases(Vec<Alias>),
    Changed(Vec<Alias>),
    Scoped(SenderRouteScope),
    Refused(AliasError),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub digest: [u8; 32],
    pub at: CanonicalTime,
    pub outcome: Outcome,
}
/// The host signs a committed result; the client checks it before clearing work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgement {
    pub receipt: Receipt,
    pub signature: Vec<u8>,
}
impl Acknowledgement {
    fn bytes(&self) -> Result<Vec<u8>, AliasError> {
        serde_json::to_vec(&("cs-mail/alias-acknowledgement/v3", &self.receipt))
            .map_err(|_| AliasError::Invalid)
    }
    /// # Errors
    /// Refuses unencodable results.
    pub fn sign(receipt: Receipt, secret: &[u8; 32]) -> Result<Self, AliasError> {
        let mut result = Self {
            receipt,
            signature: vec![],
        };
        result.signature = SigningKey::from_bytes(secret)
            .sign(&result.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(result)
    }
    /// # Errors
    /// Refuses altered provider evidence or a result for another operation.
    pub fn verify(&self, request: &SignedCommand, provider: &[u8; 32]) -> Result<(), AliasError> {
        if self.receipt.digest != request.digest()? || !self.receipt.matches(request) {
            return Err(AliasError::Invalid);
        }
        VerifyingKey::from_bytes(provider)
            .map_err(|_| AliasError::Permission)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| AliasError::Permission)?,
            )
            .map_err(|_| AliasError::Permission)
    }
}
pub struct Context {
    pub authority: AccountState,
    /// Original enrolled participant. Additional legacy personas are not new aliases.
    pub participant: ProtocolIdentity,
    pub aliases: BTreeMap<AliasId, Alias>,
    /// Includes permanently reserved retired addresses and the existing directory.
    pub reserved_addresses: BTreeSet<String>,
    pub reserved_ids: BTreeSet<AliasId>,
    pub existing: Vec<AdmittedConversation>,
    pub relationship: Option<Relationship>,
    pub lane: Option<SenderScopedLane>,
    pub prior: Option<Receipt>,
    pub sender_scope: Option<SenderRouteScope>,
}
pub struct Effects {
    aliases: Vec<Alias>,
    scope: Option<SenderRouteScope>,
    receipt: Receipt,
}
impl Effects {
    pub fn into_parts(self) -> (Vec<Alias>, Option<SenderRouteScope>, Receipt) {
        (self.aliases, self.scope, self.receipt)
    }
}
pub struct Decision {
    receipt: Receipt,
    effects: Option<Effects>,
}
impl Decision {
    pub fn into_parts(self) -> (Receipt, Option<Effects>) {
        (self.receipt, self.effects)
    }
}
pub trait Store {
    type Error: From<AliasError>;
    /// Serialize route creation, lifecycle, admitted-conversation evidence, caller
    /// authority, sender blocks and retired-address reservations. Invoke once and
    /// atomically persist ALL declared aliases, receiving scopes and the receipt.
    /// No external I/O under protection. Failure rolls back the entire decision.
    /// # Errors
    /// Refuses storage faults and incomplete protected context.
    fn transact_alias(
        &self,
        request: &SignedCommand,
        decide: impl FnOnce(Context, &dyn AccountClock) -> Result<Decision, Self::Error>,
    ) -> Result<Receipt, Self::Error>;
}
pub struct AliasService<'a, S>(pub &'a S);
impl<S: Store> AliasService<'_, S> {
    /// # Errors
    /// Refuses invalid authority, altered retries or storage faults.
    pub fn handle(
        &self,
        request: &SignedCommand,
        scope: cs_mail_security::RecipientSigningScope,
        domain: &str,
        entropy: &impl PrivateAliasEntropy,
    ) -> Result<Receipt, S::Error> {
        let generated = if matches!(
            request.command,
            Command::Create {
                creation: AliasCreation::Private { .. },
                ..
            } | Command::Replace {
                address: ReplacementAddress::GeneratePrivate,
                ..
            }
        ) {
            Some(private_address(domain, entropy.sample()?)?)
        } else {
            None
        };
        self.0.transact_alias(request, |context, clock| {
            decide(
                request,
                &context,
                scope,
                domain,
                generated.as_ref(),
                clock.now(),
            )
            .map_err(Into::into)
        })
    }
}
fn decide(
    request: &SignedCommand,
    context: &Context,
    scope: cs_mail_security::RecipientSigningScope,
    domain: &str,
    generated: Option<&MemberAddress>,
    at: CanonicalTime,
) -> Result<Decision, AliasError> {
    if request.scope != scope || context.participant != request.owner {
        return Err(AliasError::Permission);
    }
    let key = context
        .authority
        .persona_key(&request.product, request.owner, request.key, at)
        .map_err(|_| AliasError::Permission)?;
    context
        .authority
        .registry
        .active_verifying_key(
            request.key,
            cs_mail_protocol::ActorRef::Recipient(request.owner),
            at,
        )
        .map_err(|_| AliasError::Permission)?;
    request.verify(&key)?;
    let digest = request.digest()?;
    if let Some(prior) = &context.prior {
        if prior.digest != digest || !prior.matches(request) {
            return Err(AliasError::Conflict);
        }
        return Ok(Decision {
            receipt: prior.clone(),
            effects: None,
        });
    }
    AliasPortfolio::new(request.owner, &context.aliases)?;
    if matches!(request.command, Command::Inspect) {
        return Ok(Decision {
            receipt: Receipt {
                digest,
                at,
                outcome: Outcome::Aliases(context.aliases.values().cloned().collect()),
            },
            effects: None,
        });
    }
    let (aliases, scope, outcome) = match mutate(request, context, domain, generated, at) {
        Ok((aliases, scope)) => {
            let outcome = scope.as_ref().map_or_else(
                || Outcome::Changed(aliases.clone()),
                |scope| Outcome::Scoped(scope.clone()),
            );
            (aliases, scope, outcome)
        }
        Err(error) => (vec![], None, Outcome::Refused(error)),
    };
    let receipt = Receipt {
        digest,
        at,
        outcome,
    };
    Ok(Decision {
        receipt: receipt.clone(),
        effects: Some(Effects {
            aliases,
            scope,
            receipt,
        }),
    })
}
fn fresh(
    context: &Context,
    id: AliasId,
    address: &MemberAddress,
    domain: &str,
) -> Result<(), AliasError> {
    if address.domain() != domain {
        return Err(AliasError::Invalid);
    }
    if context.reserved_ids.contains(&id) || context.reserved_addresses.contains(address.as_str()) {
        return Err(AliasError::Conflict);
    }
    Ok(())
}
fn permission_change(
    request: &SignedCommand,
    c: &Context,
    id: &AliasId,
    expected_address: u64,
    expected_permission: PermissionRevision,
    action: &PermissionAction,
    at: CanonicalTime,
) -> Result<Alias, AliasError> {
    let alias = c.aliases.get(id).ok_or(AliasError::Permission)?;
    if alias.revision() != expected_address {
        return Err(AliasError::Conflict);
    }
    if !matches!(
        alias.lifecycle(),
        cs_mail_accounts::aliases::AliasLifecycle::Active
            | cs_mail_accounts::aliases::AliasLifecycle::Restricted { .. }
    ) || alias.kind() != AliasKind::Private
    {
        return Err(AliasError::Retired);
    }
    let permission = alias.permission();
    let next = match action {
        PermissionAction::Open => permission.open(expected_permission, at)?,
        PermissionAction::Restrict => permission.restrict(
            expected_permission,
            c.existing
                .iter()
                .filter(|conversation| conversation.alias() == *id)
                .cloned()
                .collect(),
            at,
        )?,
        PermissionAction::Remove => permission.remove(expected_permission, at)?,
        PermissionAction::Senders(grants) => {
            if grants.is_empty() || grants.len() > 32 {
                return Err(AliasError::Invalid);
            }
            let mut lanes = Vec::new();
            for signed in grants {
                let g = &signed.grant;
                if g.protocol_version != cs_mail_primitives::ProtocolVersion(2)
                    || g.recipient != request.owner
                    || g.recipient_operational_key != request.key
                    || g.deployment_domain != request.scope.deployment_domain
                    || g.intended_provider != request.scope.provider
                    || g.issued_at > at
                    || g.sender == request.owner
                {
                    return Err(AliasError::Permission);
                }
                if let cs_mail_capabilities::LaneSubject::LegacyDomain(domain) = &g.subject {
                    if domain.synthetic_protocol_identity(&request.scope.deployment_domain, 1)
                        != g.sender
                    {
                        return Err(AliasError::Invalid);
                    }
                }
                let key = c
                    .authority
                    .persona_key(&request.product, request.owner, request.key, at)
                    .map_err(|_| AliasError::Permission)?;
                let lane = cs_mail_capabilities::VerifiedLaneGrant::verify(signed, &key)
                    .and_then(cs_mail_capabilities::VerifiedLaneGrant::activate)
                    .map_err(|_| AliasError::Permission)?;
                if !lane.is_current(at) {
                    return Err(AliasError::Permission);
                }
                lanes.push(lane);
            }
            permission.senders(expected_permission, lanes, at)?
        }
    };
    alias.with_permission(next)
}

#[allow(clippy::too_many_lines)] // One exhaustive dispatch of the authenticated alias command.
fn mutate(
    request: &SignedCommand,
    c: &Context,
    domain: &str,
    generated: Option<&MemberAddress>,
    at: CanonicalTime,
) -> Result<(Vec<Alias>, Option<SenderRouteScope>), AliasError> {
    let owned = |id: AliasId| c.aliases.get(&id).ok_or(AliasError::Permission);
    let existing = |id| {
        c.existing
            .iter()
            .filter(|conversation| conversation.alias() == id)
            .cloned()
            .collect()
    };
    let aliases = match &request.command {
        Command::Inspect => return Err(AliasError::Invalid),
        Command::Permission {
            id,
            expected_address,
            expected_permission,
            action,
        } => vec![permission_change(
            request,
            c,
            id,
            *expected_address,
            *expected_permission,
            action,
            at,
        )?],
        Command::Create { id, creation } => {
            let (address, usage) = match creation {
                AliasCreation::Private { description } => (
                    generated.ok_or(AliasError::Invalid)?.clone(),
                    AliasUsage::Private {
                        description: description.clone(),
                    },
                ),
                AliasCreation::Public { address } => (address.clone(), AliasUsage::Public),
            };
            fresh(c, *id, &address, domain)?;
            vec![AliasPortfolio::new(request.owner, &c.aliases)?.create(*id, address, usage, at)?]
        }
        Command::Restrict { id, expected } => {
            vec![owned(*id)?.restrict(*expected, existing(*id), at)?]
        }
        Command::Replace {
            id,
            expected,
            replacement,
            address,
            until,
        } => {
            let old = owned(*id)?;
            let address = match (old.kind(), address) {
                (AliasKind::Private, ReplacementAddress::GeneratePrivate) => {
                    generated.ok_or(AliasError::Invalid)?.clone()
                }
                (AliasKind::Public, ReplacementAddress::Public { address }) => address.clone(),
                _ => return Err(AliasError::Invalid),
            };
            fresh(c, *replacement, &address, domain)?;
            let new = Alias::create(
                *replacement,
                request.owner,
                address,
                old.usage().clone(),
                at,
            )?;
            vec![
                old.replace(*expected, &new, *until, existing(*id), at)?,
                new,
            ]
        }
        Command::Retire { id, expected } => vec![owned(*id)?.retire(*expected, at)?],
        Command::ApproveConversation {
            id,
            expected,
            sender,
            conversation,
        } => {
            if c.relationship
                .as_ref()
                .is_some_and(|r| r.key.sender != *sender || r.key.recipient != request.owner)
            {
                return Err(AliasError::Invalid);
            }
            if c.relationship
                .as_ref()
                .is_some_and(|r| r.state == RelationshipState::Blocked)
            {
                return Err(AliasError::Blocked);
            }
            vec![owned(*id)?.approve_conversation(*expected, *sender, *conversation, at)?]
        }
        Command::ScopeSender {
            sender,
            expected,
            receiving,
        } => {
            return Ok((
                vec![],
                Some(scope_sender(
                    c,
                    request.owner,
                    *sender,
                    *expected,
                    receiving,
                    at,
                )?),
            ));
        }
    };
    Ok((aliases, None))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SenderAuthentication {
    Authenticated(ProtocolIdentity),
    Uncertain,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReceivingProvenance {
    AliasPermission(RouteBasis),
    Native {
        admission: cs_mail_protocol::AdmissionBasis,
    },
    Request {
        request: cs_mail_primitives::RequestId,
    },
    ConversationReply,
    /// Explicit fixture assignment, never historical receiving authority.
    DevelopmentAssignment {
        operation: cs_mail_primitives::IdempotencyKey,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RouteBasis {
    AliasLane {
        permission_revision: PermissionRevision,
    },
    PairedSender {
        id: cs_mail_primitives::LaneId,
        version: cs_mail_primitives::Version,
        permission_revision: PermissionRevision,
    },
    Standing {
        version: cs_mail_primitives::RelationshipVersion,
    },
    SenderLane {
        id: cs_mail_primitives::LaneId,
        version: cs_mail_primitives::Version,
    },
}
/// Current route permission, not evidence that a message was delivered. It is
/// constructed only by the receiving decision and cannot be restored as authority.
#[derive(Debug, Eq, PartialEq)]
pub struct ReceivingAuthorization {
    alias: AliasId,
    alias_revision: u64,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
    conversation: ConversationId,
    authorized_at: CanonicalTime,
    basis: RouteBasis,
    paired: Option<Box<(SenderScopedLane, LaneEvidence)>>,
}
#[derive(Debug, Eq, PartialEq)]
pub enum RouteDecision {
    Authorize(ReceivingAuthorization),
    Hold,
    Request,
    Refuse(AliasError),
}
/// Protected facts only. The adapter must load the identity pair even when a new
/// sender has never acquired an alias-specific grant. Missing pair is protected absence.
pub struct AdmissionContext<'a> {
    pub alias: &'a Alias,
    pub authentication: SenderAuthentication,
    pub relationship: Option<&'a Relationship>,
    pub sender_scope: Option<&'a SenderRouteScope>,
    pub lane: Option<&'a SenderScopedLane>,
    pub declarations: &'a MessageDeclarations,
    pub lane_evidence: Option<&'a LaneEvidence>,
    pub domain_evidence: Option<&'a cs_mail_adapters::VerifiedDomain>,
    pub covered: bool,
    /// Policy interpretation is separate from authenticated sender evidence.
    pub review_required: bool,
    pub conversation: ConversationId,
}
/// Deterministic routing; no payment fallback exists for a private alias. The
/// returned permission must produce delivery evidence only after full message validation,
/// committed with ciphertext, delivery, allowance changes and the operation receipt.
pub fn admit(c: &AdmissionContext<'_>, at: CanonicalTime) -> RouteDecision {
    match admission(c, at) {
        Ok(result) => result,
        Err(error) => RouteDecision::Refuse(error),
    }
}
fn admission(c: &AdmissionContext<'_>, at: CanonicalTime) -> Result<RouteDecision, AliasError> {
    c.declarations.validate().map_err(|_| AliasError::Invalid)?;
    if !c.covered {
        return Err(AliasError::Coverage);
    }
    // Even uncertain mail cannot bypass a known blocked identity. Conversely,
    // absence of authenticated identity never establishes unblocked authority.
    if c.relationship
        .is_some_and(|r| r.key.recipient != c.alias.owner())
    {
        return Err(AliasError::Invalid);
    }
    if c.relationship
        .is_some_and(|r| r.state == RelationshipState::Blocked)
    {
        return Err(AliasError::Blocked);
    }
    let SenderAuthentication::Authenticated(sender) = c.authentication else {
        if matches!(
            c.alias.lifecycle(),
            cs_mail_accounts::aliases::AliasLifecycle::Active
        ) && c.alias.kind() == AliasKind::Private
        {
            return Ok(RouteDecision::Hold);
        }
        return Err(AliasError::Authentication);
    };
    if sender.0 == 0
        || sender == c.alias.owner()
        || c.relationship.is_some_and(|r| r.key.sender != sender)
    {
        return Err(AliasError::Invalid);
    }
    c.alias.check_conversation(sender, c.conversation, at)?;
    if c.review_required {
        return if c.alias.kind() == AliasKind::Private {
            Ok(RouteDecision::Hold)
        } else {
            Err(AliasError::Authentication)
        };
    }
    let mut paired = None;
    let permission = c.alias.permission();
    let basis = if permission.authorizes_alias(sender, c.conversation, at) {
        RouteBasis::AliasLane {
            permission_revision: permission.revision(),
        }
    } else if let Some((lane, evidence)) = paired_sender(c, sender, at) {
        let basis = RouteBasis::PairedSender {
            id: lane.grant().id,
            version: lane.version(),
            permission_revision: permission.revision(),
        };
        paired = Some(Box::new((lane, evidence)));
        basis
    } else {
        let Some(basis) = relationship_basis(c, sender, at)? else {
            return no_permission(c.alias.kind());
        };
        basis
    };
    Ok(RouteDecision::Authorize(ReceivingAuthorization {
        alias: c.alias.id(),
        alias_revision: c.alias.revision(),
        sender,
        recipient: c.alias.owner(),
        conversation: c.conversation,
        authorized_at: at,
        basis,
        paired,
    }))
}

fn relationship_basis(
    c: &AdmissionContext<'_>,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Result<Option<RouteBasis>, AliasError> {
    let scope = c.sender_scope.filter(|scope| {
        scope.0.sender == sender
            && scope.0.recipient == c.alias.owner()
            && scope.0.receiving.contains(c.alias.id())
    });
    let Some((r, scope)) = c.relationship.zip(scope) else {
        return Ok(None);
    };
    if r.version != scope.0.relationship_version {
        return Ok(None);
    }
    Ok(Some(match r.state {
        RelationshipState::Accepted if scope.0.lane.is_none() => {
            RouteBasis::Standing { version: r.version }
        }
        RelationshipState::ExpressLane(id) => {
            let lane = c
                .lane
                .filter(|lane| {
                    scope.0.lane == Some((id, lane.grant().version)) && lane.grant().id == id
                })
                .ok_or(AliasError::Permission)?;
            lane.authorizes(
                sender,
                c.alias.owner(),
                Some(id),
                c.declarations,
                c.lane_evidence.ok_or(AliasError::Permission)?,
                at,
            )
            .map_err(|_| AliasError::Permission)?;
            RouteBasis::SenderLane {
                id,
                version: lane.version(),
            }
        }
        RelationshipState::Blocked => return Err(AliasError::Blocked),
        _ => return Ok(None),
    }))
}

fn no_permission(kind: AliasKind) -> Result<RouteDecision, AliasError> {
    if kind == AliasKind::Private {
        Err(AliasError::Permission)
    } else {
        Ok(RouteDecision::Request)
    }
}
fn paired_sender(
    c: &AdmissionContext<'_>,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Option<(SenderScopedLane, LaneEvidence)> {
    let PermissionState::Senders(lanes) = c.alias.permission().state() else {
        return None;
    };
    for lane in lanes {
        let g = lane.grant();
        let evidence = match &g.subject {
            cs_mail_capabilities::LaneSubject::LegacyDomain(domain) => {
                let Some(verified) = c.domain_evidence else {
                    continue;
                };
                if verified.evaluated_at > at
                    || at.0.saturating_sub(verified.evaluated_at.0) > 300_000
                {
                    continue;
                }
                let Ok(bound) =
                    cs_mail_adapters::LegacyDmarcEvidence::new(domain.clone(), verified.clone())
                else {
                    continue;
                };
                LaneEvidence::Legacy(bound)
            }
            _ if g.sender == sender => match c.lane_evidence {
                Some(e) => e.clone(),
                None => continue,
            },
            _ => continue,
        };
        if lane
            .authorizes(
                g.sender,
                c.alias.owner(),
                Some(g.id),
                c.declarations,
                &evidence,
                at,
            )
            .is_ok()
        {
            return Some((lane.clone(), evidence));
        }
    }
    None
}

fn matches_permission(
    aliases: &[Alias],
    id: &AliasId,
    expected_address: u64,
    expected_permission: PermissionRevision,
    action: &PermissionAction,
    at: CanonicalTime,
) -> bool {
    aliases.len() == 1
        && aliases[0].id() == *id
        && aliases[0].revision() == expected_address
        && Some(aliases[0].permission().revision()) == expected_permission.checked_next()
        && aliases[0].permission().changed_at() == at
        && match (action, aliases[0].permission().state()) {
            (PermissionAction::Open, PermissionState::Broad(g)) => g.established_at() == at,
            (PermissionAction::Restrict, PermissionState::Restricted { .. })
            | (PermissionAction::Remove, PermissionState::None) => true,
            (PermissionAction::Senders(grants), PermissionState::Senders(lanes)) => {
                grants.len() == lanes.len()
                    && grants.iter().zip(lanes).all(|(g, l)| &g.grant == l.grant())
            }
            _ => false,
        }
}

impl Receipt {
    /// Historical outcome shape must describe this exact signed command. It is
    /// never a current permission check or authority to deliver another message.
    #[allow(clippy::too_many_lines)] // Keep exact command/outcome bindings together for review.
    pub fn matches(&self, request: &SignedCommand) -> bool {
        let owned = |aliases: &[Alias]| {
            aliases.iter().all(|a| a.owner() == request.owner)
                && aliases.iter().map(Alias::id).collect::<BTreeSet<_>>().len() == aliases.len()
        };
        match (&request.command, &self.outcome) {
            (Command::Inspect, Outcome::Aliases(aliases)) => owned(aliases),
            (
                Command::Permission {
                    id,
                    expected_address,
                    expected_permission,
                    action,
                },
                Outcome::Changed(aliases),
            ) => {
                owned(aliases)
                    && matches_permission(
                        aliases,
                        id,
                        *expected_address,
                        *expected_permission,
                        action,
                        self.at,
                    )
            }
            (Command::Create { id, creation }, Outcome::Changed(aliases)) => {
                owned(aliases)
                    && aliases.len() == 1
                    && aliases[0].id() == *id
                    && creation.matches(&aliases[0])
                    && aliases[0].revision() == 0
                    && aliases[0].created_at() == self.at
                    && aliases[0].permission().revision().value() == 0
            }
            (Command::Restrict { id, expected }, Outcome::Changed(aliases)) => {
                owned(aliases)
                    && aliases.len() == 1
                    && aliases[0].id() == *id
                    && Some(aliases[0].revision()) == expected.checked_add(1)
                    && aliases[0].changed_at() == self.at
                    && matches!(aliases[0].lifecycle(), cs_mail_accounts::aliases::AliasLifecycle::Restricted { at, .. } if *at == self.at)
            }
            (
                Command::Replace {
                    id,
                    expected,
                    replacement,
                    address,
                    until,
                },
                Outcome::Changed(aliases),
            ) => {
                if !owned(aliases) || aliases.len() != 2 {
                    return false;
                }
                let old = aliases.iter().find(|a| a.id() == *id);
                let new = aliases.iter().find(|a| a.id() == *replacement);
                match (old, new) {
                    (Some(old), Some(new)) => {
                        Some(old.revision()) == expected.checked_add(1)
                            && new.revision() == 0
                            && new.created_at() == self.at
                            && new.permission().revision().value() == 0
                            && address.matches(new)
                            && new.usage() == old.usage()
                            && matches!(old.lifecycle(), cs_mail_accounts::aliases::AliasLifecycle::Replaced { at, replacement: target, until: deadline, .. } if *at == self.at && target == replacement && deadline == until)
                    }
                    _ => false,
                }
            }
            (Command::Retire { id, expected }, Outcome::Changed(aliases)) => {
                owned(aliases)
                    && aliases.len() == 1
                    && aliases[0].id() == *id
                    && Some(aliases[0].revision()) == expected.checked_add(1)
                    && matches!(aliases[0].lifecycle(), cs_mail_accounts::aliases::AliasLifecycle::Retired { at } if *at == self.at)
            }
            (
                Command::ScopeSender {
                    sender,
                    expected,
                    receiving,
                },
                Outcome::Scoped(scope),
            ) => {
                scope.0.sender == *sender
                    && scope.0.recipient == request.owner
                    && Some(scope.revision()) == expected.checked_add(1)
                    && &scope.0.receiving == receiving
                    && scope.0.established_at == self.at
            }
            (
                Command::ApproveConversation {
                    id,
                    expected,
                    sender,
                    conversation,
                },
                Outcome::Changed(aliases),
            ) => {
                owned(aliases)
                    && aliases.len() == 1
                    && aliases[0].id() == *id
                    && Some(aliases[0].revision()) == expected.checked_add(1)
                    && aliases[0].changed_at() == self.at
                    && matches!(aliases[0].lifecycle(), cs_mail_accounts::aliases::AliasLifecycle::Restricted { approvals, .. } if approvals.get(conversation).is_some_and(|approval| approval.sender() == *sender && approval.at() == self.at))
            }
            (Command::Inspect, _) => false,
            (_, Outcome::Refused(_)) => true,
            _ => false,
        }
    }
}

fn scope_sender(
    c: &Context,
    owner: ProtocolIdentity,
    sender: ProtocolIdentity,
    expected: u64,
    receiving: &ReceivingScope,
    at: CanonicalTime,
) -> Result<SenderRouteScope, AliasError> {
    if c.sender_scope
        .as_ref()
        .map_or(0, SenderRouteScope::revision)
        != expected
    {
        return Err(AliasError::Conflict);
    }
    if sender == owner
        || receiving
            .aliases()
            .iter()
            .any(|id| !c.aliases.contains_key(id))
    {
        return Err(AliasError::Permission);
    }
    let r = c.relationship.as_ref().ok_or(AliasError::Permission)?;
    if r.key.sender != sender || r.key.recipient != owner {
        return Err(AliasError::Invalid);
    }
    let lane = match r.state {
        RelationshipState::Accepted => None,
        RelationshipState::ExpressLane(id) => {
            let lane = c
                .lane
                .as_ref()
                .filter(|lane| {
                    lane.grant().id == id
                        && lane.grant().sender == sender
                        && lane.grant().recipient == owner
                        && lane.is_current(at)
                })
                .ok_or(AliasError::Permission)?;
            Some((id, lane.grant().version))
        }
        RelationshipState::Blocked => return Err(AliasError::Blocked),
        _ => return Err(AliasError::Permission),
    };
    SenderScopeRecord {
        revision: expected.checked_add(1).ok_or(AliasError::Invalid)?,
        established_at: at,
        sender,
        recipient: owner,
        relationship_version: r.version,
        receiving: receiving.clone(),
        lane,
    }
    .try_into()
}

impl ReceivingAuthorization {
    /// No allowance effects are inferred by storage. Consumption belongs to this decision.
    /// # Errors
    /// Rejects unavailable allowance, altered scope and duplicate consumption.
    pub fn consume_paired(
        &self,
        message: cs_mail_primitives::MessageId,
        declarations: &MessageDeclarations,
    ) -> Result<Option<SenderScopedLane>, AliasError> {
        let Some(pair) = &self.paired else {
            return Ok(None);
        };
        let (lane, evidence) = pair.as_ref();
        let mut next = lane.clone();
        if next
            .consume(
                message,
                Some(lane.grant().id),
                declarations,
                evidence,
                self.authorized_at,
            )
            .map_err(|_| AliasError::Permission)?
        {
            return Err(AliasError::Conflict);
        }
        Ok(Some(next))
    }
    pub const fn alias(&self) -> AliasId {
        self.alias
    }
    pub const fn alias_revision(&self) -> u64 {
        self.alias_revision
    }
    pub const fn sender(&self) -> ProtocolIdentity {
        self.sender
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.recipient
    }
    pub const fn conversation(&self) -> ConversationId {
        self.conversation
    }
    pub const fn authorized_at(&self) -> CanonicalTime {
        self.authorized_at
    }
    pub const fn basis(&self) -> &RouteBasis {
        &self.basis
    }
}

/// Historical receiving evidence. Its existence does not authorize another delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeliveryTransport {
    Native,
    External,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "DeliveredRouteRecord", into = "DeliveredRouteRecord")]
pub struct DeliveredRoute(DeliveredRouteRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveredRouteRecord {
    transport: DeliveryTransport,
    message: cs_mail_primitives::MessageId,
    alias: AliasId,
    alias_revision: u64,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
    conversation: ConversationId,
    at: CanonicalTime,
    provenance: ReceivingProvenance,
}
impl TryFrom<DeliveredRouteRecord> for DeliveredRoute {
    type Error = cs_mail_correspondence::CorrespondenceError;
    fn try_from(r: DeliveredRouteRecord) -> Result<Self, Self::Error> {
        if r.message.0 == 0
            || r.sender.0 == 0
            || r.recipient.0 == 0
            || r.sender == r.recipient
            || matches!(r.provenance, ReceivingProvenance::AliasPermission(RouteBasis::SenderLane { id, .. } | RouteBasis::PairedSender { id, .. }) if id.0 == 0)
            || matches!(r.provenance, ReceivingProvenance::Request { request } if request.0 == 0)
            || matches!(r.provenance, ReceivingProvenance::DevelopmentAssignment { operation } if operation.0 == 0)
        {
            return Err(Self::Error::Invalid);
        }
        Ok(Self(r))
    }
}
impl From<DeliveredRoute> for DeliveredRouteRecord {
    fn from(r: DeliveredRoute) -> Self {
        r.0
    }
}
impl ReceivingAuthorization {
    /// Construct only after validating the complete message. The delivery store
    /// must commit this fact atomically with that message and allowance effects.
    /// # Errors
    /// Rejects a zero message identity or inconsistent participant binding.
    pub fn delivered(
        self,
        message: cs_mail_primitives::MessageId,
        transport: DeliveryTransport,
    ) -> Result<DeliveredRoute, cs_mail_correspondence::CorrespondenceError> {
        DeliveredRouteRecord {
            transport,
            message,
            alias: self.alias,
            alias_revision: self.alias_revision,
            sender: self.sender,
            recipient: self.recipient,
            conversation: self.conversation,
            at: self.authorized_at,
            provenance: ReceivingProvenance::AliasPermission(self.basis),
        }
        .try_into()
    }
}
impl DeliveredRoute {
    /// # Errors
    /// Rejects an assignment that is not a native delivery between distinct participants.
    pub fn native(
        alias: &Alias,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        message: cs_mail_primitives::MessageId,
        admission: cs_mail_protocol::AdmissionBasis,
        at: CanonicalTime,
    ) -> Result<Self, cs_mail_correspondence::CorrespondenceError> {
        DeliveredRouteRecord {
            transport: DeliveryTransport::Native,
            message,
            alias: alias.id(),
            alias_revision: alias.revision(),
            sender,
            recipient: alias.owner(),
            conversation,
            at,
            provenance: ReceivingProvenance::Native { admission },
        }
        .try_into()
    }
    /// # Errors
    /// Only a staged request document and its admitted recipient establish this assignment.
    pub(crate) fn request(
        alias: &Alias,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        message: cs_mail_primitives::MessageId,
        request: cs_mail_primitives::RequestId,
        at: CanonicalTime,
    ) -> Result<Self, cs_mail_correspondence::CorrespondenceError> {
        DeliveredRouteRecord {
            transport: DeliveryTransport::Native,
            message,
            alias: alias.id(),
            alias_revision: alias.revision(),
            sender,
            recipient: alias.owner(),
            conversation,
            at,
            provenance: ReceivingProvenance::Request { request },
        }
        .try_into()
    }
    /// # Errors
    /// Records an authenticated conversation reply after its separate permission decision.
    pub fn external_reply(
        alias: &Alias,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        message: cs_mail_primitives::MessageId,
        at: CanonicalTime,
    ) -> Result<Self, cs_mail_correspondence::CorrespondenceError> {
        DeliveredRouteRecord {
            transport: DeliveryTransport::External,
            message,
            alias: alias.id(),
            alias_revision: alias.revision(),
            sender,
            recipient: alias.owner(),
            conversation,
            at,
            provenance: ReceivingProvenance::ConversationReply,
        }
        .try_into()
    }
    /// Explicit offline fixture correction, never historical receiving authority.
    /// # Errors
    /// Rejects a missing operation or inconsistent participant assignment.
    #[cfg(feature = "development-maintenance")]
    pub fn development_assignment(
        alias: &Alias,
        sender: ProtocolIdentity,
        conversation: ConversationId,
        message: cs_mail_primitives::MessageId,
        at: CanonicalTime,
        operation: cs_mail_primitives::IdempotencyKey,
    ) -> Result<Self, cs_mail_correspondence::CorrespondenceError> {
        if operation.0 == 0 {
            return Err(cs_mail_correspondence::CorrespondenceError::Invalid);
        }
        DeliveredRouteRecord {
            transport: DeliveryTransport::Native,
            message,
            alias: alias.id(),
            alias_revision: alias.revision(),
            sender,
            recipient: alias.owner(),
            conversation,
            at,
            provenance: ReceivingProvenance::DevelopmentAssignment { operation },
        }
        .try_into()
    }
    pub const fn provenance(&self) -> &ReceivingProvenance {
        &self.0.provenance
    }
    pub const fn transport(&self) -> DeliveryTransport {
        self.0.transport
    }
    pub const fn message(&self) -> cs_mail_primitives::MessageId {
        self.0.message
    }
    pub const fn alias(&self) -> AliasId {
        self.0.alias
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.0.recipient
    }
    pub const fn sender(&self) -> ProtocolIdentity {
        self.0.sender
    }
    pub const fn conversation(&self) -> ConversationId {
        self.0.conversation
    }
    pub const fn at(&self) -> CanonicalTime {
        self.0.at
    }
}

/// The sender-visible destination; no private alias metadata is published.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicReceivingAlias {
    pub owner: ProtocolIdentity,
    pub id: AliasId,
    pub address: MemberAddress,
}
