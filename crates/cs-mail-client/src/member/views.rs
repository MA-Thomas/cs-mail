//! Presentation projections over existing domain facts; no transition decisions.
use super::{Error, Profile, State, enrollment::Setup, storage::Secrets};
use cs_mail_application::members::{GuestDeliveredContent, RelationshipView};
use cs_mail_finance::RefundView;
use cs_mail_protocol::{RelationshipState, RequestLifecycle};

#[derive(Clone, Debug)]
pub struct Inbox {
    pub last_send: Option<super::SendOutcome>,
    pub pending_request: bool,
    pub request_progress: Option<super::RequestProgress>,
    pub pending_policy: bool,
    pub pending_send: Option<String>,
    pub account: String,
    pub persona: String,
    pub synchronized_at: Option<u64>,
    pub conversations: Vec<Conversation>,
    pub pending_sender: Option<String>,
    pub pending_setup: Option<PendingSetup>,
    pub last_decision: Option<String>,
}
/// Durable client work, not a claim that the corresponding server effect failed.
#[derive(Clone, Copy, Debug)]
pub enum PendingSetup {
    DeviceRegistration,
    ContentKeyPublication,
    ServicePurchase,
    DirectoryPublication,
}
#[derive(Clone, Debug)]
pub struct Conversation {
    pub sent_requests: Vec<Request>,
    pub controls: RelationshipControls,
    pub threads: Vec<super::correspondence::ConversationSummary>,
    pub reply: super::correspondence::ReplyRoute,
    pub request_entry: RequestEntry,
    pub sender: String,
    pub title: String,
    pub permission: String,
    pub permission_route: PermissionRoute,
    pub external: bool,
    pub requests: Vec<Request>,
    pub messages: Vec<Message>,
}
/// A presentation entry, not authorization to form or fund a request. The service
/// rechecks the protected pair, selected class, history and policy at quote/formation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestEntry {
    Unverified,
    OwnAddress,
    AlreadyPermitted,
    ReviewApproaches,
    IncomingPending {
        request: cs_mail_primitives::RequestId,
    },
    OutgoingPending {
        request: cs_mail_primitives::RequestId,
    },
    Unavailable,
}
pub(super) fn request_entry(
    receiving: Option<&RelationshipView>,
    sending: Option<&RelationshipView>,
    now: cs_mail_primitives::CanonicalTime,
) -> RequestEntry {
    if let Some(request) = receiving.and_then(|view| {
        view.requests
            .iter()
            .find(|request| request.lifecycle.awaits_decision_at(now))
    }) {
        return RequestEntry::IncomingPending {
            request: request.id,
        };
    }
    if [receiving, sending]
        .into_iter()
        .flatten()
        .any(|view| view.state.sender_reply_restriction().is_some())
    {
        return RequestEntry::Unavailable;
    }
    if sending.is_some_and(|view| {
        view.state == RelationshipState::Accepted
            || view.lane.as_ref().is_some_and(|lane| lane.is_current(now))
    }) {
        return RequestEntry::AlreadyPermitted;
    }
    if let Some(request) = sending.and_then(|view| {
        view.requests.iter().find(|request| {
            request.lifecycle.is_preparing_submission() || request.lifecycle.awaits_decision_at(now)
        })
    }) {
        return RequestEntry::OutgoingPending {
            request: request.id,
        };
    }
    RequestEntry::ReviewApproaches
}

/// Current facts for contact management; commands are authorized again by the server.
#[derive(Clone, Debug)]
pub struct RelationshipControls {
    pub state: ContactState,
    pub revocable: bool,
    pub followup_max: u32,
    pub followup_per_interval: u32,
    pub followup_interval_ms: u64,
    pub lane: Option<String>,
    pub lane_expiry_reminder: Option<LaneExpiryReminder>,
}
#[derive(Clone, Copy, Debug)]
pub struct LaneExpiryReminder {
    pub expires_at: cs_mail_primitives::CanonicalTime,
}
#[derive(Clone, Copy, Debug)]
pub enum ContactState {
    Absent,
    Closed,
    Standing,
    Lane,
    Blocked,
    Revoked,
}
/// Presentation of the directional relationship arrangement, not send-time authority.
#[derive(Clone, Copy, Debug)]
pub enum PermissionRoute {
    Standing,
    Bounded,
    Closed,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub id: String,
    pub capture: Option<cs_mail_finance::CaptureStatus>,
    pub class_name: String,
    pub status: String,
    pub deadline: Option<u64>,
    pub awaiting_decision: bool,
    pub financial_status: String,
    pub has_pool_forfeiture: bool,
}
#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub conversation: Option<String>,
    pub outgoing: bool,
    pub text: String,
    pub references: Vec<u32>,
}

