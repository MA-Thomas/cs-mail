//! Complete request consequences across the two independently owned directions.
//! Protected facts enter once; storage receives effects, never another decision callback.
use cs_mail_primitives::{JournalPosition, RelationshipRef};
pub mod documents;
use cs_mail_protocol::{
    KernelCommand, ProtocolCommand, ProtocolError, ProtocolEventKind, SettlementSnapshot,
    TransitionContext, TransitionManifest,
};

pub struct DirectionSnapshot {
    pub reply_offers: std::collections::BTreeMap<
        cs_mail_primitives::RequestId,
        Option<cs_mail_primitives::ReplyEpoch>,
    >,
    pub snapshot: Box<SettlementSnapshot>,
}
pub struct DirectionEffect {
    lane: Option<cs_mail_capabilities::SenderScopedLane>,
    schedules: Vec<cs_mail_primitives::ScheduleChange>,
    relationship: RelationshipRef,
    manifest: TransitionManifest,
    journal_position: JournalPosition,
}
impl DirectionEffect {
    pub fn schedules(&self) -> &[cs_mail_primitives::ScheduleChange] {
        &self.schedules
    }
    pub fn lane(&self) -> Option<&cs_mail_capabilities::SenderScopedLane> {
        self.lane.as_ref()
    }
    pub const fn relationship(&self) -> RelationshipRef {
        self.relationship
    }
    pub const fn manifest(&self) -> &TransitionManifest {
        &self.manifest
    }
    pub const fn journal_position(&self) -> JournalPosition {
        self.journal_position
    }
}

/// Guest requests have no enrolled opposite member. An absent member direction is
/// a protected absence, not an invitation to skip the pair's concurrency boundary.
pub enum CounterpartyContext {
    Guest,
    Member { opposite: Option<DirectionSnapshot> },
}

pub struct ReceivingScopes {
    pub forward: Option<crate::aliases::SenderRouteScope>,
    pub opposite: Option<crate::aliases::SenderRouteScope>,
}

pub struct RequestDecision {
    scopes: Vec<crate::aliases::SenderScopeRecord>,
    delivery: Option<documents::MailboxDelivery>,
    forward: TransitionManifest,
    opposite: Option<DirectionEffect>,
}
impl RequestDecision {
    pub fn into_effects(
        self,
    ) -> (
        TransitionManifest,
        Option<DirectionEffect>,
        Option<documents::MailboxDelivery>,
        Vec<crate::aliases::SenderScopeRecord>,
    ) {
        (self.forward, self.opposite, self.delivery, self.scopes)
    }

