//! The email notices a guest receives about a request: each is owed once, when the
//! request's recorded state first shows it.
use cs_mail_finance::{RefundView, RequestFinancialView};
use cs_mail_primitives::{Money, RequestId};
use cs_mail_protocol::RequestLifecycle;
use serde::{Deserialize, Serialize};

/// A transactional email is distinct from a member-authored message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct NoticeId(u128);
impl NoticeId {
    pub const fn value(self) -> u128 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidNoticeId;
impl core::fmt::Display for InvalidNoticeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("notice identifier must be nonzero")
    }
}
impl std::error::Error for InvalidNoticeId {}
impl TryFrom<u128> for NoticeId {
    type Error = InvalidNoticeId;
    fn try_from(value: u128) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvalidNoticeId);
        }
        Ok(Self(value))
    }
}
impl From<NoticeId> for u128 {
    fn from(value: NoticeId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum GuestOutcome {
    /// Delivered to the recipient; the decision window runs.
    Submitted,
    Accepted,
    Rejected,
    Expired,
    Cancelled,
    /// The processor confirmed a refund of this amount to the funding payment method.
    RefundConfirmed(Money),
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuestNotice {
    pub request: RequestId,
    pub outcome: GuestOutcome,
}

/// Every notice the request's current state warrants. Sending is idempotent per notice.
pub fn notices_for(
    request: RequestId,
    lifecycle: &RequestLifecycle,
    money: Option<&RequestFinancialView>,
) -> Vec<GuestNotice> {
    let mut outcomes = Vec::new();
    match lifecycle {
        RequestLifecycle::PreparingSubmission(_) => {}
        RequestLifecycle::AwaitingRecipientDecision(_) => outcomes.push(GuestOutcome::Submitted),
        RequestLifecycle::Accepted { .. } => {
            outcomes.extend([GuestOutcome::Submitted, GuestOutcome::Accepted]);
        }
        RequestLifecycle::Rejected { .. } => {
            outcomes.extend([GuestOutcome::Submitted, GuestOutcome::Rejected]);
        }
        RequestLifecycle::Expired { .. } => {
            outcomes.extend([GuestOutcome::Submitted, GuestOutcome::Expired]);
        }
        RequestLifecycle::Cancelled { .. } => outcomes.push(GuestOutcome::Cancelled),
    }
    if let Some(RefundView::Confirmed(amount)) = money.map(|m| m.refund) {
        outcomes.push(GuestOutcome::RefundConfirmed(amount));
    }
    outcomes
        .into_iter()
        .map(|outcome| GuestNotice { request, outcome })
        .collect()
}
