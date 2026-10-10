//! Presentation projections over existing domain facts; no transition decisions.
use super::{Error, Profile, State, storage::Secrets};
use cs_mail_application::members::{DeliveredContent, RelationshipView};
use cs_mail_finance::RefundView;
use cs_mail_protocol::{RelationshipState, RequestLifecycle};

#[derive(Clone, Debug)]
pub struct Inbox {
    pub account: String,
    pub persona: String,
    pub synchronized_at: Option<u64>,
    pub conversations: Vec<Conversation>,
    pub pending_sender: Option<String>,
    pub last_decision: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Conversation {
    pub sender: String,
    pub title: String,
    pub permission: String,
    pub external: bool,
    pub requests: Vec<Request>,
    pub messages: Vec<Message>,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub id: String,
    pub class_name: String,
    pub status: String,
    pub deadline: Option<u64>,
    pub awaiting_decision: bool,
    pub financial_status: String,
}
#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub text: String,
}

pub(super) fn inbox(state: &State) -> Result<Inbox, Error> {
    if state.inbox.len() > 10000 {
        return Err(Error::InvalidData);
    }
    Ok(Inbox {
        account: state.profile.account.0.to_string(),
        persona: state.profile.persona.0.to_string(),
        synchronized_at: state.synchronized_at.map(|t| t.0),
        conversations: state.inbox.iter().map(summary).collect(),
        pending_sender: state.pending.as_ref().map(|p| p.sender.0.to_string()),
        last_decision: state.last_decision.clone(),
    })
}
fn summary(view: &RelationshipView) -> Conversation {
    let permission = match view.state {
        RelationshipState::Unknown => "No standing permission",
        RelationshipState::Accepted => "Standing permission",
        RelationshipState::ExpressLane(_) => "Express lane — scope and validity apply",
        RelationshipState::Rejected => "Rejected",
        RelationshipState::Revoked => "Permission revoked",
        RelationshipState::Blocked => "Blocked",
    };
    Conversation {
        sender: view.handle.sender.0.to_string(),
        title: view.guest.as_ref().map_or_else(
            || format!("Member {}", view.handle.sender.0),
            |m| m.address().clone(),
        ),
        permission: permission.into(),
        external: view.guest.is_some(),
        messages: Vec::new(),
        requests: view
            .requests
            .iter()
            .map(|r| {
                let status = match r.lifecycle {
                    RequestLifecycle::PreparingSubmission(_) => "Preparing",
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
                            RefundView::None => "No refund recorded",
                            RefundView::Owed(_) => "Refund owed; not yet confirmed",
                            RefundView::Confirmed(_) => "Refund confirmed",
                        };
                        format!(
                            "Charge {units} minor units · {refund}{}",
                            if money.awaiting_processor {
                                " · Processor outcome pending"
                            } else {
                                ""
                            }
                        )
                    },
                );
                Request {
                    id: r.id.0.to_string(),
                    class_name: r.terms.selected_class.description().to_owned(),
                    status: status.into(),
                    deadline: r.lifecycle.submission().map(|s| s.decision_deadline.0),
                    awaiting_decision: r.lifecycle.is_awaiting_recipient_decision(),
                    financial_status,
                }
            })
            .collect(),
    }
}
pub(super) fn conversation(
    profile: &Profile,
    keys: &Secrets,
    view: &RelationshipView,
    content: &[DeliveredContent],
) -> Result<Conversation, Error> {
    let (secret, _) =
        cs_mail_content::EndpointSecretKey::from_secret_bytes(profile.content_key, &keys.content)
            .map_err(|_| Error::InvalidData)?;
    let mut result = summary(view);
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
            id: record.binding.message_id.0.to_string(),
            text,
        });
    }
    Ok(result)
}
