//! Explicit development correction, authorized by fresh recipient consent.
//! Historical terms stay immutable; ordinary acceptance continues to use `ReplyLane`.
use cs_mail_capabilities::{Lane, LaneMode, LaneSubject, SignedLaneGrant, VerifiedLaneGrant};
use cs_mail_primitives::{CanonicalTime, IdempotencyKey};
use cs_mail_protocol::{
    PolicySnapshot, ProtocolError, ProtocolEvent, ProtocolEventKind, Relationship,
    RelationshipRequest, RelationshipState, RequestLifecycle, RequestReservation,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcceptanceAlignmentCommand {
    pub source: RequestReservation,
    pub operation: IdempotencyKey,
    pub consent: SignedLaneGrant,
    pub policy: PolicySnapshot,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AlignmentDisposition {
    Apply,
    NotAccepted,
    AlreadyAgreed,
    Withdrawn,
    ExistingPermission,
    Expired,
    UnresolvedRequest,
}
#[derive(Clone, Copy)]
pub struct AcceptanceAlignmentContext<'a> {
    pub request: &'a RelationshipRequest,
    pub acceptance: Option<&'a ProtocolEvent>,
    pub original: &'a Relationship,
    pub reverse: &'a Relationship,
    pub lane: Option<&'a Lane>,
    pub last_withdrawal: Option<&'a ProtocolEvent>,
    pub reverse_has_unresolved_request: bool,
    pub now: CanonicalTime,
}
impl AcceptanceAlignmentCommand {
    /// # Errors
    /// Rejects substituted evidence, scope, consent or a changed correction window.
    pub fn decide(
        &self,
        context: AcceptanceAlignmentContext<'_>,
        proof: VerifiedLaneGrant,
    ) -> Result<AlignmentDisposition, ProtocolError> {
        use AlignmentDisposition as D;
        let request = context.request;
        if self.source != request.reservation()
            || context.original.key.sender != request.terms.sender
            || context.original.key.recipient != request.terms.recipient
            || context.original.key.reference != request.terms.relationship
            || context.reverse.key.sender != request.terms.recipient
            || context.reverse.key.recipient != request.terms.sender
        {
            return Err(ProtocolError::InvalidState);
        }
        let RequestLifecycle::Accepted { event, .. } = request.lifecycle else {
            return Ok(D::NotAccepted);
        };
        if request.terms.reply_lane.agreed().is_some() {
            return Ok(D::AlreadyAgreed);
        }
        let acceptance = context.acceptance.ok_or(ProtocolError::InvalidState)?;
        if acceptance.reference != event
            || acceptance.at > context.now
            || !matches!(
                acceptance.kind,
                ProtocolEventKind::RelationshipAccepted
                    | ProtocolEventKind::RelationshipLaneGranted(_)
            )
        {
            return Err(ProtocolError::InvalidState);
        }
        if context.original.state.sender_reply_restriction().is_some()
            || context.reverse.state.sender_reply_restriction().is_some()
            || context
                .last_withdrawal
                .is_some_and(|event| event.reference.0 >= acceptance.reference.0)
        {
            return Ok(D::Withdrawn);
        }
        if context.reverse.state == RelationshipState::Accepted
            || context.lane.is_some_and(|lane| {
                lane.provides_relationship_access(context.now)
                    || (matches!(
                        lane.authority(),
                        cs_mail_capabilities::reply::LaneAuthority::RecipientGrant
                    ) && lane.grant().issued_at >= acceptance.at)
            })
        {
            return Ok(D::ExistingPermission);
        }
        if context.reverse_has_unresolved_request {
            return Ok(D::UnresolvedRequest);
        }
        let expires = acceptance
            .at
            .checked_add(self.policy.reply_lane.lifetime())
            .ok_or(ProtocolError::ArithmeticOverflow)?;
        if expires <= context.now {
            return Ok(D::Expired);
        }
        let lane = proof.activate().map_err(|_| ProtocolError::InvalidState)?;
        let grant = lane.grant();
        if grant != &self.consent.grant
            || grant.sender != context.reverse.key.sender
            || grant.recipient != context.reverse.key.recipient
            || grant.subject
                != LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                    *context.reverse.key.reference.as_bytes(),
                ))
            || grant.mode != LaneMode::Expiring
            || grant.not_after != expires
            || grant.issued_at > context.now
            || grant.issued_at < acceptance.at
            || grant.origin != Some(cs_mail_primitives::OriginMode::HumanInitiated)
            || grant.not_before != grant.issued_at
            || grant.lifetime != self.policy.reply_lane.lifetime()
            || grant.rate_limit != self.policy.reply_lane.rate_limit()
            || &grant.purpose != self.policy.reply_lane.purpose()
            || grant.deployment_domain != request.terms.financial.scope.deployment_domain
            || grant.intended_provider != self.policy.recipient_provider
            || grant.protocol_version != self.policy.protocol_version
        {
            return Err(ProtocolError::InvalidState);
        }
        Ok(D::Apply)
    }
}
