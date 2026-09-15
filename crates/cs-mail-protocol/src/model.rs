//! Request, relationship, history, and message records. Financial records live in finance.
use cs_mail_finance::FinancialTerms;
use cs_mail_primitives::PaymentOperationId;
use std::collections::{BTreeMap, BTreeSet};

use cs_mail_ledger::LedgerView;
use cs_mail_primitives::{
    CanonicalTime, ContentRef, DeliveryIntentRef, Duration, EventRef, LaneId,
    MessageDeclarationDigest, MessageId, MessageValidityUntil, Money, PolicyVersion, PrincipalRef,
    PrivacyProfileVersion, ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId,
    RelationshipRef, RelationshipVersion, RequestHistoryRef, RequestHistoryVersion, RequestId,
    RequestVersion, RetentionPolicyVersion, SettlementUnit, Version,
};
use serde::{Deserialize, Serialize};

use crate::ProtocolError;
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RelationshipKey {
    pub reference: RelationshipRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum RelationshipState {
    Unknown,
    Accepted,
    Rejected,
    Revoked,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Relationship {
    pub key: RelationshipKey,
    pub history: RequestHistoryRef,
    pub generation: u64,
    pub state: RelationshipState,
    pub version: RelationshipVersion,
    pub last_event: Option<EventRef>,
    pub changed_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestReservation {
    pub relationship: RelationshipRef,
    pub request: RequestId,
}

/// Shared by every sender alias of a principal addressing the same recipient.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestHistory {
    pub reference: RequestHistoryRef,
    pub recipient: ProtocolIdentity,
    pub level: u32,

    pub earliest_next_submission: CanonicalTime,

    pub pending_submission: Option<RequestReservation>,
    pub version: RequestHistoryVersion,
    pub last_event: Option<EventRef>,
    pub changed_at: CanonicalTime,
}
impl RequestHistory {
    pub fn new(
        reference: RequestHistoryRef,
        recipient: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Self {
        Self {
            reference,
            recipient,
            level: 0,
            earliest_next_submission: now,
            pending_submission: None,
            version: RequestHistoryVersion::default(),
            last_event: None,
            changed_at: now,
        }
    }
    pub(crate) fn record_submission(
        &mut self,
        reservation: RequestReservation,
        at: CanonicalTime,
        backoff: Duration,
        event: EventRef,
    ) -> Result<(), ProtocolError> {
        if self.pending_submission != Some(reservation) {
            return Err(ProtocolError::QuoteVersionStale);
        }
        let next_level = self
            .level
            .checked_add(1)
            .ok_or(ProtocolError::ArithmeticOverflow)?;
        self.extend_cooldown(at, backoff, at, event)?;
        self.level = next_level;
        self.pending_submission = None;
        Ok(())
    }
    pub(crate) fn extend_cooldown(
        &mut self,
        from: CanonicalTime,
        duration: Duration,
        now: CanonicalTime,
        event: EventRef,
    ) -> Result<(), ProtocolError> {
        let next = from
            .checked_add(duration)
            .ok_or(ProtocolError::ArithmeticOverflow)?;
        let version = self
            .version
            .checked_next()
            .ok_or(ProtocolError::ArithmeticOverflow)?;
        self.earliest_next_submission = self.earliest_next_submission.max(next);
        self.version = version;
        self.last_event = Some(event);
        self.changed_at = now;
        Ok(())
    }
}

/// The time allowed to complete funding and submit the initial message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SubmissionPreparation {
    pub deadline: CanonicalTime,
}
/// The provider's commitment to deliver the request and await the recipient's decision.
/// Submission does not establish transport completion or reading.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestSubmission {
    pub at: CanonicalTime,
    pub decision_deadline: CanonicalTime,
}
/// Every submitted outcome retains its submission facts; cancellation has none.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RequestLifecycle {
    /// Funding and the initial message are prepared here; submission has not yet committed.
    PreparingSubmission(SubmissionPreparation),
    /// Initial delivery is committed and the recipient decision window has started.
    AwaitingRecipientDecision(RequestSubmission),
    Accepted {
        submission: RequestSubmission,
        event: EventRef,
    },
    Rejected {
        submission: RequestSubmission,
        event: EventRef,
    },
    Expired {
        submission: RequestSubmission,
        event: EventRef,
    },
    Cancelled {
        submission_preparation: SubmissionPreparation,
        event: EventRef,
    },
}
impl RequestLifecycle {
    pub const fn is_terminal(self) -> bool {
        !matches!(
            self,
            Self::PreparingSubmission(_) | Self::AwaitingRecipientDecision(_)
        )
    }
    pub const fn is_awaiting_recipient_decision(self) -> bool {
        matches!(self, Self::AwaitingRecipientDecision(_))
    }
    pub const fn is_preparing_submission(self) -> bool {
        matches!(self, Self::PreparingSubmission(_))
    }
    pub const fn submission_preparation(self) -> Option<SubmissionPreparation> {
        match self {
            Self::PreparingSubmission(p)
            | Self::Cancelled {
                submission_preparation: p,
                ..
            } => Some(p),
            _ => None,
        }
    }
    pub const fn submission(self) -> Option<RequestSubmission> {
        match self {
            Self::AwaitingRecipientDecision(a)
            | Self::Accepted { submission: a, .. }
            | Self::Rejected { submission: a, .. }
            | Self::Expired { submission: a, .. } => Some(a),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RequestTerms {
    pub pricing_policy_version: PolicyVersion,
    pub quote_id: QuoteId,
    pub protocol_version: ProtocolVersion,
    pub policy_version: PolicyVersion,
    pub privacy_profile_version: PrivacyProfileVersion,
    pub retention_policy_version: RetentionPolicyVersion,
    pub relationship: RelationshipRef,
    pub request_history: RequestHistoryRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub recipient_provider: ProviderRef,
    pub relationship_version: RelationshipVersion,
    pub history_version: RequestHistoryVersion,
    pub processing_charge: Money,
    pub collateral: Money,
    pub request_level: u32,
    pub eligibility_time: CanonicalTime,
    pub unit: SettlementUnit,

    pub submission_window: Duration,
    pub decision_window: Duration,
    pub issued_at: CanonicalTime,
    pub expires_at: CanonicalTime,
    pub declaration_digest: Option<MessageDeclarationDigest>,
    pub financial: FinancialTerms,
    pub payment_provider_key: [u8; 32],
    pub expiry_cooldown: Duration,
    pub rejection_cooldown: Duration,
    pub next_request_backoff: Duration,
}

impl RequestTerms {
    /// Returns the `C + S` request portion of the reservation.
    ///
    /// # Errors
    ///
    /// Returns `ArithmeticOverflow` if the quoted values cannot be added.
    pub fn charge_amount(&self) -> Result<Money, ProtocolError> {
        self.processing_charge
            .checked_add(self.collateral)
            .ok_or(ProtocolError::ArithmeticOverflow)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelationshipRequest {
    pub id: RequestId,
    pub generation: u64,
    pub initial_message: MessageId,
    pub funding: PaymentOperationId,
    pub terms: RequestTerms,
    pub created_at: CanonicalTime,
    pub lifecycle: RequestLifecycle,
    pub version: RequestVersion,
}
impl RelationshipRequest {
    pub const fn reservation(&self) -> RequestReservation {
        RequestReservation {
            relationship: self.terms.relationship,
            request: self.id,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdmissionBasis {
    InitialRequest {
        request: RequestId,
    },
    RequestFollowup {
        request: RequestId,
        policy_version: Version,
    },
    AcceptedRelationship {
        version: RelationshipVersion,
    },
    ExpressLane {
        lane: LaneId,
        version: Version,
    },
}
/// Delivery facts have one representation regardless of the authorizing route.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Message {
    pub id: MessageId,
    pub relationship: RelationshipRef,
    pub content: ContentRef,
    pub delivery: DeliveryIntentRef,
    pub declaration: MessageDeclarationDigest,
    pub valid_until: MessageValidityUntil,
    pub admitted_at: CanonicalTime,
    pub basis: AdmissionBasis,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FollowupPolicy {
    pub version: Version,
    pub max_messages: u32,
    pub max_per_interval: u32,
    pub interval: Duration,
}
impl Default for FollowupPolicy {
    fn default() -> Self {
        Self {
            version: Version(0),
            max_messages: 0,
            max_per_interval: 0,
            interval: Duration(0),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolState {
    pub followup_policy: FollowupPolicy,
    pub quotes: BTreeMap<QuoteId, RequestTerms>,
    pub used_quotes: BTreeSet<QuoteId>,
    pub relationship: Relationship,
    pub requests: BTreeMap<RequestId, RelationshipRequest>,
}

impl ProtocolState {
    /// The active solicitation is a projection of the one unresolved request.
    pub fn active_request(&self) -> Option<&RelationshipRequest> {
        self.requests.values().find(|r| !r.lifecycle.is_terminal())
    }

    pub fn initial_scoped(
        relationship: RelationshipRef,
        request_history: RequestHistoryRef,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Self {
        Self {
            followup_policy: FollowupPolicy::default(),
            quotes: BTreeMap::new(),
            used_quotes: BTreeSet::new(),
            relationship: Relationship {
                history: request_history,
                generation: 0,
                key: RelationshipKey {
                    reference: relationship,
                    sender,
                    recipient,
                },
                state: RelationshipState::Unknown,
                version: RelationshipVersion::default(),
                last_event: None,
                changed_at: now,
            },
            requests: BTreeMap::new(),
        }
    }

    pub fn initial(
        principal: PrincipalRef,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        now: CanonicalTime,
    ) -> Self {
        Self::initial_scoped(
            RelationshipRef::from_u128_for_test(sender.0 ^ recipient.0),
            RequestHistoryRef::from_u128_for_test(principal.0 ^ recipient.0),
            sender,
            recipient,
            now,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettlementSnapshot {
    pub revision: u64,
    pub state: ProtocolState,
    pub history: RequestHistory,
    pub payments: BTreeMap<RequestId, cs_mail_finance::RequestFinancials>,
    pub messages: BTreeMap<MessageId, Message>,
    pub ledger: LedgerView,
    pub(super) complete: bool,
}

impl SettlementSnapshot {
    /// Groups initial and follow-up messages without storing another solicitation object.
    pub fn request_messages(&self, id: RequestId) -> impl Iterator<Item = &Message> {
        self.messages.values().filter(move |message| matches!(message.basis,
            AdmissionBasis::InitialRequest { request } | AdmissionBasis::RequestFollowup { request, .. } if request == id))
    }

    pub fn complete(
        revision: u64,
        state: ProtocolState,
        history: RequestHistory,
        payments: BTreeMap<RequestId, cs_mail_finance::RequestFinancials>,
        messages: BTreeMap<MessageId, Message>,
        ledger: LedgerView,
    ) -> Self {
        Self {
            revision,
            state,
            history,
            payments,
            messages,
            ledger,
            complete: true,
        }
    }

    #[cfg(test)]
    pub(super) fn incomplete(revision: u64, state: ProtocolState, ledger: LedgerView) -> Self {
        let history = RequestHistory::new(
            state.relationship.history,
            state.relationship.key.recipient,
            CanonicalTime(0),
        );
        let mut snapshot = Self::complete(
            revision,
            state,
            history,
            BTreeMap::new(),
            BTreeMap::new(),
            ledger,
        );
        snapshot.complete = false;
        snapshot
    }
}
