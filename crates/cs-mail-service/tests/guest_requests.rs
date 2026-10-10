//! Milestone 3b claims about guest senders, challenged through the guest gateway against
//! `PostgreSQL`:
//!
//! 1. Changing mailbox while reusing a card cannot reset request history or bypass a
//!    pending request. Falsified by a new mailbox with a known card preparing a request
//!    while the history's reservation is held, escaping its backoff, or asking at a lower
//!    level or in a fresh history. The request page would become a harassment channel.
//! 2. No two distinct sender subjects share a protocol identity. Falsified by a guest
//!    admitted under a member's identity, or a member enrolled under a guest's. Commands
//!    and history would otherwise be attributed to the wrong party.
//! 3. A guest's message text never reaches the database in plaintext. Falsified by the
//!    text, in any encoding, in any table. The page promises encryption on arrival.
//! 4. A guest refund goes only to the token that funded the request. Falsified by a refund
//!    to any other destination after acceptance, cancellation or expiry. Refunds are the
//!    guest's only protection.
use cs_mail_test_support as support;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cs_mail_finance::{
    BankVerification, FinancialScope, PaymentError, PaymentKind, PaymentOperation,
    PaymentProcessor, SignedPaymentEvidence, SimulatedProcessor,
};
use cs_mail_guests::{GuestError, GuestMailbox, InstrumentFingerprint, SenderSubject};
use cs_mail_primitives::{
    BillingAccountId, CanonicalTime, ContentKeyRef, Duration, IdempotencyKey, MemberId, Money,
    OperationalKeyRef, PolicyVersion, PrivacyProfileVersion, ProgramRef, ProtocolIdentity,
    ProtocolVersion, ProviderRef, RequestClassId, RequestId, RetentionPolicyVersion,
    SettlementUnit,
};
use cs_mail_protocol::policy::{ProtocolPolicy, ProtocolPolicyRecord};
use cs_mail_protocol::{
    ActorRef, ProtocolCommand, ProtocolError, RequestLifecycle, SettlementSnapshot,
};
use cs_mail_security::{CommandSigner, EndpointKeyPublication, SignedEndpointKey, SigningScope};
use cs_mail_service::guests::{GuestGateway, InstrumentTokens, Outbox};
use cs_mail_service::members::{GatewaySecrets, MemberGateway};
use cs_mail_service::{CanonicalClock, ServiceError};
use cs_mail_smtp_gateway::outbound::OutboundMail;
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};

const UNIT: SettlementUnit = SettlementUnit(1);
const DOMAIN: [u8; 32] = [7; 32];
const ALICE: ProtocolIdentity = ProtocolIdentity(10);
const BOB: ProtocolIdentity = ProtocolIdentity(20);
const ALICE_KEY: OperationalKeyRef = OperationalKeyRef(11);
const BOB_KEY: OperationalKeyRef = OperationalKeyRef(21);
const BOB_CONTENT: [u8; 32] = [22; 32];
const DECISION_WINDOW: u64 = 60_000;
static NEXT_SCHEMA: AtomicU64 = AtomicU64::new(1);

