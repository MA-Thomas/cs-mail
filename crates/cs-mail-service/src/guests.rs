//! The guest request page's service: mailbox verification, the guest's request to a
//! member, its submission and cancellation, the guest's view, and outcome notices.
//!
//! Guest content is not end-to-end encrypted: the guest's message arrives in plaintext,
//! is encrypted to the member's content key at once, and only ciphertext is stored. The
//! plaintext is never logged, stored or put into a notice.
use crate::external::Handoff;
use crate::members::MemberGateway;
use crate::{CanonicalClock, ServiceError};
use cs_mail_application::guests::{GuestApproach, plan_guest_relationship};
use cs_mail_application::members::{DERIVATION_VERSION, RelationshipAddress, RelationshipView};
use cs_mail_application::notices::{NoticePurpose, NoticeRecord};
use cs_mail_application::receipts::ReceivedOutcome;
use cs_mail_content::{
    ContentBinding, ContentKeyCertificate, EndpointSecretKey, content_key_certificate_digest,
    message_declaration_digest,
};
use cs_mail_guests::{
    GuestCommand, GuestError, GuestMailbox, GuestNotice, GuestOutcome, GuestSession,
    InstrumentFingerprint, SessionTokenDigest, notices_for,
};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef, DeclarationAuthority,
    DeclaredPurpose, DeliveryIntentRef, Duration, IdempotencyKey, KnownPurpose,
    MessageDeclarations, MessageId, MessageValidityUntil, OriginDeclaration, OriginMode,
    ProtocolIdentity, QuoteId, RequestClassId, RequestId, WireVersion,
};
use cs_mail_protocol::{CancellationReason, ProtocolCommand, RequestLifecycle, TermsOutcome};
use cs_mail_smtp_gateway::outbound::OutboundMail;
use cs_mail_storage_postgres::StorageError;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::sync::Arc;

/// Largest guest message accepted, in bytes of UTF-8.
pub const MAX_GUEST_MESSAGE: usize = 16 * 1024;
/// How long a guest's content and message validity last.
const GUEST_CONTENT_LIFETIME: Duration = Duration(30 * 24 * 60 * 60 * 1000);
/// The provider's guest content key reference.
const GUEST_CONTENT_KEY: ContentKeyRef = ContentKeyRef(0x6775_6573_7400_0001);

/// Where notices go: an email delivery service, or a development outbox.
pub trait Outbox: Send + Sync {
    fn domain(&self) -> &str;
    /// Called only after the attempt commits. An uncertain result is never retried.
    fn deliver(&self, mail: &OutboundMail) -> Handoff;
}
/// The payment processor's tokenization of a guest's card or account.
pub trait InstrumentTokens: Send + Sync {
    /// The processor's token for this holder's instrument and its stable fingerprint.
    /// # Errors
    /// Returns an error for an instrument the processor declines.
    fn tokenize(
        &self,
        instrument: &str,
        holder: &GuestMailbox,
    ) -> Result<([u8; 32], InstrumentFingerprint), String>;
}

/// What a guest sees before writing: the recipient's classes and `C`, the request
/// windows, and whether the recipient can receive guests.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GuestOffer {
    pub recipient: ProtocolIdentity,
    pub offer: cs_mail_protocol::pricing::SenderOffer,
    pub submission_window: Duration,
    pub decision_window: Duration,
    pub ready: bool,
}
/// The answer to a guest's request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GuestRequestAnswer {
    pub request: RequestId,
    pub terms: cs_mail_protocol::RequestTerms,
}

/// The guest request page's service over the member gateway.
pub struct GuestGateway<C> {
    pub(crate) members: Arc<MemberGateway<C>>,
    pub(crate) content_seed: [u8; 32],
    outbox: Arc<dyn Outbox>,
    tokens: Arc<dyn InstrumentTokens>,
    request_site: cs_mail_smtp_gateway::outbound::RequestSite,
}

