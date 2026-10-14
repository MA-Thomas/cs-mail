//! The member request loop's deployment-level rules: where a relationship between two
//! addresses lives, who may publish an address's endpoint content key, who may read a
//! member's records, and what each party sees of a relationship.
//!
//! Relationship commands themselves are decided by the protocol kernel; this module only
//! locates relationships and authorizes and shapes what surrounds them.
use cs_mail_capabilities::Lane;
use cs_mail_finance::RequestFinancialView;
use cs_mail_primitives::{
    CanonicalTime, ContentScopeRef, PrincipalRef, ProtocolIdentity, RelationshipRef,
    RelationshipVersion, RequestHistoryRef, RequestId, RequestVersion,
};
use cs_mail_privacy::PrivacyDeriver;
use cs_mail_protocol::{
    ActorRef, AdmissionBasis, FollowupPolicy, ProtocolError, ProtocolState, RelationshipState,
    RequestLifecycle, RequestTerms, SettlementSnapshot,
};
use cs_mail_security::{
    AuthoritySnapshot, EndpointKeyPublication, RecipientSigningScope, SecurityError,
    SignedEndpointKey, SignedMemberRead,
};
use serde::{Deserialize, Serialize};

/// Version of the relationship, history and content-scope derivations.
pub const DERIVATION_VERSION: u16 = 1;

/// Where the relationship in which `sender` writes to `recipient` lives, and the scoped
/// references that name it. Derived from the two addresses, never chosen by a client.
/// The request history a new relationship starts with is chosen separately: see
/// [`member_history`] and the guest history rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationshipAddress {
    aggregate_key: String,
    relationship: RelationshipRef,
    content_scope: ContentScopeRef,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
}
impl RelationshipAddress {
    /// # Errors
    /// `PolicyInvalid` for a zero address or an address writing to itself.
    pub fn derive(
        deriver: &PrivacyDeriver,
        deployment_domain: &[u8; 32],
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> Result<Self, ProtocolError> {
        if sender.0 == 0 || recipient.0 == 0 || sender == recipient {
            return Err(ProtocolError::PolicyInvalid);
        }
        let relationship =
            deriver.relationship_ref(DERIVATION_VERSION, deployment_domain, sender, recipient);
        let aggregate_key = format!(
            "relationship/v{DERIVATION_VERSION}/{}",
            relationship
                .as_bytes()
                .iter()
                .fold(String::new(), |mut hex, b| {
                    use core::fmt::Write as _;
                    let _ = write!(hex, "{b:02x}");
                    hex
                })
        );
        Ok(Self {
            aggregate_key,
            relationship,
            content_scope: deriver.content_scope_ref(
                DERIVATION_VERSION,
                deployment_domain,
                sender,
                recipient,
            ),
            sender,
            recipient,
        })
    }
    pub fn aggregate_key(&self) -> &str {
        &self.aggregate_key
    }
    pub const fn relationship(&self) -> RelationshipRef {
        self.relationship
    }
    pub const fn content_scope(&self) -> ContentScopeRef {
        self.content_scope
    }
    pub const fn sender(&self) -> ProtocolIdentity {
        self.sender
    }
    pub const fn recipient(&self) -> ProtocolIdentity {
        self.recipient
    }
    /// The state the relationship starts from when it is first opened, under `history`.
    pub fn initial_state(&self, history: RequestHistoryRef, now: CanonicalTime) -> ProtocolState {
        ProtocolState::initial_scoped(self.relationship, history, self.sender, self.recipient, now)
    }
    /// What a participant needs to sign commands and bind content for this relationship.
    pub const fn public(&self) -> RelationshipHandle {
        RelationshipHandle {
            relationship: self.relationship,
            content_scope: self.content_scope,
            sender: self.sender,
            recipient: self.recipient,
        }
    }
}

/// A member's request history with `recipient`, shared by all of the member's addresses
/// because it derives from the member's private principal.
pub fn member_history(
    deriver: &PrivacyDeriver,
    deployment_domain: &[u8; 32],
    principal: PrincipalRef,
    recipient: ProtocolIdentity,
) -> RequestHistoryRef {
    deriver.request_history_ref(DERIVATION_VERSION, deployment_domain, principal, recipient)
}

/// The references a participant signs and binds content under.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelationshipHandle {
    pub relationship: RelationshipRef,
    pub content_scope: ContentScopeRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
}

