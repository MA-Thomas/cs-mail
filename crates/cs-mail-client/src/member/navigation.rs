//! Read-only navigation facts. No cache-format change or communication authority.
use super::{Error, MemberAnswer, MemberClient, storage, views};
use cs_mail_application::members::{OutgoingRelationshipView, RelationshipView};
use cs_mail_primitives::{CanonicalTime, DeclaredPurpose, KnownPurpose, OriginMode};
use cs_mail_security::MemberRead;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct NavigationLane {
    pub id: String,
    pub expires_at: u64,
    pub scope: String,
    pub max_messages: u32,
    pub interval_ms: u64,
}
#[derive(Clone, Debug)]
pub struct NavigationContact {
    pub sender: String,
    pub title: String,
    pub external: bool,
    pub requests: Vec<NavigationRequest>,
    pub sent_requests: Vec<NavigationRequest>,
    pub incoming_lane: Option<NavigationLane>,
    pub outgoing_lane: Option<NavigationLane>,
}
#[derive(Clone, Debug)]
pub struct NavigationRequest {
    pub record: views::Request,
    pub issued_at: u64,
    pub financial_summary: String,
}
impl MemberClient {
    /// Fresh records for both directions, with time-scoped lane projections.
    /// # Errors
    /// Local custody, authentication, transport or record binding failed.
    pub fn navigation(&self) -> Result<Vec<NavigationContact>, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let MemberAnswer::Inbox(incoming) = self.read(&state.profile, &keys, MemberRead::Inbox)?
        else {
            return Err(Error::InvalidData);
        };
        let MemberAnswer::Outbox(outgoing) =
            self.read(&state.profile, &keys, MemberRead::Outbox)?
        else {
            return Err(Error::InvalidData);
        };
        project(state.profile.persona, &incoming, &outgoing, (self.clock)())
    }
}
pub(super) fn project(
    me: cs_mail_primitives::ProtocolIdentity,
    incoming: &[RelationshipView],
    outgoing: &[OutgoingRelationshipView],
    now: CanonicalTime,
) -> Result<Vec<NavigationContact>, Error> {
    let mut contacts = BTreeMap::new();
    for view in incoming {
        if view.handle.recipient != me {
            return Err(Error::InvalidData);
        }
        let summary = views::summary(view, Some(now));
        contacts.insert(
            view.handle.sender,
            NavigationContact {
                sender: summary.sender,
                title: summary.title,
                external: summary.external,
                requests: requests(view, now),
                sent_requests: Vec::new(),
                incoming_lane: lane(view, now),
                outgoing_lane: None,
            },
        );
    }
    for outgoing in outgoing {
        let view = &outgoing.relationship;
        if view.handle.sender != me {
            return Err(Error::InvalidData);
        }
        if view.state == cs_mail_protocol::RelationshipState::Unknown && view.requests.is_empty() {
            continue;
        }
        let other = view.handle.recipient;
        let contact = contacts.entry(other).or_insert_with(|| NavigationContact {
            sender: other.0.to_string(),
            title: outgoing
                .recipient_name
                .clone()
                .unwrap_or_else(|| format!("Member {}", other.0)),
            external: false,
            requests: Vec::new(),
            sent_requests: Vec::new(),
            incoming_lane: None,
            outgoing_lane: None,
        });
        contact.sent_requests = requests(view, now);
        contact.outgoing_lane = lane(view, now);
    }
    Ok(contacts.into_values().collect())
}
fn requests(view: &RelationshipView, now: CanonicalTime) -> Vec<NavigationRequest> {
    views::project_requests(view, Some(now))
        .into_iter()
        .zip(&view.requests)
        .map(|(record, request)| {
            let financial_summary = request
                .money
                .as_ref()
                .map_or("Financial status unavailable", |money| {
                    use cs_mail_finance::{CaptureStatus, RefundView};
                    match money.capture {
                        CaptureStatus::Pending => "Payment pending",
                        CaptureStatus::CancellationRequested => "Cancellation pending",
                        CaptureStatus::Voided => "No charge collected",
                        CaptureStatus::Reversed => "Payment reversed",
                        CaptureStatus::FundingFinalized => match money.refund {
                            RefundView::Owed(_) => "Refund pending",
                            RefundView::Confirmed(_) => "Refund confirmed",
                            RefundView::None => "Charged",
                        },
                    }
                })
                .to_owned();
            NavigationRequest {
                record,
                issued_at: request.terms.issued_at.0,
                financial_summary,
            }
        })
        .collect()
}
fn lane(view: &RelationshipView, now: CanonicalTime) -> Option<NavigationLane> {
    if !matches!(
        views::permission(view.state, view.lane.as_ref(), Some(now)).1,
        views::PermissionRoute::Bounded
    ) {
        return None;
    }
    let lane = view.lane.as_ref()?;
    let grant = lane.grant();
    if grant.sender != view.handle.sender || grant.recipient != view.handle.recipient {
        return None;
    }
    let expires_at = match grant.mode {
        cs_mail_capabilities::LaneMode::Expiring => lane.horizon_at().min(grant.not_after),
        cs_mail_capabilities::LaneMode::Persistent => grant.not_after,
    }
    .0;
    let purpose = match &grant.purpose {
        DeclaredPurpose::Unspecified => "Unspecified purpose".into(),
        DeclaredPurpose::Known(purpose) => match purpose {
            KnownPurpose::Personal => "Personal",
            KnownPurpose::Transactional => "Transactional",
            KnownPurpose::Request => "Requests",
            KnownPurpose::Bulk => "Bulk",
        }
        .into(),
        DeclaredPurpose::Extension(identifier) => identifier.name().to_owned(),
    };
    let scope = match grant.origin {
        Some(OriginMode::HumanInitiated) => format!("{purpose} · human initiated"),
        Some(OriginMode::AgentDraftHumanApproved) => format!("{purpose} · human approved"),
        Some(OriginMode::AutonomousAgent) => format!("{purpose} · autonomous agent"),
        Some(OriginMode::AutomatedSystem) => format!("{purpose} · automated system"),
        Some(OriginMode::LegacyOrUnspecified) | None => purpose,
    };
    Some(NavigationLane {
        id: grant.id.0.to_string(),
        expires_at,
        scope,
        max_messages: grant.rate_limit.max_messages,
        interval_ms: grant.rate_limit.interval.0,
    })
}