fn random<const N: usize>() -> Result<[u8; N], ServiceError> {
    let mut bytes = [0; N];
    getrandom::getrandom(&mut bytes).map_err(|_| ServiceError::Outbox)?;
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, b| {
        use core::fmt::Write as _;
        let _ = write!(text, "{b:02x}");
        text
    })
}
fn token_digest(token: &str) -> SessionTokenDigest {
    SessionTokenDigest::new(
        Sha256::new()
            .chain_update(b"cs-mail/guest-session/v1")
            .chain_update(token.as_bytes())
            .finalize()
            .into(),
    )
}
fn verification_salt(id: &str) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"cs-mail/guest-verification-salt/v1")
        .chain_update(id.as_bytes())
        .finalize()
        .into()
}
/// A stable identifier for one step of one guest operation, so retries replay.
fn step(operation: u128, name: &str) -> u128 {
    let digest = Sha256::new()
        .chain_update(b"cs-mail/guest-operation/v1")
        .chain_update(operation.to_be_bytes())
        .chain_update(name.as_bytes())
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    u128::from_be_bytes(bytes) | 1
}
pub(crate) fn rfc5322_date(now: CanonicalTime) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let seconds = now.0 / 1000;
    let days = seconds / 86_400;
    let (h, m, s) = (seconds % 86_400 / 3600, seconds % 3600 / 60, seconds % 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{}, {day:02} {} {year} {h:02}:{m:02}:{s:02} +0000",
        DAYS[usize::try_from(days % 7).unwrap_or(0)],
        MONTHS[usize::try_from(month - 1).unwrap_or(0)],
    )
}
fn declarations(provider: cs_mail_primitives::ProviderRef) -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::GuestGateway(provider),
        },
        payload_schema: None,
    }
}