pub(super) fn inbox(state: &State) -> Result<Inbox, Error> {
    if state.inbox.len() > 10000 {
        return Err(Error::InvalidData);
    }
    Ok(Inbox {
        pending_request: state.requesting.is_some(),
        request_progress: super::MemberClient::request_progress_of(state)?,
        last_send: state.last_send.clone(),
        pending_policy: state.editing_conversation.is_some(),
        pending_send: super::MemberClient::pending_send_other(state)
            .map(|other| other.0.to_string()),
        account: state.profile.account.0.to_string(),
        persona: state.profile.persona.0.to_string(),
        synchronized_at: state.synchronized_at.map(|t| t.0),
        conversations: contact_summaries(state),
        pending_sender: state.pending.as_ref().map(|p| p.sender.0.to_string()),
        pending_setup: state.setup.as_ref().map(|setup| match setup {
            Setup::Register(_) => PendingSetup::DeviceRegistration,
            Setup::Publish => PendingSetup::ContentKeyPublication,
            Setup::Purchase(_) => PendingSetup::ServicePurchase,
            Setup::Directory(_) => PendingSetup::DirectoryPublication,
        }),
        last_decision: state.last_decision.clone(),
    })
}
fn contact_summaries(state: &State) -> Vec<Conversation> {
    let mut contacts: Vec<_> = state
        .inbox
        .iter()
        .map(|view| summary(view, state.synchronized_at))
        .collect();
    for other in super::MemberClient::mailbox_contacts(state) {
        if !contacts
            .iter()
            .any(|contact| contact.sender == other.0.to_string())
        {
            contacts.push(unestablished(other));
        }
    }
    contacts
}
const LANE_REMINDER_WINDOW: cs_mail_primitives::Duration =
    cs_mail_primitives::Duration(3 * 24 * 60 * 60 * 1000);

// Presentation policy confirmed by the owner. Time and evidence are explicit;
// this reminder neither grants permission nor changes the lane's expiry.
fn lane_expiry_reminder(
    view: &RelationshipView,
    now: cs_mail_primitives::CanonicalTime,
    window: cs_mail_primitives::Duration,
) -> Option<LaneExpiryReminder> {
    if !matches!(view.state, RelationshipState::ExpressLane(_)) {
        return None;
    }
    let lane = view.lane.as_ref()?;
    if !lane.is_current(now) {
        return None;
    }
    let expires_at = match lane.grant().mode {
        cs_mail_capabilities::LaneMode::Expiring => lane.horizon_at().min(lane.grant().not_after),
        cs_mail_capabilities::LaneMode::Persistent => lane.grant().not_after,
    };
    (expires_at.0.checked_sub(now.0)? <= window.0).then_some(LaneExpiryReminder { expires_at })
}

/// One time-scoped presentation rule for native and external directional permission.
pub(super) fn permission(
    state: RelationshipState,
    lane: Option<&cs_mail_capabilities::Lane>,
    now: Option<cs_mail_primitives::CanonicalTime>,
) -> (&'static str, PermissionRoute) {
    match state {
        RelationshipState::Unknown => ("No standing permission", PermissionRoute::Closed),
        RelationshipState::Accepted => ("Standing permission", PermissionRoute::Standing),
        RelationshipState::ExpressLane(id)
            if now.is_some_and(|now| {
                lane.is_some_and(|lane| lane.grant().id == id && lane.is_current(now))
            }) =>
        {
            (
                "Express lane — scope and validity apply",
                PermissionRoute::Bounded,
            )
        }
        RelationshipState::ExpressLane(_) => ("No active express lane", PermissionRoute::Closed),
        RelationshipState::Rejected => ("Rejected", PermissionRoute::Closed),
        RelationshipState::Revoked => ("Permission revoked", PermissionRoute::Closed),
        RelationshipState::Blocked => ("Blocked", PermissionRoute::Closed),
    }
}

