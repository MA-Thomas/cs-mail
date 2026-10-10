//! Guest senders: people without an account who approach a member through the member's
//! request page (see `docs/guest-sender-design.md`).
//!
//! A guest is identified by one verified mailbox, mapped at the edge to a synthetic
//! protocol identity so the request kernel applies unchanged. This crate owns the rules
//! around that mapping: what a mailbox is, which sender subject an identity belongs to,
//! what a mailbox verification establishes, and how guest request histories are linked
//! by mailbox and payment-instrument evidence. Every rule is deterministic: time, codes
//! and evidence are passed in.
use cs_mail_adapters::DomainIdentity;
use cs_mail_primitives::{
    AccountId, CanonicalTime, IdempotencyKey, ProtocolIdentity, RequestHistoryRef,
};
use cs_mail_protocol::{CancellationReason, ProtocolCommand, RequestHistory};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod notices;
pub use notices::{GuestNotice, GuestOutcome, InvalidNoticeId, NoticeId, notices_for};
mod verification;
pub use verification::{
    CodeCheck, GuestSession, MailboxVerification, SESSION_LIFETIME, SessionTokenDigest,
    VERIFICATION_ATTEMPTS, VERIFICATION_LIFETIME, VerificationCompletion,
};
mod request;
pub use request::{GuestRequestFingerprint, GuestRequestInput};

/// Version of the mailbox-to-identity mapping.
pub const MAILBOX_MAPPING_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuestError {
    /// Not a single ASCII mailbox address.
    InvalidMailbox,
    /// A restored verification or its timestamps violates the domain invariants.
    InvalidVerificationState,
    /// The verification has expired or was already used.
    VerificationClosed,
    /// Too many wrong codes were entered for this verification.
    TooManyAttempts,
    /// The code does not match.
    WrongCode,
    /// The session has expired.
    SessionExpired,
    /// Linking this evidence would combine two histories that each hold a pending request.
    PendingRequestsConflict,
    /// The guest already has an undecided request to this recipient under this mailbox.
    PendingRequest,
    /// Histories for different recipients cannot be linked.
    RecipientMismatch,
    /// Too many verification codes were requested for this mailbox recently.
    RateLimited,
    /// A guest may issue only a sender's own request commands.
    NotAGuestCommand,
    /// An operation identity was already bound to different request inputs.
    AlteredRequest,
}
impl core::fmt::Display for GuestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidMailbox => "not a valid mailbox address",
            Self::InvalidVerificationState => "invalid mailbox verification state",
            Self::VerificationClosed => "the verification has expired or was used",
            Self::TooManyAttempts => "too many wrong codes",
            Self::WrongCode => "the code does not match",
            Self::SessionExpired => "the session has expired",
            Self::PendingRequestsConflict => "two linked histories each hold a pending request",
            Self::PendingRequest => "an undecided request to this recipient exists",
            Self::RecipientMismatch => "histories for different recipients",
            Self::RateLimited => "too many verification codes requested",
            Self::NotAGuestCommand => "not a command a guest may issue",
            Self::AlteredRequest => "request operation reused with changed inputs",
        })
    }
}
impl std::error::Error for GuestError {}