impl<C: CanonicalClock + Clone + 'static> GuestGateway<C> {
    pub fn new(
        members: Arc<MemberGateway<C>>,
        content_seed: [u8; 32],
        outbox: Arc<dyn Outbox>,
        tokens: Arc<dyn InstrumentTokens>,
        request_site: cs_mail_smtp_gateway::outbound::RequestSite,
    ) -> Self {
        Self {
            members,
            content_seed,
            outbox,
            tokens,
            request_site,
        }
    }
    fn now(&self) -> CanonicalTime {
        self.members.clock().now()
    }
    fn domain(&self) -> [u8; 32] {
        self.members.record().record().deployment_domain
    }
    /// Sends a verification code to `mailbox` and returns the verification's identifier.
    /// # Errors
    /// `InvalidMailbox`, the rate limit, storage and outbox failures.
    pub fn begin_verification(&self, mailbox: &str) -> Result<String, ServiceError> {
        let mailbox = GuestMailbox::parse(mailbox).map_err(StorageError::from)?;
        let id = hex(&random::<16>()?);
        let code = format!("{:06}", u32::from_be_bytes(random::<4>()?) % 1_000_000);
        let notice = self.members.deployment().begin_guest_verification(
            &self.domain(),
            &id,
            &mailbox,
            &code,
            &verification_salt(&id),
            || self.now(),
        )?;
        self.deliver_notice(
            &notice,
            &OutboundMail {
                thread: None,
                from_local: "notices".into(),
                to: mailbox.address(),
                subject: "Your cs-mail verification code".into(),
                body: format!(
                    "Your verification code is {code}.\n\nIt is valid for 15 minutes. \
                     If you did not ask for it, you can ignore this message."
                ),
                message_id: notice_message_id(notice.id),
                date: rfc5322_date(notice.created_at),
            },
        )?;
        Ok(id)
    }
    /// Checks a code and returns a session token for the verified mailbox.
    /// # Errors
    /// Wrong or expired codes, too many attempts, storage failures.
    pub fn confirm_verification(&self, id: &str, code: &str) -> Result<String, ServiceError> {
        let token = hex(&random::<32>()?);
        self.members.deployment().confirm_guest_verification(
            id,
            code,
            &verification_salt(id),
            token_digest(&token),
            || self.now(),
        )?;
        Ok(token)
    }
    fn session(&self, token: &str) -> Result<GuestSession, ServiceError> {
        Ok(self
            .members
            .deployment()
            .guest_session(token_digest(token), || self.now())?)
    }
    /// The guest's own address and mailbox for a session.
    /// # Errors
    /// An unknown or expired session.
    pub fn whoami(&self, token: &str) -> Result<(GuestMailbox, ProtocolIdentity), ServiceError> {
        let session = self.session(token)?;
        let identity = session
            .mailbox()
            .synthetic_protocol_identity(&self.domain());
        Ok((session.mailbox().clone(), identity))
    }
    /// What a guest may choose when approaching `recipient`.
    /// # Errors
    /// An unknown recipient or a missing offer.
    pub fn offer(&self, recipient: ProtocolIdentity) -> Result<GuestOffer, ServiceError> {
        let policy = self.members.policy()?;
        let offer = self
            .members
            .offer(recipient)?
            .ok_or(ServiceError::RecipientNotReady)?;
        Ok(GuestOffer {
            recipient,
            offer,
            submission_window: policy.submission_window,
            decision_window: policy.decision_window,
            ready: self.members.endpoint_key(recipient)?.is_some(),
        })
    }
    fn address(
        &self,
        guest: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> Result<RelationshipAddress, ServiceError> {
        self.members
            .deployment()
            .accounts({
                let clock = self.members.clock().clone();
                move || clock.now()
            })
            .persona_account(recipient)?;
        Ok(
            RelationshipAddress::derive(&self.members.deriver(), &self.domain(), guest, recipient)
                .map_err(StorageError::from)?,
        )
    }
    fn command(
        &self,
        session: &GuestSession,
        command: ProtocolCommand,
        key: u128,
    ) -> Result<GuestCommand, ServiceError> {
        Ok(GuestCommand::authorize(
            session,
            &self.domain(),
            command,
            IdempotencyKey(key),
            self.now(),
        )
        .map_err(StorageError::from)?)
    }
    fn accepted(outcome: ReceivedOutcome) -> Result<ReceivedOutcome, ServiceError> {
        if let ReceivedOutcome::Refused(refusal) = outcome {
            return Err(cs_mail_storage_postgres::StorageError::from(refusal).into());
        }
        Ok(outcome)
    }
    /// A guest's request: terms for the class, one conditional charge on the guest's
    /// instrument, and the message, encrypted to the recipient on arrival. Retrying with
    /// the same `operation` replays every step.
    /// # Errors
    /// Session, recipient, payment-method, history and protocol refusals.
    #[allow(clippy::too_many_lines)] // Terms, relationship, charge and content form one request.
    pub fn request(
        &self,
        token: &str,
        recipient: ProtocolIdentity,
        class: RequestClassId,
        instrument: &str,
        message: &str,
        operation: u128,
    ) -> Result<GuestRequestAnswer, ServiceError> {
        if message.is_empty() || message.len() > MAX_GUEST_MESSAGE {
            return Err(ServiceError::ContentTimeInvalid);
        }
        let session = self.session(token)?;
        let now = self.now();
        let domain = self.domain();
        let identity = session.mailbox().synthetic_protocol_identity(&domain);
        let recipient_key = self
            .members
            .endpoint_key(recipient)?
            .ok_or(ServiceError::RecipientNotReady)?;
        let address = self.address(identity, recipient)?;
        let (payment_method, fingerprint) = self
            .tokens
            .tokenize(instrument, session.mailbox())
            .map_err(|_| ServiceError::PaymentMethod)?;
        let input = cs_mail_guests::GuestRequestInput {
            mailbox: session.mailbox(),
            recipient,
            class,
            payment_token: &payment_method,
            message,
        };
        self.members.deployment().bind_guest_request(
            &session,
            &domain,
            IdempotencyKey(operation),
            input.fingerprint(&self.content_seed, &domain, IdempotencyKey(operation)),
            || self.now(),
        )?;
        let deriver = self.members.deriver();
        let approach = GuestApproach {
            session: &session,
            identity,
            fingerprint,
            token: payment_method,
            address: &address,
        };
        let record = self.members.record().record();
        let engine = self.members.deployment().open_guest_relationship(
            &approach,
            record.financial_scope,
            record.unit,
            || self.now(),
            |context| {
                Ok(plan_guest_relationship(
                    &approach,
                    &context,
                    deriver.guest_request_history_ref(
                        DERIVATION_VERSION,
                        &domain,
                        identity,
                        recipient,
                    ),
                    |a, b| deriver.merged_request_history_ref(DERIVATION_VERSION, &domain, a, b),
                )?)
            },
        )?;
        let ingress = self.members.ingress(engine)?;
        let provider = record.provider;
        let digest = message_declaration_digest(&declarations(provider))
            .map_err(|_| ServiceError::ContentTimeInvalid)?;

        let issued = Self::accepted(ingress.submit_guest(&self.command(
            &session,
            ProtocolCommand::IssueRequestTerms {
                class_id: class,
                quote_id: QuoteId(step(operation, "quote")),
                declaration_digest: Some(digest),
            },
            step(operation, "issue"),
        )?)?)?;
        let terms = match issued {
            ReceivedOutcome::Protocol(outcome) => match outcome.transition.terms_outcome {
                Some(TermsOutcome::ChargeRequired(terms)) => terms,
                // This mailbox already has an undecided request to the recipient.
                Some(TermsOutcome::ExistingRequest(_)) => {
                    return Err(StorageError::from(GuestError::PendingRequest).into());
                }
                _ => {
                    return Err(StorageError::from(
                        cs_mail_protocol::ProtocolError::RequestAlreadyExists,
                    )
                    .into());
                }
            },
            _ => return Err(ServiceError::ReceiptUnavailable),
        };
        let request = RequestId(step(operation, "request"));
        let message_id = MessageId(step(operation, "message"));
        Self::accepted(ingress.submit_guest(&self.command(
            &session,
            ProtocolCommand::CreateRequest {
                payment_method,
                request_id: request,
                message_id,
                terms: terms.clone(),
            },
            step(operation, "create"),
        )?)?)?;

        // The message is encrypted to the recipient at once; only ciphertext is kept.
        let content_ref = ContentRef(step(operation, "content"));
        if !ingress.engine().has_content(content_ref)? {
            let (secret, public) =
                EndpointSecretKey::from_secret_bytes(GUEST_CONTENT_KEY, &self.content_seed)
                    .map_err(|_| ServiceError::SigningScopeMismatch)?;
            let valid_until = MessageValidityUntil(CanonicalTime(
                now.0.saturating_add(GUEST_CONTENT_LIFETIME.0),
            ));
            let certificate =
                self.members
                    .signer()
                    .sign_guest_content_certificate(ContentKeyCertificate {
                        wire_version: WireVersion(1),
                        protocol_version: terms.protocol_version,
                        deployment_domain: domain,
                        intended_provider: provider,
                        relationship: address.relationship(),
                        owner: identity,
                        operational_key: record.provider_key.reference,
                        content_key_version: ContentKeyVersion(1),
                        key: public,
                        valid_from: now,
                        valid_until: valid_until.0,
                    })?;
            let content = cs_mail_content::encrypt(
                &secret,
                recipient_key.key,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref,
                    message_id,
                    sender: identity,
                    recipient,
                    protocol_version: terms.protocol_version,
                    relationship: address.relationship(),
                    content_scope: address.content_scope(),
                    sender_certificate: content_key_certificate_digest(&certificate.certificate),
                    declarations: declarations(provider),
                    message_valid_until: valid_until,
                    capability: None,
                },
                message.as_bytes(),
                now,
                valid_until.0,
            )
            .map_err(|_| ServiceError::ContentTimeInvalid)?;
            ingress.store_guest_content(&session, &content, &certificate)?;
        }
        session.authorize(self.now()).map_err(StorageError::from)?;
        Ok(GuestRequestAnswer {
            request,
            terms: *terms,
        })
    }
    /// Submits the guest's funded request to the recipient, starting the decision window.
    /// # Errors
    /// `NotFunded` until the charge's funding is final; protocol refusals.
    pub fn submit(
        &self,
        token: &str,
        recipient: ProtocolIdentity,
        operation: u128,
    ) -> Result<RelationshipView, ServiceError> {
        let session = self.session(token)?;
        let identity = session
            .mailbox()
            .synthetic_protocol_identity(&self.domain());
        let address = self.address(identity, recipient)?;
        let view = self
            .members
            .view(&address)?
            .ok_or(ServiceError::NoRequest)?;
        let id = RequestId(step(operation, "request"));
        let request = view
            .requests
            .iter()
            .find(|r| r.id == id)
            .ok_or(ServiceError::NoRequest)?;
        if request.money.map(|m| m.capture)
            != Some(cs_mail_finance::CaptureStatus::FundingFinalized)
        {
            return Err(ServiceError::NotFunded);
        }
        let ingress = self
            .members
            .existing(address.aggregate_key())?
            .ok_or(ServiceError::NoRequest)?;
        let content_ref = ContentRef(step(operation, "content"));
        let content = ingress
            .engine()
            .content(content_ref)?
            .ok_or(ServiceError::NoRequest)?;
        Self::accepted(
            ingress.submit_guest(
                &self.command(
                    &session,
                    ProtocolCommand::SubmitRequestToRecipient {
                        request_id: id,
                        expected_request_version: request.version.into(),
                        content_ref,
                        delivery_intent_ref: DeliveryIntentRef(step(operation, "delivery")),
                        declaration_digest: message_declaration_digest(
                            &content.binding.declarations,
                        )
                        .map_err(|_| ServiceError::ContentTimeInvalid)?,
                        message_valid_until: content.binding.message_valid_until,
                    },
                    step(operation, "submit"),
                )?,
            )?,
        )?;
        self.status(token, recipient)?
            .ok_or(ServiceError::NoRequest)
    }
    /// Cancels the guest's request before submission; the full charge is refunded.
    /// # Errors
    /// No unresolved request; protocol refusals.
    pub fn cancel(
        &self,
        token: &str,
        recipient: ProtocolIdentity,
        operation: u128,
    ) -> Result<RelationshipView, ServiceError> {
        let session = self.session(token)?;
        let identity = session
            .mailbox()
            .synthetic_protocol_identity(&self.domain());
        let address = self.address(identity, recipient)?;
        let view = self
            .members
            .view(&address)?
            .ok_or(ServiceError::NoRequest)?;
        let id = RequestId(step(operation, "request"));
        let request = view
            .requests
            .iter()
            .find(|r| r.id == id)
            .ok_or(ServiceError::NoRequest)?;
        let ingress = self
            .members
            .existing(address.aggregate_key())?
            .ok_or(ServiceError::NoRequest)?;
        Self::accepted(ingress.submit_guest(&self.command(
            &session,
            ProtocolCommand::CancelRequestSubmission {
                request_id: id,
                expected_request_version: request.version.into(),
                reason: CancellationReason::SenderRequested,
            },
            step(operation, "cancel"),
        )?)?)?;
        self.status(token, recipient)?
            .ok_or(ServiceError::NoRequest)
    }
    /// The guest's own relationship with `recipient`.
    /// # Errors
    /// An unknown or expired session; storage failures.
    pub fn status(
        &self,
        token: &str,
        recipient: ProtocolIdentity,
    ) -> Result<Option<RelationshipView>, ServiceError> {
        let session = self.session(token)?;
        let identity = session
            .mailbox()
            .synthetic_protocol_identity(&self.domain());
        let view = self.members.view(&self.address(identity, recipient)?)?;
        session.authorize(self.now()).map_err(StorageError::from)?;
        Ok(view)
    }
    /// Returns the number of attempts begun, not recipient deliveries.
    /// Attempts each owed outcome once. The attempt commits before carrier I/O;
    /// interruptions remain explicitly uncertain and never cause an automatic resend.
    /// # Errors
    /// Storage failures or a definite carrier refusal. Unknown attempts need reconciliation.
    pub fn send_notices(&self) -> Result<usize, ServiceError> {
        let deployment = self.members.deployment();
        let mut sent = 0;
        for pending in deployment.waiting_notices(100)? {
            let Some(notice) = deployment.begin_queued_notice(pending.id, || self.now())? else {
                continue;
            };
            let NoticePurpose::RequestLink { member, .. } = notice.purpose else {
                return Err(ServiceError::Outbox);
            };
            let mail = OutboundMail {
                thread: None,
                from_local: "notices".into(),
                to: notice.recipient.address(),
                subject: "Request permission to contact this cs-mail member".into(),
                body: format!(
                    "Your email was not delivered to the member. You can request permission here:\n{}\n\nMailbox verification and the displayed request terms apply.",
                    self.request_site.member(member)
                ),
                message_id: notice_message_id(notice.id),
                date: rfc5322_date(notice.created_at),
            };
            self.deliver_notice(&notice, &mail)?;
            sent += 1;
        }
        for guest in deployment.guest_relationships()? {
            let Some(engine) = deployment.existing_relationship(&guest.aggregate_key)? else {
                continue;
            };
            let snapshot = engine.snapshot()?;
            for request in snapshot.state.requests.values() {
                let money = snapshot
                    .payments
                    .get(&request.id)
                    .map(cs_mail_finance::RequestFinancials::view);
                for notice in notices_for(request.id, &request.lifecycle, money.as_ref()) {
                    let now = self.now();
                    let intent = NoticeRecord::new(
                        &self.domain(),
                        guest.mailbox.clone(),
                        NoticePurpose::Outcome {
                            relationship: guest.aggregate_key.clone(),
                            member: guest.recipient,
                            notice,
                        },
                        now,
                    )
                    .map_err(StorageError::from)?;
                    if !deployment.reserve_notice(&intent, || self.now())? {
                        continue;
                    }
                    let mut mail = notice_mail(
                        intent.id,
                        &guest.mailbox,
                        guest.recipient,
                        &notice,
                        &request.lifecycle,
                        now,
                    );
                    if let Some(conversation) =
                        cs_mail_application::notices::paid_conversation(&intent)
                    {
                        mail.thread = Some(cs_mail_smtp_gateway::outbound::MailThread {
                            reply_to: format!(
                                "reply+{:032x}@{}",
                                conversation.value(),
                                self.outbox.domain()
                            ),
                            root: None,
                        });
                        mail.body.push_str("\n\nYou may reply to this email to continue the conversation, subject to the recipient's current permission.");
                    }
                    self.deliver_notice(&intent, &mail)?;
                    sent += 1;
                }
            }
        }
        Ok(sent)
    }
    fn deliver_notice(
        &self,
        notice: &NoticeRecord,
        mail: &OutboundMail,
    ) -> Result<(), ServiceError> {
        match self.outbox.deliver(mail) {
            Handoff::Accepted(reference) => {
                self.members
                    .deployment()
                    .finish_notice(notice.id, Some(reference), self.now())?;
            }
            Handoff::Failed => {
                self.members
                    .deployment()
                    .finish_notice(notice.id, None, self.now())?;
                return Err(ServiceError::Outbox);
            }
            Handoff::Unknown => {}
        }
        Ok(())
    }
}