pub(super) fn summary(
    view: &RelationshipView,
    now: Option<cs_mail_primitives::CanonicalTime>,
) -> Conversation {
    let (permission, permission_route) = permission(view.state, view.lane.as_ref(), now);
    Conversation {
        sent_requests: Vec::new(),
        request_entry: RequestEntry::Unverified,
        controls: RelationshipControls {
            state: match view.state {
                RelationshipState::Unknown | RelationshipState::Rejected => ContactState::Closed,
                RelationshipState::Accepted => ContactState::Standing,
                RelationshipState::ExpressLane(_) => ContactState::Lane,
                RelationshipState::Blocked => ContactState::Blocked,
                RelationshipState::Revoked => ContactState::Revoked,
            },
            revocable: !matches!(view.state, RelationshipState::Blocked | RelationshipState::Revoked),
            followup_max: view.followup_policy.max_messages,
            followup_per_interval: view.followup_policy.max_per_interval,
            followup_interval_ms: view.followup_policy.interval.0,
            lane_expiry_reminder: now.and_then(|now| lane_expiry_reminder(view, now, LANE_REMINDER_WINDOW)),
            lane: view.lane.as_ref().map(|lane| format!("{:?}; {} messages per {} seconds; {} used in the current window; expires at {}", lane.state(), lane.grant().rate_limit.max_messages, lane.grant().rate_limit.interval.0 / 1000, lane.messages_in_rate_window(), lane.grant().not_after.0)),
        },
        threads:Vec::new(),reply:super::correspondence::ReplyRoute::Closed,
        sender: view.handle.sender.0.to_string(),
        title: view.guest.as_ref().map_or_else(
            || view.display_name.clone().unwrap_or_else(|| format!("Member {}", view.handle.sender.0)),
            |m| m.address().clone(),
        ),
        permission: permission.into(),
        permission_route,
        external: view.guest.is_some(),
        messages: Vec::new(),
        requests: project_requests(view, now),
    }
}
pub(super) fn project_requests(
    view: &RelationshipView,
    now: Option<cs_mail_primitives::CanonicalTime>,
) -> Vec<Request> {
    view.requests
        .iter()
        .map(|r| {
            let status = match r.lifecycle {
                RequestLifecycle::PreparingSubmission(_) => "Preparing",
                RequestLifecycle::AwaitingRecipientDecision(_)
                    if now.is_some_and(|now| !r.lifecycle.awaits_decision_at(now)) =>
                {
                    "Decision window ended"
                }
                RequestLifecycle::AwaitingRecipientDecision(_) => "Awaiting your decision",
                RequestLifecycle::Accepted { .. } => "Accepted",
                RequestLifecycle::Rejected { .. } => "Rejected",
                RequestLifecycle::Expired { .. } => "Expired",
                RequestLifecycle::Cancelled { .. } => "Cancelled",
            };
            let financial_status = r.money.as_ref().map_or_else(
                || "Financial status unavailable".into(),
                |money| {
                    let units = money.charge.minor_units();
                    let refund = match money.refund {
                        RefundView::None => "No refund recorded".to_owned(),
                        RefundView::Owed(amount) => format!(
                            "Refund owed: {} minor units; not yet confirmed",
                            amount.minor_units()
                        ),
                        RefundView::Confirmed(amount) => {
                            format!("Refund confirmed: {} minor units", amount.minor_units())
                        }
                    };
                    let capture = match money.capture {
                        cs_mail_finance::CaptureStatus::Pending => "Funding outcome pending",
                        cs_mail_finance::CaptureStatus::CancellationRequested => {
                            "Funding cancellation requested; outcome pending"
                        }
                        cs_mail_finance::CaptureStatus::FundingFinalized => "Funding finalized",
                        cs_mail_finance::CaptureStatus::Voided => {
                            "Funding voided; no charge collected"
                        }
                        cs_mail_finance::CaptureStatus::Reversed => "Collected funding reversed",
                    };
                    let mut lines = vec![format!(
                        "Authorized amount: {units} minor units · {capture} · {refund}"
                    )];
                    if !money.processing_retained.is_zero() {
                        lines.push(format!(
                            "Processing retained by CSQD: {} minor units",
                            money.processing_retained.minor_units()
                        ));
                    }
                    if !money.collateral_forfeited.is_zero() {
                        lines.push(format!(
                            "Collateral pending forfeiture to the member program: {} minor units",
                            money.collateral_forfeited.minor_units()
                        ));
                    }
                    if money.awaiting_processor {
                        lines.push("Processor outcome pending".into());
                    }
                    lines.join("\n")
                },
            );
            Request {
                id: r.id.0.to_string(),
                capture: r.money.as_ref().map(|money| money.capture),
                class_name: r.terms.selected_class.description().to_owned(),
                status: status.into(),
                deadline: r.lifecycle.submission().map(|s| s.decision_deadline.0),
                awaiting_decision: now.is_some_and(|now| r.lifecycle.awaits_decision_at(now)),
                financial_status,
                has_pool_forfeiture: r
                    .money
                    .as_ref()
                    .is_some_and(|money| !money.collateral_forfeited.is_zero()),
            }
        })
        .collect()
}
pub(super) fn unestablished(other: cs_mail_primitives::ProtocolIdentity) -> Conversation {
    Conversation {
        sent_requests: Vec::new(),
        request_entry: RequestEntry::Unverified,
        controls: RelationshipControls {
            state: ContactState::Absent,
            revocable: false,
            followup_max: 0,
            followup_per_interval: 0,
            followup_interval_ms: 0,
            lane: None,
            lane_expiry_reminder: None,
        },
        threads: Vec::new(),
        reply: super::ReplyRoute::Closed,
        sender: other.0.to_string(),
        title: format!("Member {}", other.0),
        permission: "No incoming permission recorded".into(),
        permission_route: PermissionRoute::Closed,
        external: false,
        requests: Vec::new(),
        messages: Vec::new(),
    }
}
pub(super) fn conversation(
    profile: &Profile,
    keys: &Secrets,
    view: &RelationshipView,
    content: &[GuestDeliveredContent],
    now: cs_mail_primitives::CanonicalTime,
) -> Result<Conversation, Error> {
    let (secret, _) =
        cs_mail_content::EndpointSecretKey::from_secret_bytes(profile.content_key, &keys.content)
            .map_err(|_| Error::InvalidData)?;
    let mut result = summary(view, Some(now));
    for delivered in content {
        let record = &delivered.record;
        let certificate = delivered.certificate.as_ref().ok_or(Error::InvalidData)?;
        if record.binding.sender != view.handle.sender
            || record.binding.recipient != profile.persona
            || record.binding.relationship != view.handle.relationship
            || record.binding.content_scope != view.handle.content_scope
            || certificate.certificate.owner != view.handle.sender
            || certificate.certificate.deployment_domain
                != profile.deployment.scope.deployment_domain
            || certificate.certificate.intended_provider != profile.deployment.scope.provider
            || cs_mail_content::content_key_certificate_digest(&certificate.certificate)
                != record.binding.sender_certificate
        {
            return Err(Error::InvalidData);
        }
        // The authenticated service returned an already admitted record and its certificate.
        // Binding and HPKE authentication are checked again before releasing plaintext.
        let plain = zeroize::Zeroizing::new(
            cs_mail_content::decrypt(&secret, certificate.certificate.key, record)
                .map_err(|_| Error::InvalidData)?,
        );
        let text = std::str::from_utf8(&plain)
            .map_err(|_| Error::InvalidData)?
            .to_owned();
        result.messages.push(Message {
            conversation: None,
            outgoing: false,
            id: record.binding.message_id.0.to_string(),
            text,
            references: Vec::new(),
        });
    }
    Ok(result)
}