/// One exact mailbox address. The domain is compared case-insensitively and the local
/// part exactly, as mail systems are required to treat them. Unicode addresses must be
/// converted to ASCII before they reach this type.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct GuestMailbox {
    local: String,
    domain: DomainIdentity,
}
impl TryFrom<String> for GuestMailbox {
    type Error = GuestError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<GuestMailbox> for String {
    fn from(mailbox: GuestMailbox) -> Self {
        mailbox.address()
    }
}
impl GuestMailbox {
    /// # Errors
    /// `InvalidMailbox` for anything but `local@domain` with a printable ASCII local part
    /// of at most 64 characters and a valid ASCII domain.
    pub fn parse(input: &str) -> Result<Self, GuestError> {
        let input = input.trim();
        let (local, domain) = input.rsplit_once('@').ok_or(GuestError::InvalidMailbox)?;
        if local.is_empty()
            || local.len() > 64
            || !local
                .bytes()
                .all(|b| b.is_ascii_graphic() && !b"@\"(),:;<>[\\]".contains(&b))
        {
            return Err(GuestError::InvalidMailbox);
        }
        Ok(Self {
            local: local.to_owned(),
            domain: DomainIdentity::parse_ascii(domain).map_err(|_| GuestError::InvalidMailbox)?,
        })
    }
    pub fn address(&self) -> String {
        format!("{}@{}", self.local, self.domain.as_str())
    }
    pub const fn domain(&self) -> &DomainIdentity {
        &self.domain
    }
    /// The protocol identity this mailbox writes as. Its separation string keeps guest
    /// identities apart from legacy-domain identities of the same text.
    pub fn synthetic_protocol_identity(&self, deployment_domain: &[u8; 32]) -> ProtocolIdentity {
        let mut hasher = Sha256::new();
        hasher.update(b"cs-mail/guest-mailbox-identity/v1");
        hasher.update(deployment_domain);
        hasher.update(MAILBOX_MAPPING_VERSION.to_be_bytes());
        hasher.update(self.address().as_bytes());
        let digest = hasher.finalize();
        let mut identity = [0; 16];
        identity.copy_from_slice(&digest[..16]);
        ProtocolIdentity(u128::from_be_bytes(identity) | 1)
    }
}

/// Who a protocol identity belongs to. Each identity has exactly one subject.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SenderSubject {
    /// An address of an enrolled member account.
    Member(AccountId),
    /// An organization's DMARC-authenticated domain.
    LegacyDomain(DomainIdentity),
    /// A person without an account, by verified mailbox.
    Guest(GuestMailbox),
}
impl SenderSubject {
    /// A key unique to the subject, so one subject cannot hold two identities either.
    /// Addresses of one account are distinct identities of one subject, so a member's key
    /// names the address.
    pub fn key(&self, identity: ProtocolIdentity) -> String {
        match self {
            Self::Member(account) => format!("member:{}:{}", account.0, identity.0),
            Self::LegacyDomain(domain) => format!("legacy-domain:{}", domain.as_str()),
            Self::Guest(mailbox) => format!("guest:{}", mailbox.address()),
        }
    }
    pub const fn is_guest(&self) -> bool {
        matches!(self, Self::Guest(_))
    }
}

/// The payment processor's stable fingerprint of a card or account: an anti-abuse
/// signal, not an identity claim.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InstrumentFingerprint(pub [u8; 32]);

/// Evidence a guest request history is linked by, for one recipient.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoryEvidence {
    Mailbox(GuestMailbox),
    Fingerprint(InstrumentFingerprint),
}
impl HistoryEvidence {
    pub fn key(&self) -> String {
        match self {
            Self::Mailbox(mailbox) => format!("mailbox:{}", mailbox.address()),
            Self::Fingerprint(fingerprint) => {
                fingerprint
                    .0
                    .iter()
                    .fold(String::from("fingerprint:"), |mut key, b| {
                        use core::fmt::Write as _;
                        let _ = write!(key, "{b:02x}");
                        key
                    })
            }
        }
    }
}

/// Which history a guest request uses, and what must change for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryDecision {
    /// The history the request's relationship uses.
    pub history: RequestHistoryRef,
    /// A history to create: a fresh one, or the merge of two linked ones.
    pub create: Option<RequestHistory>,
    /// The two histories merged into `history`, when the evidence linked them.
    pub merged: Option<[RequestHistoryRef; 2]>,
    /// Evidence not yet linked to `history`.
    pub link: Vec<HistoryEvidence>,
}