pub fn notice_message_id(id: cs_mail_guests::NoticeId) -> String {
    format!("notice-{:032x}", id.value())
}

/// Transactional notices use the same carrier and uncertainty semantics as authored mail.
pub struct Smtp2goOutbox {
    pub carrier: cs_mail_smtp_gateway::smtp2go::Smtp2go,
    pub dkim: cs_mail_smtp_gateway::outbound::DkimIdentity,
}
impl Outbox for Smtp2goOutbox {
    fn domain(&self) -> &str {
        self.dkim.domain()
    }
    fn deliver(&self, mail: &OutboundMail) -> Handoff {
        use cs_mail_smtp_gateway::{outbound::render_signed, smtp2go::Submission};
        let Ok(mime) = render_signed(mail, &self.dkim) else {
            return Handoff::Failed;
        };
        match self.carrier.submit(&mime) {
            Submission::Accepted(reference) => Handoff::Accepted(reference),
            Submission::Rejected => Handoff::Failed,
            Submission::Unknown => Handoff::Unknown,
        }
    }
}

fn notice_mail(
    id: cs_mail_guests::NoticeId,
    mailbox: &GuestMailbox,
    recipient: ProtocolIdentity,
    notice: &GuestNotice,
    lifecycle: &RequestLifecycle,
    now: CanonicalTime,
) -> OutboundMail {
    let money = |amount: cs_mail_primitives::Money| {
        let units = amount.minor_units();
        format!("{}.{:02}", units / 100, units % 100)
    };
    let (subject, body) = match notice.outcome {
        GuestOutcome::Submitted => (
            "Your request was delivered".to_owned(),
            match lifecycle.submission() {
                Some(submission) => format!(
                    "Your request to {} was delivered. The recipient decides by {} (Unix ms).",
                    recipient.0, submission.decision_deadline.0
                ),
                None => format!("Your request to {} was delivered.", recipient.0),
            },
        ),
        GuestOutcome::Accepted => (
            "Your request was accepted".to_owned(),
            "The recipient accepted your request. Your full charge will be refunded to the \
             payment method you used."
                .to_owned(),
        ),
        GuestOutcome::Rejected => (
            "Your request was declined".to_owned(),
            "The recipient declined your request. The processing charge is retained and the \
             collateral goes to the cs-mail member program."
                .to_owned(),
        ),
        GuestOutcome::Expired => (
            "Your request expired".to_owned(),
            "The recipient did not decide in time. The processing charge is retained and \
             your collateral will be refunded to the payment method you used."
                .to_owned(),
        ),
        GuestOutcome::Cancelled => (
            "Your request was cancelled".to_owned(),
            "Your request was cancelled before delivery. Your full charge will be refunded to \
             the payment method you used."
                .to_owned(),
        ),
        GuestOutcome::RefundConfirmed(amount) => (
            "Your refund was paid".to_owned(),
            format!(
                "A refund of {} was paid to the payment method you used.",
                money(amount)
            ),
        ),
    };
    OutboundMail {
        thread: None,
        from_local: "notices".into(),
        to: mailbox.address(),
        subject,
        body,
        message_id: notice_message_id(id),
        date: rfc5322_date(now),
    }
}
