//! Opening a guest's relationship with a recipient: registering the guest's synthetic
//! identity as its sender subject, and choosing the request history by the mailbox and
//! payment-instrument evidence (see `docs/guest-sender-design.md`). The decision runs
//! once, under the storage adapter's protection, and its effects commit together.
use crate::members::RelationshipAddress;
use cs_mail_guests::{
    GuestError, GuestSession, HistoryDecision, InstrumentFingerprint, SenderSubject,
    resolve_history,
};
use cs_mail_primitives::{CanonicalTime, ProtocolIdentity, RequestHistoryRef};
use cs_mail_protocol::RequestHistory;

/// What a guest brings to a recipient: a verified mailbox and a tokenized instrument.
pub struct GuestApproach<'a> {
    pub session: &'a GuestSession,
    pub identity: ProtocolIdentity,
    pub fingerprint: InstrumentFingerprint,
    /// The processor's token for the instrument; the capture draws on it and refunds
    /// return to it.
    pub token: [u8; 32],
    pub address: &'a RelationshipAddress,
}

/// Protected context for opening a guest relationship.
pub struct GuestRelationshipContext {
    /// Trusted time sampled after acquiring the protected context.
    pub now: CanonicalTime,
    /// The subject already registered under the guest's identity, if any.
    pub registered: Option<SenderSubject>,
    /// The history already linked to the mailbox for this recipient, if any.
    pub mailbox_history: Option<RequestHistory>,
    /// The history already linked to the fingerprint for this recipient, if any.
    pub fingerprint_history: Option<RequestHistory>,
}

/// The complete consequence of opening a guest relationship.
#[derive(Debug)]
pub struct GuestRelationshipPlan {
    /// The subject to register, when the identity is not yet registered.
    pub register: Option<SenderSubject>,
    pub history: HistoryDecision,
    /// When the plan was made.
    pub at: CanonicalTime,
}

#[derive(Debug)]
pub enum GuestAccessError {
    /// The guest's identity already belongs to another sender subject.
    IdentityTaken,
    Guest(GuestError),
}
impl From<GuestError> for GuestAccessError {
    fn from(error: GuestError) -> Self {
        Self::Guest(error)
    }
}

/// Decides opening the relationship: the identity must be free or already this guest's,
/// and the history follows the evidence.
/// # Errors
/// `IdentityTaken` for an identity registered to another subject; the history rules'
/// refusals.
pub fn plan_guest_relationship(
    approach: &GuestApproach<'_>,
    context: &GuestRelationshipContext,
    fresh: RequestHistoryRef,
    merge: impl FnOnce(&RequestHistoryRef, &RequestHistoryRef) -> RequestHistoryRef,
) -> Result<GuestRelationshipPlan, GuestAccessError> {
    let mailbox = approach.session.authorize(context.now)?;
    let subject = SenderSubject::Guest(mailbox.clone());
    let register = match &context.registered {
        None => Some(subject),
        Some(existing) if *existing == subject => None,
        Some(_) => return Err(GuestAccessError::IdentityTaken),
    };
    let merged = match (&context.mailbox_history, &context.fingerprint_history) {
        (Some(a), Some(b)) => merge(&a.reference, &b.reference),
        _ => fresh,
    };
    let history = resolve_history(
        approach.address.recipient(),
        (mailbox, context.mailbox_history.as_ref()),
        (approach.fingerprint, context.fingerprint_history.as_ref()),
        fresh,
        merged,
        context.now,
    )?;
    Ok(GuestRelationshipPlan {
        register,
        history,
        at: context.now,
    })
}

/// Atomic persistence of a guest relationship's opening: the subject registration, the
/// history created or merged with the evidence that caused it, the evidence links, the
/// instrument as a funding source, and the relationship itself.
pub trait GuestStore {
    type Error: From<GuestAccessError>;
    /// Loads the protected context, invokes `decide` once, and commits its plan, returning
    /// the history the relationship uses.
    /// # Errors
    /// Rolls back on refusal or persistence failure.
    fn transact_guest_relationship(
        &self,
        approach: &GuestApproach<'_>,
        clock: impl FnOnce() -> CanonicalTime,
        decide: impl FnOnce(GuestRelationshipContext) -> Result<GuestRelationshipPlan, Self::Error>,
    ) -> Result<RequestHistoryRef, Self::Error>;
}