/// Authorizes a member read: the signature is from an active key of the read's address,
/// in this deployment, and fresh.
/// # Errors
/// `SigningScopeMismatch` for another deployment or a key of another address; the read's
/// own freshness and signature errors.
pub fn authorize_read(
    signed: &SignedMemberRead,
    authority: &AuthoritySnapshot,
    expected: RecipientSigningScope,
    now: CanonicalTime,
) -> Result<(), SecurityError> {
    if signed.scope != expected {
        return Err(SecurityError::SigningScopeMismatch);
    }
    let (actor, key) = authority.active_actor(signed.operational_key, now)?;
    if !matches!(actor, ActorRef::Sender(p) | ActorRef::Recipient(p) if p == signed.persona) {
        return Err(SecurityError::SigningScopeMismatch);
    }
    signed.verify(&key, now)
}

/// Protected context for publishing an address's endpoint key.
pub struct EndpointKeyContext {
    /// Authority of the account owning the address, present only if it may use the service.
    pub authority: AuthoritySnapshot,
    pub current: Option<EndpointKeyPublication>,
}
/// Decides an endpoint key publication. Returns the publication to record, or `None` for
/// an exact replay of the current one.
/// # Errors
/// Rejects another deployment, a key of another address, a bad signature and a version
/// that does not increase.
pub fn decide_endpoint_key(
    signed: &SignedEndpointKey,
    context: &EndpointKeyContext,
    expected: RecipientSigningScope,
    now: CanonicalTime,
) -> Result<Option<EndpointKeyPublication>, EndpointKeyError> {
    if signed.scope != expected {
        return Err(SecurityError::SigningScopeMismatch.into());
    }
    let persona = signed.publication.persona;
    let (actor, key) = context
        .authority
        .active_actor(signed.operational_key, now)?;
    if !matches!(actor, ActorRef::Sender(p) | ActorRef::Recipient(p) if p == persona) {
        return Err(SecurityError::SigningScopeMismatch.into());
    }
    signed.verify(&key)?;
    match context.current {
        Some(current) if current == signed.publication => Ok(None),
        Some(current) if signed.publication.version <= current.version => {
            Err(ProtocolError::VersionConflict.into())
        }
        _ => Ok(Some(signed.publication)),
    }
}
#[derive(Debug)]
pub enum EndpointKeyError {
    Security(SecurityError),
    Protocol(ProtocolError),
}
impl From<SecurityError> for EndpointKeyError {
    fn from(error: SecurityError) -> Self {
        Self::Security(error)
    }
}
impl From<ProtocolError> for EndpointKeyError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

/// Atomic persistence of an endpoint key publication.
pub trait EndpointKeyStore {
    type Error: From<EndpointKeyError>;
    /// Loads the protected context, invokes `decide` once, and records its publication
    /// together with its evidence, or nothing.
    /// # Errors
    /// Rolls back on refusal or persistence failure.
    fn transact_endpoint_key(
        &self,
        signed: &SignedEndpointKey,
        decide: impl FnOnce(
            EndpointKeyContext,
        )
            -> Result<Option<(EndpointKeyPublication, CanonicalTime)>, Self::Error>,
    ) -> Result<(), Self::Error>;
}
/// Publishes an address's endpoint key.
/// # Errors
/// The decision's refusals and persistence failures.
pub fn publish_endpoint_key<R: EndpointKeyStore>(
    repository: &R,
    signed: &SignedEndpointKey,
    expected: RecipientSigningScope,
    clock: impl FnOnce() -> CanonicalTime,
) -> Result<(), R::Error> {
    repository.transact_endpoint_key(signed, |context| {
        let now = clock();
        Ok(decide_endpoint_key(signed, &context, expected, now)?.map(|p| (p, now)))
    })
}