/// Resolves the request history for a guest approaching `recipient` with `mailbox` and
/// `fingerprint`. Evidence already linked to a history selects it; evidence linking two
/// histories merges them prospectively into the most restrictive state: the highest
/// level, the latest next-submission time, and any pending reservation.
/// # Errors
/// `PendingRequestsConflict` when both linked histories hold a pending request: the new
/// request is refused and nothing is linked or merged. `RecipientMismatch` for a history
/// of another recipient.
pub fn resolve_history(
    recipient: ProtocolIdentity,
    mailbox: (&GuestMailbox, Option<&RequestHistory>),
    fingerprint: (InstrumentFingerprint, Option<&RequestHistory>),
    fresh: RequestHistoryRef,
    merged: RequestHistoryRef,
    now: CanonicalTime,
) -> Result<HistoryDecision, GuestError> {
    let mailbox_evidence = HistoryEvidence::Mailbox(mailbox.0.clone());
    let fingerprint_evidence = HistoryEvidence::Fingerprint(fingerprint.0);
    for history in [mailbox.1, fingerprint.1].into_iter().flatten() {
        if history.recipient != recipient {
            return Err(GuestError::RecipientMismatch);
        }
    }
    Ok(match (mailbox.1, fingerprint.1) {
        (None, None) => HistoryDecision {
            history: fresh,
            create: Some(RequestHistory::new(fresh, recipient, now)),
            merged: None,
            link: vec![mailbox_evidence, fingerprint_evidence],
        },
        (Some(one), None) => HistoryDecision {
            history: one.reference,
            create: None,
            merged: None,
            link: vec![fingerprint_evidence],
        },
        (None, Some(one)) => HistoryDecision {
            history: one.reference,
            create: None,
            merged: None,
            link: vec![mailbox_evidence],
        },
        (Some(a), Some(b)) if a.reference == b.reference => HistoryDecision {
            history: a.reference,
            create: None,
            merged: None,
            link: Vec::new(),
        },
        (Some(a), Some(b)) => {
            if a.pending_submission.is_some() && b.pending_submission.is_some() {
                return Err(GuestError::PendingRequestsConflict);
            }
            HistoryDecision {
                history: merged,
                create: Some(RequestHistory {
                    reference: merged,
                    recipient,
                    level: a.level.max(b.level),
                    earliest_next_submission: a
                        .earliest_next_submission
                        .max(b.earliest_next_submission),
                    pending_submission: a.pending_submission.or(b.pending_submission),
                    version: cs_mail_primitives::RequestHistoryVersion(
                        a.version.0.max(b.version.0).saturating_add(1),
                    ),
                    last_event: None,
                    changed_at: now,
                }),
                merged: Some([a.reference, b.reference]),
                link: vec![mailbox_evidence, fingerprint_evidence],
            }
        }
    })
}

/// A guest's command, authorized by a verified mailbox's live session. Only a sender's
/// own request commands can be authorized; the guest writes as the mailbox's synthetic
/// identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GuestCommand {
    #[serde(skip)]
    session: GuestSession,
    identity: ProtocolIdentity,
    mailbox: GuestMailbox,
    command: ProtocolCommand,
    idempotency: IdempotencyKey,
}
impl GuestCommand {
    /// # Errors
    /// `SessionExpired` for a closed session and `NotAGuestCommand` for any command but
    /// issuing terms, creating, submitting or cancelling one's own request.
    pub fn authorize(
        session: &GuestSession,
        deployment_domain: &[u8; 32],
        command: ProtocolCommand,
        idempotency: IdempotencyKey,
        now: CanonicalTime,
    ) -> Result<Self, GuestError> {
        let mailbox = session.authorize(now)?.clone();
        if !matches!(
            command,
            ProtocolCommand::IssueRequestTerms { .. }
                | ProtocolCommand::CreateRequest { .. }
                | ProtocolCommand::SubmitRequestToRecipient { .. }
                | ProtocolCommand::CancelRequestSubmission {
                    reason: CancellationReason::SenderRequested,
                    ..
                }
        ) {
            return Err(GuestError::NotAGuestCommand);
        }
        Ok(Self {
            session: session.clone(),
            identity: mailbox.synthetic_protocol_identity(deployment_domain),
            mailbox,
            command,
            idempotency,
        })
    }
    /// Rechecks the session at durable receipt. Session evidence is excluded from
    /// replay bytes: renewed verification must not change the requested operation.
    /// # Errors
    /// Refuses expiry or a command presented in another deployment.
    pub fn authorize_receipt(
        &self,
        deployment: &[u8; 32],
        now: CanonicalTime,
    ) -> Result<(), GuestError> {
        if self
            .session
            .authorize(now)?
            .synthetic_protocol_identity(deployment)
            != self.identity
        {
            return Err(GuestError::NotAGuestCommand);
        }
        Ok(())
    }
    pub const fn identity(&self) -> ProtocolIdentity {
        self.identity
    }
    pub const fn mailbox(&self) -> &GuestMailbox {
        &self.mailbox
    }
    pub const fn command(&self) -> &ProtocolCommand {
        &self.command
    }
    pub const fn idempotency(&self) -> IdempotencyKey {
        self.idempotency
    }
}