    /// # Errors
    /// Refuses crossed requests, contradictory sender permission and incomplete pairs.
    pub fn decide(
        snapshot: &SettlementSnapshot,
        counterparty: &CounterpartyContext,
        command: &KernelCommand<ProtocolCommand>,
        context: &TransitionContext,
        document: Option<&documents::SubmissionDocument>,
        receiving: &ReceivingScopes,
    ) -> Result<Self, ProtocolError> {
        let opposite = match counterparty {
            CounterpartyContext::Guest => None,
            CounterpartyContext::Member { opposite } => opposite.as_ref(),
        };
        if let Some(opposite) = opposite {
            let opposite = &opposite.snapshot;
            opposite.state.validate()?;
            let forward = snapshot.state.relationship.key;
            let reverse = opposite.state.relationship.key;
            if reverse.sender != forward.recipient || reverse.recipient != forward.sender {
                return Err(ProtocolError::IncompleteSnapshot);
            }
            if matches!(
                command.command(),
                ProtocolCommand::IssueRequestTerms { .. }
                    | ProtocolCommand::CreateRequest { .. }
                    | ProtocolCommand::SubmitRequestToRecipient { .. }
            ) {
                if let Some(request) = opposite
                    .state
                    .active_request()
                    .filter(|request| request.lifecycle.awaits_decision_at(context.now))
                {
                    return Err(ProtocolError::OppositeRequestPending {
                        request: request.id,
                    });
                }
            }
            if matches!(
                command.command(),
                ProtocolCommand::SubmitRequestToRecipient { .. }
            ) {
                if let Some(reason) = opposite.state.relationship.state.sender_reply_restriction() {
                    return Err(ProtocolError::SenderReplyRestricted(reason));
                }
            }
        }
        let mut context = context.clone();
        context.request_delivery = match counterparty {
            CounterpartyContext::Guest => cs_mail_protocol::RequestDeliveryRoute::GuestRelay,
            CounterpartyContext::Member { .. } => {
                cs_mail_protocol::RequestDeliveryRoute::NativeMailboxes
            }
        };
        let forward = cs_mail_protocol::transition(snapshot, command, &context)?;
        let delivery = mailbox_delivery(
            &forward,
            document,
            &context,
            opposite.map_or(cs_mail_primitives::ReplyEpoch(0), |direction| {
                direction.snapshot.state.relationship.reply_epoch
            }),
        )?;
        let incoming = forward
            .protocol_events
            .iter()
            .find_map(|event| match event.kind {
                ProtocolEventKind::RequestSubmitted(id) => forward.next_state.requests.get(&id),
                _ => None,
            });
        let opposite = match (counterparty, incoming) {
            (CounterpartyContext::Member { opposite }, Some(incoming)) => {
                let direction = opposite.as_ref().ok_or(ProtocolError::IncompleteSnapshot)?;
                let manifest = cs_mail_protocol::receive_native_request(
                    &direction.snapshot,
                    incoming,
                    &context,
                )?;
                manifest.map(|manifest| DirectionEffect {
                    relationship: direction.snapshot.state.relationship.key.reference,
                    manifest,
                    journal_position: context.journal_position,
                    lane: None,
                    schedules: Vec::new(),
                })
            }
            _ => None,
        };
        let mut result = Self::coordinate_acceptance(
            snapshot,
            counterparty,
            forward,
            opposite,
            &context,
            None,
            receiving,
        )?;
        result.delivery = delivery;
        Ok(result)
    }
    /// Coordinates the same reverse consequence for standing and express-lane acceptance.
    /// # Errors
    /// Refuses missing opposite ownership, withdrawn agreement or invalid lane terms.
    #[allow(clippy::too_many_arguments)] // Both protected directions and their prior receiving scopes form one acceptance.
    pub fn coordinate_acceptance(
        before: &SettlementSnapshot,
        counterparty: &CounterpartyContext,
        forward: TransitionManifest,
        mut opposite: Option<DirectionEffect>,
        context: &TransitionContext,
        forward_lane: Option<(cs_mail_primitives::LaneId, cs_mail_primitives::Version)>,
        receiving: &ReceivingScopes,
    ) -> Result<Self, ProtocolError> {
        let accepted = forward.next_state.requests.values().find(|request| {
            matches!(
                request.lifecycle,
                cs_mail_protocol::RequestLifecycle::Accepted { .. }
            ) && before.state.requests.get(&request.id).is_some_and(|old| {
                !matches!(
                    old.lifecycle,
                    cs_mail_protocol::RequestLifecycle::Accepted { .. }
                )
            })
        });
        let mut scopes = Vec::new();
        if let Some(request) = accepted {
            scopes.push(default_scope(
                &forward.next_state.relationship,
                cs_mail_capabilities::routes::ReceivingScope::one(request.terms.receiving_alias),
                context.now,
                forward_lane,
                receiving.forward.as_ref(),
            )?);
        }
        // An explicit new recipient grant renews the previously selected entrances.
        // Revocation/unblocking and alias creation never reach this branch.
        if accepted.is_none()
            && (forward_lane.is_some()
                || (forward.next_state.relationship.state
                    == cs_mail_protocol::RelationshipState::Accepted
                    && before.state.relationship.state
                        != cs_mail_protocol::RelationshipState::Accepted))
            && let Some(previous) = receiving.forward.as_ref()
        {
            scopes.push(default_scope(
                &forward.next_state.relationship,
                previous.receiving().clone(),
                context.now,
                forward_lane,
                Some(previous),
            )?);
        }
        if let (
            CounterpartyContext::Member {
                opposite: direction,
            },
            Some(request),
        ) = (counterparty, accepted)
        {
            let direction = direction
                .as_ref()
                .ok_or(ProtocolError::IncompleteSnapshot)?;
            if opposite.is_some() {
                return Err(ProtocolError::InvalidState);
            }
            let plan = crate::relationships::ReplyLane::decide(
                &direction.snapshot,
                request,
                context,
                direction.reply_offers.get(&request.id).copied().flatten(),
            )
            .map_err(|error| match error {
                crate::relationships::LaneAcceptanceError::Protocol(error) => error,
                crate::relationships::LaneAcceptanceError::Capability(
                    cs_mail_capabilities::CapabilityError::ArithmeticOverflow,
                ) => ProtocolError::ArithmeticOverflow,
                crate::relationships::LaneAcceptanceError::Capability(_) => {
                    ProtocolError::InvalidState
                }
            })?;
            if let Some(plan) = plan {
                let (lane, manifest) = plan.into_effects();
                let schedules = vec![lane.schedule_change()];
                if let cs_mail_protocol::RequestReturnRoute::Native(alias) =
                    request.terms.return_route
                {
                    scopes.push(default_scope(
                        &manifest.next_state.relationship,
                        cs_mail_capabilities::routes::ReceivingScope::one(alias),
                        context.now,
                        Some((lane.grant().id, lane.grant().version)),
                        receiving.opposite.as_ref(),
                    )?);
                }
                opposite = Some(DirectionEffect {
                    relationship: direction.snapshot.state.relationship.key.reference,
                    manifest,
                    journal_position: context.journal_position,
                    lane: Some(lane),
                    schedules,
                });
            }
        }
        Ok(Self {
            scopes,
            forward,
            opposite,
            delivery: None,
        })
    }
}

