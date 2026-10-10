//! Transactional email intents. They confer neither communication permission nor
//! financial authority. The existing handoff lifecycle owns delivery uncertainty.
use cs_mail_correspondence::{CorrespondenceError, external::delivery::DeliveryState};
use cs_mail_guests::{GuestMailbox, GuestNotice, NoticeId};
use cs_mail_primitives::{CanonicalTime, ProtocolIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NoticePurpose {
    /// The code is never retained here. Issuance commits the attempt before sending;
    /// interrupted issuance requires a fresh challenge, never a second submission.
    Verification { challenge: String },
    Outcome {
        relationship: String,
        member: ProtocolIdentity,
        notice: GuestNotice,
    },
    /// A verified sender has no permission to deliver to this member.
    RequestLink {
        member: ProtocolIdentity,
        source: [u8; 32],
    },
}

/// Operational mail-loop bound, separate from paid-request history or permission.
pub fn request_link_due(last: Option<CanonicalTime>, now: CanonicalTime) -> bool {
    last.is_none_or(|last| now.0.saturating_sub(last.0) >= 86_400_000)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeRecord {
    pub id: NoticeId,
    pub recipient: GuestMailbox,
    pub purpose: NoticePurpose,
    pub created_at: CanonicalTime,
}
impl NoticeRecord {
    /// Stable identity binds the complete purpose and destination within a deployment.
    /// # Errors
    /// Refuses unencodable intent facts.
    pub fn new(
        deployment: &[u8; 32],
        recipient: GuestMailbox,
        purpose: NoticePurpose,
        now: CanonicalTime,
    ) -> Result<Self, CorrespondenceError> {
        let bytes = serde_json::to_vec(&(deployment, &recipient, &purpose))
            .map_err(|_| CorrespondenceError::Invalid)?;
        let hash = Sha256::new()
            .chain_update(b"cs-mail/transactional-notice/v1")
            .chain_update(bytes)
            .finalize();
        let mut id = [0; 16];
        id.copy_from_slice(&hash[..16]);
        Ok(Self {
            id: NoticeId::try_from(u128::from_be_bytes(id) | 1)
                .map_err(|_| CorrespondenceError::Invalid)?,
            recipient,
            purpose,
            created_at: now,
        })
    }
    /// Timestamps are observations, not part of an operation's replay identity.
    pub fn same_intent(&self, other: &Self) -> bool {
        self.id == other.id && self.recipient == other.recipient && self.purpose == other.purpose
    }
}

/// # Errors
/// Refuses a colliding or altered intent; an existing attempt must not be sent again.
pub fn reserve(
    record: &NoticeRecord,
    prior: Option<&NoticeRecord>,
    now: CanonicalTime,
) -> Result<Option<DeliveryState>, CorrespondenceError> {
    if let Some(prior) = prior {
        return if prior.same_intent(record) {
            Ok(None)
        } else {
            Err(CorrespondenceError::Conflict)
        };
    }
    DeliveryState::Waiting.begin(now).map(Some)
}

/// The acceptance notice and its reply route identify one paid request, including
/// across worker restarts. They use different identity domains.
pub fn paid_conversation(record: &NoticeRecord) -> Option<cs_mail_correspondence::ConversationId> {
    if !matches!(
        record.purpose,
        NoticePurpose::Outcome {
            notice: GuestNotice {
                outcome: cs_mail_guests::GuestOutcome::Accepted,
                ..
            },
            ..
        }
    ) {
        return None;
    }
    let hash = Sha256::new()
        .chain_update(b"cs-mail/paid-guest-conversation/v1")
        .chain_update(record.id.value().to_be_bytes())
        .finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&hash[..16]);
    cs_mail_correspondence::ConversationId::new(u128::from_be_bytes(id) | 1).ok()
}