fn scope() -> FinancialScope {
    FinancialScope::new(
        DOMAIN,
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn database_url() -> (String, String) {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_guest_{}_{}_{}",
        std::process::id(),
        NEXT_SCHEMA.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut client = postgres::Client::connect(&base, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    (format!("{base}?options=-csearch_path%3D{schema}"), schema)
}

#[derive(Clone)]
struct Clock(Arc<AtomicU64>);
impl CanonicalClock for Clock {
    fn now(&self) -> CanonicalTime {
        CanonicalTime(self.0.load(Ordering::SeqCst))
    }
}

/// Keeps every notice, so a test can read verification codes.
#[derive(Default)]
struct Mailbag(Mutex<Vec<OutboundMail>>);
impl Outbox for Mailbag {
    fn domain(&self) -> &'static str {
        "example.test"
    }
    fn deliver(&self, mail: &OutboundMail) -> cs_mail_service::external::Handoff {
        self.0.lock().unwrap().push(mail.clone());
        cs_mail_service::external::Handoff::Accepted(
            cs_mail_correspondence::external::delivery::ProviderReference::new(format!(
                "test:{}",
                mail.message_id
            ))
            .unwrap(),
        )
    }
}
struct Cards(cs_mail_simulation::SimulatedCards);
impl InstrumentTokens for Cards {
    fn tokenize(
        &self,
        instrument: &str,
        holder: &GuestMailbox,
    ) -> Result<([u8; 32], InstrumentFingerprint), String> {
        self.0
            .tokenize(instrument, &holder.address())
            .map(|(token, fingerprint)| (token, InstrumentFingerprint(fingerprint)))
            .map_err(|_| "declined".into())
    }
}
fn cards() -> Cards {
    Cards(cs_mail_simulation::SimulatedCards::new([8; 32]))
}

/// The simulator, recording every operation it executes.
struct RecordingProcessor {
    inner: SimulatedProcessor,
    executed: Vec<PaymentOperation>,
}
impl PaymentProcessor for RecordingProcessor {
    fn lookup(
        &mut self,
        id: cs_mail_primitives::PaymentOperationId,
    ) -> Result<Option<SignedPaymentEvidence>, PaymentError> {
        self.inner.lookup(id)
    }
    fn submit(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        self.executed.push(operation.clone());
        self.inner.submit(operation)
    }
    fn cancel_capture(
        &mut self,
        request: &cs_mail_finance::CaptureCancellation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        self.inner.cancel_capture(request)
    }
}

fn policy() -> ProtocolPolicy {
    ProtocolPolicy::new(ProtocolPolicyRecord {
        version: PolicyVersion(1),
        privacy_profile_version: PrivacyProfileVersion(1),
        retention_policy_version: RetentionPolicyVersion(1),
        submission_window: Duration(600_000),
        decision_window: Duration(DECISION_WINDOW),
        quote_lifetime: Duration(600_000),
        backoff: vec![Duration(0), Duration(1_000)],
        expiry_cooldown: Duration(1_000),
        rejection_cooldown: Duration(1_000),
        corporate_basis_points: 300,
        maturity_delay: Duration(1_000),
    })
    .unwrap()
}
fn enroll(
    deployment: &PostgresDeployment,
    n: u8,
    persona: ProtocolIdentity,
    actor: ActorRef,
    key: OperationalKeyRef,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    let bank = BankVerification {
        scope: scope(),
        account: BillingAccountId(u128::from(n)),
        member: MemberId(u128::from(n)),
        bank_token: [n + 40; 32],
        unit: UNIT,
        version: 1,
        signature: vec![],
    }
    .sign(&[77; 32])
    .unwrap();
    let secret = u8::try_from(key.0).unwrap();
    support::enrollment::enroll(
        deployment,
        cs_mail_accounts::EnrollmentInput {
            bank,
            persona,
            actor,
            key_ref: key,
            initial_key: CommandSigner::from_secret_bytes(actor, key, &[secret; 32])
                .verifying_key_bytes(),
            maximum_unresolved: 10,
        },
        at,
    )
    .map(|_| ())
}

struct Fixture {
    url: String,
    schema: String,
    clock: Clock,
    members: Arc<MemberGateway<Clock>>,
    guests: GuestGateway<Clock>,
    mail: Arc<Mailbag>,
    processor: RecordingProcessor,
    bob: CommandSigner,
    next: u128,
}

impl Fixture {
    /// A bootstrapped deployment with published policy, Alice and Bob enrolled and covered,
    /// and Bob receiving: one request class (`C = 2`, `S = 8`) and an endpoint key.
    #[allow(clippy::too_many_lines)] // Complete enrolled and covered guest-request fixture.
    fn new() -> Self {
        let (url, schema) = database_url();
        // Inside the fixtures' 1971 service year.
        let clock = Clock(Arc::new(AtomicU64::new(44_582_400_000)));
        let at = clock.now();
        let deployment = PostgresDeployment::connect(&url).unwrap();
        support::bootstrap(&deployment, scope(), UNIT).unwrap();
        for command in [
            cs_mail_operations::OperatorCommand::PublishRequestPricing(
                support::fixture_pricing(UNIT).unwrap(),
            ),
            cs_mail_operations::OperatorCommand::PublishProtocolPolicy(policy()),
        ] {
            support::operator(&deployment, command, at).unwrap();
        }
        enroll(
            &deployment,
            1,
            ALICE,
            ActorRef::Sender(ALICE),
            ALICE_KEY,
            at,
        )
        .unwrap();
        enroll(&deployment, 2, BOB, ActorRef::Recipient(BOB), BOB_KEY, at).unwrap();
        for (n, key) in [(1, ALICE_KEY), (2, BOB_KEY)] {
            support::purchase_service(&deployment, scope(), UNIT, BillingAccountId(n), key, at)
                .unwrap();
        }
        let record = deployment.deployment_record().unwrap().unwrap();
        let members = Arc::new(
            MemberGateway::new(
                deployment,
                record,
                GatewaySecrets {
                    provider_seed: [3; 32],
                    relationship_secret: [5; 32],
                    principal_secret: [6; 32],
                },
                clock.clone(),
            )
            .unwrap(),
        );
        let bob = CommandSigner::from_secret_bytes(ActorRef::Recipient(BOB), BOB_KEY, &[21; 32]);
        members
            .publish_classes(
                &bob.sign_request_classes(
                    members.scope(),
                    cs_mail_protocol::pricing::RecipientRequestClasses::new(
                        BOB,
                        1,
                        vec![
                            cs_mail_protocol::pricing::RequestClass::new(
                                RequestClassId(1),
                                "Introductions".into(),
                                Money::from_minor_units(support::FIXTURE_COLLATERAL),
                            )
                            .unwrap(),
                        ],
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let (_, content) =
            cs_mail_content::EndpointSecretKey::from_secret_bytes(ContentKeyRef(22), &BOB_CONTENT)
                .unwrap();
        members
            .publish_endpoint_key(
                &SignedEndpointKey::sign(
                    members.scope(),
                    BOB_KEY,
                    EndpointKeyPublication {
                        persona: BOB,
                        version: 1,
                        key: content,
                    },
                    &[21; 32],
                )
                .unwrap(),
            )
            .unwrap();
        let mail = Arc::new(Mailbag::default());
        let guests = GuestGateway::new(
            members.clone(),
            [4; 32],
            mail.clone(),
            Arc::new(cards()),
            cs_mail_smtp_gateway::outbound::RequestSite::new("https://requests.example.test")
                .unwrap(),
        );
        Self {
            url,
            schema,
            clock,
            members,
            guests,
            mail,
            processor: RecordingProcessor {
                inner: SimulatedProcessor::new([7; 32]),
                executed: Vec::new(),
            },
            bob,
            next: 1,
        }
    }
    fn advance(&self, millis: u64) {
        self.clock.0.fetch_add(millis, Ordering::SeqCst);
    }
    fn work(&mut self) {
        for _ in 0..3 {
            self.advance(1);
            self.members
                .run_relationship_work(&mut self.processor, Duration(30_000), 50)
                .unwrap();
        }
    }
    /// A verified session for `mailbox`, using the code from the outbox.
    fn login(&self, mailbox: &str) -> String {
        let id = self.guests.begin_verification(mailbox).unwrap();
        let body = self.mail.0.lock().unwrap().last().unwrap().body.clone();
        let code = body
            .split_whitespace()
            .find(|word| word.len() == 7 && word.ends_with('.'))
            .unwrap()
            .trim_end_matches('.')
            .to_owned();
        self.guests.confirm_verification(&id, &code).unwrap()
    }
    fn operation(&mut self) -> u128 {
        self.next += 1;
        self.next
    }
    /// A funded, prepared request; returns its operation.
    fn prepare(
        &mut self,
        session: &str,
        card: &str,
        message: &str,
    ) -> Result<(u128, cs_mail_service::guests::GuestRequestAnswer), ServiceError> {
        let operation = self.operation();
        self.advance(1);
        let answer =
            self.guests
                .request(session, BOB, RequestClassId(1), card, message, operation)?;
        self.work();
        Ok((operation, answer))
    }
    /// A request submitted to Bob.
    fn send(&mut self, session: &str, card: &str, message: &str) -> (u128, RequestId) {
        let (operation, answer) = self.prepare(session, card, message).unwrap();
        self.advance(1);
        self.guests.submit(session, BOB, operation).unwrap();
        (operation, answer.request)
    }
    fn identity(mailbox: &str) -> ProtocolIdentity {
        GuestMailbox::parse(mailbox)
            .unwrap()
            .synthetic_protocol_identity(&DOMAIN)
    }
    fn snapshot(&self, mailbox: &str) -> SettlementSnapshot {
        let address = self.members.address(Self::identity(mailbox), BOB).unwrap();
        self.members
            .deployment()
            .existing_relationship(address.aggregate_key())
            .unwrap()
            .unwrap()
            .snapshot()
            .unwrap()
    }
    fn decide(&mut self, mailbox: &str, accept: bool) {
        let guest = Self::identity(mailbox);
        let version = self.snapshot(mailbox).state.relationship.version;
        let command = if accept {
            ProtocolCommand::AcceptRelationship {
                expected_version: version.into(),
            }
        } else {
            ProtocolCommand::RejectRelationship {
                expected_version: version.into(),
            }
        };
        let key = self.operation();
        let signed = self
            .bob
            .sign(
                SigningScope {
                    deployment_domain: DOMAIN,
                    intended_provider: ProviderRef(30),
                    relationship: self
                        .members
                        .address(guest, BOB)
                        .unwrap()
                        .public()
                        .relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(key),
                command,
            )
            .unwrap();
        self.advance(1);
        self.members.submit(guest, BOB, &signed).unwrap();
        self.work();
    }
    fn client(&self) -> postgres::Client {
        postgres::Client::connect(&self.url, postgres::NoTls).unwrap()
    }
}

fn guest_error(result: Result<impl std::fmt::Debug, ServiceError>) -> GuestError {
    match result {
        Err(ServiceError::Storage(StorageError::Guest(error))) => error,
        other => panic!("expected a guest refusal, got {other:?}"),
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn changing_mailbox_while_reusing_a_card_cannot_reset_history_or_bypass_a_pending_request() {
    let mut f = Fixture::new();
    let visa = "4242424242424242";

    // While Ana's request is prepared and funded, its history holds the reservation: the
    // same card under a new mailbox shares that history and cannot prepare another.
    let ana = f.login("ana@guest.example");
    let (op, first) = f.prepare(&ana, visa, "first").unwrap();
    let alias = f.login("ana.alias@guest.example");
    assert!(matches!(
        f.prepare(&alias, visa, "second"),
        Err(ServiceError::Storage(StorageError::Protocol(
            ProtocolError::RequestAlreadyExists
        )))
    ));

    // Submission moves the history up a level and starts its backoff, which binds the
    // alias too; Ana's own mailbox still holds its undecided request.
    f.advance(1);
    f.guests.submit(&ana, BOB, op).unwrap();
    assert!(matches!(
        f.prepare(&alias, visa, "second"),
        Err(ServiceError::Storage(StorageError::Protocol(
            ProtocolError::BackoffActive { .. }
        )))
    ));
    f.advance(5_000);
    assert_eq!(
        guest_error(f.prepare(&ana, visa, "again")),
        GuestError::PendingRequest
    );

    // After the backoff the alias may ask, but under Ana's history, above her level.
    let (_, second) = f.prepare(&alias, visa, "second").unwrap();
    assert_eq!(second.terms.request_history, first.terms.request_history);
    assert!(second.terms.request_level > first.terms.request_level);

    // Two separate resolved histories are merged when evidence links them: Cara's mailbox
    // history and the history of Dev's card become one, and both keep using it.
    let cara = f.login("cara@guest.example");
    let (op, cara_first) = f.prepare(&cara, "5555555555554444", "cara").unwrap();
    f.guests.cancel(&cara, BOB, op).unwrap();
    let dev = f.login("dev@guest.example");
    let (op, dev_first) = f.prepare(&dev, "6011111111111117", "dev").unwrap();
    f.guests.cancel(&dev, BOB, op).unwrap();
    f.work();
    f.advance(5_000);
    let (op, merged) = f.prepare(&cara, "6011111111111117", "cara again").unwrap();
    let merged_history = merged.terms.request_history;
    assert_ne!(merged_history, cara_first.terms.request_history);
    assert_ne!(merged_history, dev_first.terms.request_history);
    f.guests.cancel(&cara, BOB, op).unwrap();
    f.work();
    f.advance(5_000);
    let (_, dev_again) = f.prepare(&dev, "6011111111111117", "dev again").unwrap();
    assert_eq!(dev_again.terms.request_history, merged_history);

    // Evidence that would join two histories each holding a pending request is refused.
    let eli = f.login("eli@guest.example");
    f.prepare(&eli, "378282246310005", "eli").unwrap();
    let fay = f.login("fay@guest.example");
    f.prepare(&fay, "3530111333300000", "fay").unwrap();
    f.advance(5_000);
    assert_eq!(
        guest_error(f.prepare(&eli, "3530111333300000", "eli again")),
        GuestError::PendingRequestsConflict
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn no_two_sender_subjects_share_an_identity() {
    let mut f = Fixture::new();
    let members = f.members.clone();
    let subject = move |identity| {
        members
            .deployment()
            .sender_subject(identity)
            .unwrap()
            .unwrap()
    };
    assert!(matches!(subject(ALICE), SenderSubject::Member(_)));
    assert!(matches!(subject(BOB), SenderSubject::Member(_)));

    // A member enrolled under a guest's synthetic identity keeps it; the guest is refused
    // before anything is charged or linked.
    let mallory = Fixture::identity("mallory@guest.example");
    enroll(
        f.members.deployment(),
        3,
        mallory,
        ActorRef::Sender(mallory),
        OperationalKeyRef(31),
        f.clock.now(),
    )
    .unwrap();
    let session = f.login("mallory@guest.example");
    assert!(matches!(
        f.prepare(&session, "4242424242424242", "hello"),
        Err(ServiceError::Storage(StorageError::SenderSubjectConflict))
    ));
    assert!(matches!(subject(mallory), SenderSubject::Member(_)));

    // A guest's identity, once registered, cannot be enrolled as a member.
    let ana = f.login("ana@guest.example");
    f.prepare(&ana, "4242424242424242", "hello").unwrap();
    let identity = Fixture::identity("ana@guest.example");
    assert_eq!(
        subject(identity),
        SenderSubject::Guest(GuestMailbox::parse("ana@guest.example").unwrap())
    );
    assert!(
        enroll(
            f.members.deployment(),
            4,
            identity,
            ActorRef::Sender(identity),
            OperationalKeyRef(41),
            f.clock.now(),
        )
        .is_err()
    );
    assert_eq!(
        subject(identity),
        SenderSubject::Guest(GuestMailbox::parse("ana@guest.example").unwrap())
    );

    let mut client = f.client();
    let charged: i64 = client
        .query_one(
            "SELECT count(*) FROM cs_funding_sources WHERE guest=$1",
            &[&mallory.0.to_string()],
        )
        .unwrap()
        .get(0);
    assert_eq!(charged, 0);
    let shared: i64 = client
        .query_one(
            "SELECT count(*) FROM (SELECT identity FROM cs_sender_subjects GROUP BY identity \
             HAVING count(*) > 1 UNION ALL SELECT subject_key FROM cs_sender_subjects \
             GROUP BY subject_key HAVING count(*) > 1) duplicates",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(shared, 0);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn guest_message_text_never_reaches_the_database_in_plaintext() {
    let mut f = Fixture::new();
    let marker = "guest-plaintext-marker-5f0c1e";
    let ana = f.login("ana@guest.example");
    f.send(&ana, "4242424242424242", marker);
    f.decide("ana@guest.example", true);

    // The recipient can read it, so the marker is present in the only form allowed.
    let address = f
        .members
        .address(Fixture::identity("ana@guest.example"), BOB)
        .unwrap();
    let (record, certificate) = f
        .members
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap()
        .content_with_certificates()
        .unwrap()
        .pop()
        .unwrap();
    let (bob_secret, _) =
        cs_mail_content::EndpointSecretKey::from_secret_bytes(ContentKeyRef(22), &BOB_CONTENT)
            .unwrap();
    let plaintext =
        cs_mail_content::decrypt(&bob_secret, certificate.unwrap().certificate.key, &record)
            .unwrap();
    assert_eq!(plaintext, marker.as_bytes());

    // No table holds it as text, hexadecimal, or a JSON byte array.
    let hex = marker.bytes().fold(String::new(), |mut hex, b| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
        hex
    });
    let decimal = marker
        .bytes()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut client = f.client();
    let tables: Vec<String> = client
        .query(
            "SELECT table_name::text FROM information_schema.tables \
             WHERE table_schema=$1 AND table_type='BASE TABLE'",
            &[&f.schema],
        )
        .unwrap()
        .into_iter()
        .map(|row| row.get(0))
        .collect();
    assert!(tables.len() > 20);
    // The scan does see guest data: the mailbox is stored as text.
    let mut seen_mailbox = false;
    for table in tables {
        let rows = client
            .query(&format!("SELECT t::text FROM {table} t"), &[])
            .unwrap();
        for row in rows {
            let text: String = row.get::<_, String>(0).replace(' ', "");
            seen_mailbox |= text.contains("ana@guest.example");
            assert!(
                !text.contains(marker) && !text.contains(&hex) && !text.contains(&decimal),
                "guest plaintext in {table}"
            );
        }
    }
    assert!(seen_mailbox);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn a_guest_refund_goes_only_to_the_token_that_funded_the_request() {
    let mut f = Fixture::new();
    let token = |mailbox: &str, card: &str| {
        cards()
            .tokenize(card, &GuestMailbox::parse(mailbox).unwrap())
            .unwrap()
            .0
    };
    let charge =
        Money::from_minor_units(support::FIXTURE_PROCESSING_CHARGE + support::FIXTURE_COLLATERAL);
    let collateral = Money::from_minor_units(support::FIXTURE_COLLATERAL);

    // Accepted: the full charge returns to Ana's token.
    let ana = f.login("ana@guest.example");
    let (_, accepted) = f.send(&ana, "4242424242424242", "accept me");
    f.decide("ana@guest.example", true);

    // Cancelled before submission: the full charge returns to Ben's token.
    let ben = f.login("ben@guest.example");
    let (op, cancelled) = f.prepare(&ben, "5555555555554444", "never sent").unwrap();
    f.guests.cancel(&ben, BOB, op).unwrap();
    f.work();

    // Expired: the collateral returns to Cy's token; the processing charge is kept.
    let cy = f.login("cy@guest.example");
    let (_, expired) = f.send(&cy, "6011111111111117", "hello?");
    f.advance(DECISION_WINDOW + 1_000);
    f.work();
    f.work();

    for (mailbox, card, request, amount) in [
        ("ana@guest.example", "4242424242424242", accepted, charge),
        (
            "ben@guest.example",
            "5555555555554444",
            cancelled.request,
            charge,
        ),
        ("cy@guest.example", "6011111111111117", expired, collateral),
    ] {
        let snapshot = f.snapshot(mailbox);
        let payments = &snapshot.payments[&request];
        let funding = token(mailbox, card);
        assert_eq!(payments.capture().destination, funding);
        let refund = payments.refund().operation().expect("a refund is owed");
        assert_eq!(refund.destination, funding, "{mailbox}");
        assert_eq!(refund.amount, amount, "{mailbox}");
        assert!(!matches!(
            snapshot.state.requests[&request].lifecycle,
            RequestLifecycle::AwaitingRecipientDecision(_)
                | RequestLifecycle::PreparingSubmission(_)
        ));
    }
    // Everything the processor was asked to pay out went to one of those tokens, once each.
    let refunds: Vec<_> = f
        .processor
        .executed
        .iter()
        .filter(|operation| matches!(operation.kind, PaymentKind::Refund { .. }))
        .collect();
    assert_eq!(refunds.len(), 3);
    let expected = [
        token("ana@guest.example", "4242424242424242"),
        token("ben@guest.example", "5555555555554444"),
        token("cy@guest.example", "6011111111111117"),
    ];
    assert!(refunds.iter().all(|r| expected.contains(&r.destination)));
}

// Claim: preparation binds every input before any effect, across failure and concurrent
// retries. Changed text accepted under the same identity falsifies it and substitutes
// the message the sender authorized; an exact retry must still finish one request.
#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn guest_preparation_rejects_altered_retries_after_failure_and_concurrently() {
    let mut f = Fixture::new();
    let mailbox = "retry@guest.example";
    let session = f.login(mailbox);
    let operation = f.operation();
    let card = "4242424242424242";
    f.client().batch_execute("CREATE FUNCTION reject_guest_content() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; CREATE TRIGGER reject_guest_content BEFORE INSERT ON cs_encrypted_content FOR EACH ROW EXECUTE FUNCTION reject_guest_content()").unwrap();
    assert!(
        f.guests
            .request(
                &session,
                BOB,
                RequestClassId(1),
                card,
                "original",
                operation
            )
            .is_err()
    );
    let before = f.snapshot(mailbox);
    f.advance(cs_mail_guests::SESSION_LIFETIME.0 + 1);
    assert_eq!(
        guest_error(f.guests.request(
            &session,
            BOB,
            RequestClassId(1),
            card,
            "original",
            operation
        )),
        GuestError::SessionExpired
    );
    let session = f.login(mailbox);
    f.client().batch_execute("DROP TRIGGER reject_guest_content ON cs_encrypted_content; DROP FUNCTION reject_guest_content()").unwrap();
    assert_eq!(
        guest_error(
            f.guests
                .request(&session, BOB, RequestClassId(1), card, "changed", operation)
        ),
        GuestError::AlteredRequest
    );
    assert_eq!(f.snapshot(mailbox), before);
    let answer = f
        .guests
        .request(
            &session,
            BOB,
            RequestClassId(1),
            card,
            "original",
            operation,
        )
        .unwrap();
    assert_eq!(
        f.guests
            .request(
                &session,
                BOB,
                RequestClassId(1),
                card,
                "original",
                operation
            )
            .unwrap()
            .request,
        answer.request
    );
    assert_eq!(f.snapshot(mailbox).state.requests.len(), 1);
    assert_eq!(f.snapshot(mailbox).payments.len(), 1);

    assert_competing_inputs_conflict(&mut f, &session, mailbox, card);
}

fn assert_competing_inputs_conflict(f: &mut Fixture, session: &str, mailbox: &str, card: &str) {
    let verified = hardening::session(f, session);
    let mailbox = GuestMailbox::parse(mailbox).unwrap();
    let token = cards().tokenize(card, &mailbox).unwrap().0;
    let operation = IdempotencyKey(f.operation());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|message| {
            let fingerprint = cs_mail_guests::GuestRequestInput {
                mailbox: &mailbox,
                recipient: BOB,
                class: RequestClassId(1),
                payment_token: &token,
                message,
            }
            .fingerprint(&[4; 32], &DOMAIN, operation);
            let deployment = PostgresDeployment::connect(&f.url).unwrap();
            let barrier = barrier.clone();
            let verified = verified.clone();
            let clock = f.clock.clone();
            std::thread::spawn(move || {
                barrier.wait();
                deployment
                    .bind_guest_request(&verified, &DOMAIN, operation, fingerprint, || clock.now())
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(StorageError::Guest(GuestError::AlteredRequest))))
            .count(),
        1
    );
}

#[path = "guest_requests/hardening.rs"]
mod hardening;

#[path = "guest_requests/email_journey.rs"]
mod email_journey;
