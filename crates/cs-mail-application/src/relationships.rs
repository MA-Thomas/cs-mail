//! Relationship-level lane acceptance coordinates permission and request settlement.
use cs_mail_capabilities::{CapabilityError, Lane, ValidatedLaneGrant};
use cs_mail_protocol::{ProtocolError, SettlementSnapshot, TransitionContext, TransitionManifest};

pub struct LaneAcceptance {
    lane: Lane,
    settlement: TransitionManifest,
}
impl LaneAcceptance {
    /// The caller establishes recipient signature authority under protected context.
    /// Exact stored grants replay without a second settlement. Changed grants need a new version.
    /// # Errors
    /// Rejects foreign, inactive, blocked or conflicting grants and invalid settlement.
    pub fn decide(
        snapshot: &SettlementSnapshot,
        grant: ValidatedLaneGrant,
        context: &TransitionContext,
    ) -> Result<Option<Self>, LaneAcceptanceError> {
        let grant = grant.activate().map_err(LaneAcceptanceError::Capability)?;
        let relationship = &snapshot.state.relationship;
        if relationship.state == cs_mail_protocol::RelationshipState::Blocked {
            return Err(LaneAcceptanceError::Protocol(ProtocolError::ContactBlocked));
        }
        if grant.grant.sender != relationship.key.sender
            || grant.grant.recipient != relationship.key.recipient
        {
            return Err(LaneAcceptanceError::Capability(
                CapabilityError::NotAuthorized,
            ));
        }
        if let Some(existing) = &snapshot.lane {
            if *existing == grant {
                return Ok(None);
            }
            if existing.grant.id != grant.grant.id || grant.grant.version <= existing.grant.version
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
        let settlement = cs_mail_protocol::accept_express_lane(snapshot, grant.grant.id, context)
            .map_err(LaneAcceptanceError::Protocol)?;
        Ok(Some(Self {
            lane: grant,
            settlement,
        }))
    }
    pub fn into_effects(self) -> (Lane, TransitionManifest) {
        (self.lane, self.settlement)
    }
}
#[derive(Debug)]
pub enum LaneAcceptanceError {
    Protocol(ProtocolError),
    Capability(CapabilityError),
}
