//! Relationship-level lane acceptance coordinates permission and request settlement.
#[cfg(feature = "development-maintenance")]
pub mod alignment;

use cs_mail_capabilities::{CapabilityError, Lane, VerifiedLaneGrant};
use cs_mail_protocol::{ProtocolError, SettlementSnapshot, TransitionContext, TransitionManifest};

/// Reverse permission derives from the sender's signed formation record and the
/// accepted lifecycle in protected state. It is not an ordinary recipient-signed grant.
pub struct ReplyLane {
    lane: Lane,
    manifest: TransitionManifest,
}
impl ReplyLane {
    /// # Errors
    /// Refuses foreign/withdrawn agreement, unaccepted requests and overflow.
    pub fn decide(
        snapshot: &SettlementSnapshot,
        request: &cs_mail_protocol::RelationshipRequest,
        context: &TransitionContext,
        offered_epoch: Option<cs_mail_primitives::ReplyEpoch>,
    ) -> Result<Option<Self>, LaneAcceptanceError> {
        use cs_mail_protocol::{RelationshipState, RequestLifecycle};
        let Some(agreed) = request.terms.reply_lane.agreed() else {
            return Ok(None);
        };
        let pair = snapshot.state.relationship.key;
        if pair.sender != request.terms.recipient
            || pair.recipient != request.terms.sender
            || !matches!(request.lifecycle, RequestLifecycle::Accepted { .. })
        {
            return Err(LaneAcceptanceError::Protocol(ProtocolError::InvalidState));
        }
        if snapshot
            .state
            .relationship
            .state
            .sender_reply_restriction()
            .is_some()
            || offered_epoch != Some(snapshot.state.relationship.reply_epoch)
        {
            return Ok(None);
        }
        if snapshot.state.relationship.state == RelationshipState::Accepted
            || snapshot.lane.as_ref().is_some_and(|lane| {
                lane.provides_relationship_access(context.now)
                    && matches!(
                        lane.authority(),
                        cs_mail_capabilities::reply::LaneAuthority::RecipientGrant
                    )
            })
        {
            return Ok(None);
        }
        let version = snapshot
            .lane
            .as_ref()
            .map_or(Some(cs_mail_primitives::Version(1)), |lane| {
                lane.grant().version.checked_next()
            })
            .ok_or(LaneAcceptanceError::Protocol(
                ProtocolError::ArithmeticOverflow,
            ))?;
        let lane = agreed
            .accepted_lane(&cs_mail_capabilities::reply::ReplyAcceptance {
                request: request.id,
                originating_relationship: request.terms.relationship,
                reverse_relationship: pair.reference,
                sender: request.terms.sender,
                recipient: request.terms.recipient,
                creation_key: request.creation_key,
                creation_event: request.creation_event,
                acceptance_event: cs_mail_primitives::EventRef(context.journal_position),
                at: context.now,
                version,
                deployment: request.terms.financial.scope.deployment_domain,
                protocol_version: request.terms.protocol_version,
                provider: request.terms.recipient_provider,
            })
            .map_err(LaneAcceptanceError::Capability)?;
        let manifest = cs_mail_protocol::accept_express_lane(snapshot, lane.grant().id, context)
            .map_err(LaneAcceptanceError::Protocol)?;
        Ok(Some(Self { lane, manifest }))
    }
    pub fn into_effects(self) -> (Lane, TransitionManifest) {
        (self.lane, self.manifest)
    }
}

pub struct LaneAcceptance {
    lane: Lane,
    schedules: Vec<cs_mail_primitives::ScheduleChange>,
    settlement: TransitionManifest,
}
impl LaneAcceptance {
    /// The caller establishes recipient signature authority under protected context.
    /// Exact stored grants replay without a second settlement. Changed grants need a new version.
    /// # Errors
    /// Rejects foreign, inactive, blocked or conflicting grants and invalid settlement.
    pub fn decide(
        snapshot: &SettlementSnapshot,
        grant: VerifiedLaneGrant,
        context: &TransitionContext,
    ) -> Result<Option<Self>, LaneAcceptanceError> {
        let grant = grant.activate().map_err(LaneAcceptanceError::Capability)?;
        let relationship = &snapshot.state.relationship;
        if relationship.state == cs_mail_protocol::RelationshipState::Blocked {
            return Err(LaneAcceptanceError::Protocol(ProtocolError::ContactBlocked));
        }
        if grant.grant().sender != relationship.key.sender
            || grant.grant().recipient != relationship.key.recipient
        {
            return Err(LaneAcceptanceError::Capability(
                CapabilityError::NotAuthorized,
            ));
        }
        if let Some(existing) = &snapshot.lane {
            if *existing == grant {
                return Ok(None);
            }
            if existing.grant().id != grant.grant().id
                || grant.grant().version <= existing.grant().version
            {
                return Err(LaneAcceptanceError::Protocol(
                    ProtocolError::DuplicateConflict,
                ));
            }
        }
        if !grant.provides_relationship_access(context.now) {
            return Err(LaneAcceptanceError::Capability(
                CapabilityError::NotAuthorized,
            ));
        }
        let settlement = cs_mail_protocol::accept_express_lane(snapshot, grant.grant().id, context)
            .map_err(LaneAcceptanceError::Protocol)?;
        Ok(Some(Self {
            schedules: vec![grant.schedule_change()],
            lane: grant,
            settlement,
        }))
    }
    pub fn into_effects(
        self,
    ) -> (
        Lane,
        TransitionManifest,
        Vec<cs_mail_primitives::ScheduleChange>,
    ) {
        (self.lane, self.settlement, self.schedules)
    }
}
#[derive(Debug)]
pub enum LaneAcceptanceError {
    Protocol(ProtocolError),
    Capability(CapabilityError),
}