fn mailbox_delivery(
    forward: &TransitionManifest,
    document: Option<&documents::SubmissionDocument>,
    context: &TransitionContext,
    epoch: cs_mail_primitives::ReplyEpoch,
) -> Result<Option<documents::MailboxDelivery>, ProtocolError> {
    Ok(if let Some(id) = forward.mailbox_deliveries.first() {
        let message = forward
            .next_messages
            .get(id)
            .ok_or(ProtocolError::MissingRecord)?;
        let (cs_mail_protocol::AdmissionBasis::InitialRequest {
            request: request_id,
        }
        | cs_mail_protocol::AdmissionBasis::RequestFollowup {
            request: request_id,
            ..
        }) = message.basis
        else {
            return Err(ProtocolError::InvalidState);
        };
        let request = forward
            .next_state
            .requests
            .get(&request_id)
            .ok_or(ProtocolError::MissingRecord)?;
        Some(
            documents::MailboxDelivery::decide(
                document.ok_or(ProtocolError::MissingRecord)?,
                message,
                request,
                context,
                epoch,
            )
            .map_err(|_| ProtocolError::InvalidState)?,
        )
    } else {
        None
    })
}

fn default_scope(
    r: &cs_mail_protocol::Relationship,
    receiving: cs_mail_capabilities::routes::ReceivingScope,
    at: cs_mail_primitives::CanonicalTime,
    lane: Option<(cs_mail_primitives::LaneId, cs_mail_primitives::Version)>,
    previous: Option<&crate::aliases::SenderRouteScope>,
) -> Result<crate::aliases::SenderScopeRecord, ProtocolError> {
    if previous.is_some_and(|p| p.sender() != r.key.sender || p.recipient() != r.key.recipient) {
        return Err(ProtocolError::InvalidState);
    }
    Ok(crate::aliases::SenderScopeRecord {
        revision: previous
            .map_or(Some(1), |p| p.revision().checked_add(1))
            .ok_or(ProtocolError::ArithmeticOverflow)?,
        established_at: at,
        sender: r.key.sender,
        recipient: r.key.recipient,
        relationship_version: r.version,
        receiving,
        lane,
    })
}