/// One party's view of a relationship. Both parties see the same facts about it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelationshipView {
    pub reply_epoch: cs_mail_primitives::ReplyEpoch,
    pub display_name: Option<String>,
    pub handle: RelationshipHandle,
    /// The sender's mailbox, when the sender is a guest: members see who a guest is.
    #[serde(default)]
    pub guest: Option<cs_mail_guests::GuestMailbox>,
    pub state: RelationshipState,
    /// The version a recipient decision must name.
    pub version: RelationshipVersion,
    pub followup_policy: FollowupPolicy,
    pub lane: Option<Lane>,
    pub requests: Vec<RequestView>,
    pub external_conversations: Vec<cs_mail_correspondence::ConversationId>,
}
/// A request and its money, as its parties see them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestView {
    pub id: RequestId,
    /// The version a command on this request must name.
    pub version: RequestVersion,
    pub terms: RequestTerms,
    pub lifecycle: RequestLifecycle,
    /// Messages admitted under this request, the initial message included.
    pub messages: usize,
    pub money: Option<RequestFinancialView>,
}
impl RelationshipView {
    /// The recipient's view: only requests submitted to them. A request prepared and
    /// cancelled before submission, or refused before it existed, never reached the
    /// recipient. A member's own external invitation also makes the relationship
    /// visible, even when no paid request has been submitted.
    pub fn for_recipient(mut self, invited: bool) -> Option<Self> {
        self.requests
            .retain(|request| request.lifecycle.submission().is_some());
        (invited || !self.requests.is_empty()).then_some(self)
    }
    /// The view of `snapshot`, which must be the relationship `handle` names.
    pub fn of(handle: RelationshipHandle, snapshot: &SettlementSnapshot) -> Self {
        let state = &snapshot.state;
        Self {
            reply_epoch: state.relationship.reply_epoch,
            handle,
            display_name: None,
            guest: None,
            external_conversations: Vec::new(),
            state: state.relationship.state,
            version: state.relationship.version,
            followup_policy: state.followup_policy,
            lane: snapshot.lane.clone(),
            requests: state
                .requests
                .values()
                .map(|request| RequestView {
                    id: request.id,
                    version: request.version,
                    terms: request.terms.clone(),
                    lifecycle: request.lifecycle,
                    messages: snapshot
                        .messages
                        .values()
                        .filter(|m| {
                            matches!(
                                m.basis,
                                AdmissionBasis::InitialRequest { request: r }
                                    | AdmissionBasis::RequestFollowup { request: r, .. }
                                    if r == request.id
                            )
                        })
                        .count(),
                    money: snapshot
                        .payments
                        .get(&request.id)
                        .map(cs_mail_finance::RequestFinancials::view),
                })
                .collect(),
        }
    }
}

/// A member's own account as it sees it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccountView {
    pub account: cs_mail_primitives::AccountId,
    pub personas: Vec<ProtocolIdentity>,
    pub status: cs_mail_accounts::control::AccountStatus,
    /// The revision an account command must name.
    pub revision: u64,
    pub keys: Vec<KeyView>,
    pub billing: cs_mail_primitives::BillingAccountId,
    /// The revision a billing command must name.
    pub billing_revision: u64,
    pub financial_scope: cs_mail_finance::FinancialScope,
    /// The verified bank account request charges are drawn from.
    pub payment_method: [u8; 32],
    /// Whether funded annual service covers `now`.
    pub covered: bool,
    pub contracts: usize,
}
/// One operational key of an account.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyView {
    pub reference: cs_mail_primitives::OperationalKeyRef,
    pub actor: ActorRef,
    pub manager: bool,
    pub active: bool,
}
impl AccountView {
    pub fn of(
        account: cs_mail_primitives::AccountId,
        state: &crate::accounts::operations::AccountState,
        billing: &cs_mail_billing::BillingAccount,
        now: CanonicalTime,
    ) -> Self {
        Self {
            account,
            personas: state.control.personas().iter().copied().collect(),
            status: state.control.status(),
            revision: state.control.revision(),
            keys: state
                .registry
                .records()
                .map(|record| KeyView {
                    reference: record.reference,
                    actor: record.actor,
                    manager: state.control.is_manager(record.reference),
                    active: state.registry.active_actor(record.reference, now).is_ok(),
                })
                .collect(),
            billing: billing.id(),
            billing_revision: billing.revision(),
            financial_scope: billing.scope(),
            payment_method: billing.bank().evidence().bank_token,
            covered: billing.covers(now),
            contracts: billing.contracts().len(),
        }
    }
}

/// What a member receives for an authenticated read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MemberAnswer {
    Account(AccountView),
    /// The relationship as its sender sees it. Absent until it is first opened.
    Sending {
        handle: RelationshipHandle,
        view: Option<RelationshipView>,
    },
    /// The relationship as its recipient sees it, with the ciphertext addressed to them.
    Receiving {
        handle: RelationshipHandle,
        view: Option<RelationshipView>,
        content: Vec<GuestDeliveredContent>,
    },
    Inbox(Vec<RelationshipView>),
    Outbox(Vec<OutgoingRelationshipView>),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutgoingRelationshipView {
    pub relationship: RelationshipView,
    pub recipient_name: Option<String>,
}
/// One ciphertext and the sender certificate it is bound to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuestDeliveredContent {
    pub record: cs_mail_content::EncryptedContentRecord,
    pub certificate: Option<cs_mail_security::SignedContentKeyCertificate>,
}
