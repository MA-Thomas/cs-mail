//! Milestone 2 claims about the core request loop, challenged through the member gateway
//! against `PostgreSQL`:
//!
//! 1. Retrying a submission whose outcome is unknown cannot create a second request or
//!    charge. Falsified by a client that loses the response and retries producing two
//!    requests or two captures. One request must remain one charge.
//! 2. Acceptance commits permission and the full refund obligation together or not at
//!    all. Falsified by a failure injected during acceptance leaving a lane or permission
//!    without its refund obligation, or the reverse. Either half alone breaks the
//!    settlement contract.
//! 3. An altered retry under an existing operation identity is rejected. Falsified by a
//!    retry with changed terms or content under the same identity being accepted. Retry
//!    identities would otherwise stop protecting request terms.
use cs_mail_test_support as support;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

use cs_mail_application::receipts::ReceivedOutcome;
use cs_mail_capabilities::{
    DeclarationAuthorityConstraint, LaneMode, LaneSubject, RateLimit, SenderScopedGrant,
    SignedLaneGrant,
};
use cs_mail_content::{
    ContentBinding, ContentKeyCertificate, EndpointSecretKey, content_key_certificate_digest,
    message_declaration_digest,
};
use cs_mail_finance::{
    BankVerification, FinancialScope, PaymentError, PaymentKind, PaymentOperation,
    PaymentProcessor, RefundView, SignedPaymentEvidence, SimulatedProcessor,
};
use cs_mail_ledger::Account;
use cs_mail_primitives::{
    BillingAccountId, CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef,
    DeclarationAuthority, DeclaredPurpose, DeliveryIntentRef, Duration, IdempotencyKey,
    KnownPurpose, LaneId, MemberId, MessageDeclarations, MessageId, MessageValidityUntil, Money,
    OperationalKeyRef, OriginDeclaration, OriginMode, PaymentOperationId, PolicyVersion,
    PrivacyProfileVersion, ProgramRef, ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId,
    RequestClassId, RequestId, RetentionPolicyVersion, SettlementUnit, Version, WireVersion,
};
use cs_mail_protocol::policy::{ProtocolPolicy, ProtocolPolicyRecord};
use cs_mail_protocol::{
    ActorRef, ProtocolCommand, RelationshipState, RequestLifecycle, SettlementSnapshot,
    TermsOutcome,
};
use cs_mail_security::{CommandSigner, SignedCommandBytes, SigningScope};
use cs_mail_service::members::{GatewaySecrets, MemberGateway};
use cs_mail_service::{CanonicalClock, ServiceError, ServiceOutcome};
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};

const UNIT: SettlementUnit = SettlementUnit(1);
const ALICE: ProtocolIdentity = ProtocolIdentity(10);
const BOB: ProtocolIdentity = ProtocolIdentity(20);
const ALICE_KEY: OperationalKeyRef = OperationalKeyRef(11);
const BOB_KEY: OperationalKeyRef = OperationalKeyRef(21);
static NEXT_SCHEMA: AtomicU64 = AtomicU64::new(1);

fn scope() -> FinancialScope {
    FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn database_url() -> String {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_loop_{}_{}_{}",
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
    format!("{base}?options=-csearch_path%3D{schema}")
}

/// Trusted time the test advances explicitly.
#[derive(Clone)]
struct Clock(Arc<AtomicU64>);
impl CanonicalClock for Clock {
    fn now(&self) -> CanonicalTime {
        CanonicalTime(self.0.load(Ordering::SeqCst))
    }
}

/// The simulator, counting the distinct capture operations it was asked to execute.
struct CountingProcessor {
    inner: SimulatedProcessor,
    captures: BTreeSet<PaymentOperationId>,
}
impl PaymentProcessor for CountingProcessor {
    fn lookup(
        &mut self,
        id: PaymentOperationId,
    ) -> Result<Option<SignedPaymentEvidence>, PaymentError> {
        self.inner.lookup(id)
    }
    fn submit(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        if operation.kind == PaymentKind::Capture {
            self.captures.insert(operation.id);
        }
        self.inner.submit(operation)
    }
    fn cancel_capture(
        &mut self,
        request: &cs_mail_finance::CaptureCancellation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        self.inner.cancel_capture(request)
    }
}

struct Loop {
    url: String,
    clock: Clock,
    gateway: MemberGateway<Clock>,
    processor: CountingProcessor,
    alice: CommandSigner,
    bob: CommandSigner,
}

fn policy() -> ProtocolPolicy {
    ProtocolPolicy::new(ProtocolPolicyRecord {
        reply_lane: cs_mail_capabilities::reply::ReplyLaneTerms::new(
            cs_mail_capabilities::reply::ReplyTermsRecord {
                lifetime: cs_mail_primitives::Duration(604_800_000),
                rate_limit: cs_mail_capabilities::RateLimit {
                    max_messages: 20,
                    interval: cs_mail_primitives::Duration(86_400_000),
                },
                purpose: cs_mail_primitives::DeclaredPurpose::Known(
                    cs_mail_primitives::KnownPurpose::Personal,
                ),
            },
        )
        .unwrap(),
        version: PolicyVersion(1),
        privacy_profile_version: PrivacyProfileVersion(1),
        retention_policy_version: RetentionPolicyVersion(1),
        submission_window: Duration(600_000),
        decision_window: Duration(600_000),
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
) {
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
    .unwrap();
    support::fixture_inbox(deployment, persona, key, &[secret; 32], at).unwrap();
}
fn gateway(url: &str, clock: &Clock) -> MemberGateway<Clock> {
    let deployment = PostgresDeployment::connect(url).unwrap();
    let record = deployment.deployment_record().unwrap().unwrap();
    let custody_clock = clock.clone();
    deployment
        .accounts(move || custody_clock.now())
        .configure_custody(
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(900), &[90; 32])
                .unwrap()
                .1,
        )
        .unwrap();
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
    .unwrap()
}

impl Loop {
    /// A bootstrapped deployment with published pricing and protocol policy, Alice enrolled
    /// as a sender and Bob as a recipient with one request class (`C = 2`, `S = 8`).
    fn new() -> Self {
        let url = database_url();
        // Inside the fixtures' 1971 service year.
        let clock = Clock(Arc::new(AtomicU64::new(44_582_400_000)));
        let at = clock.now();
        let deployment = PostgresDeployment::connect(&url).unwrap();
        support::bootstrap(&deployment, scope(), UNIT).unwrap();
        support::operator(
            &deployment,
            cs_mail_operations::OperatorCommand::PublishRequestPricing(
                support::fixture_pricing(UNIT).unwrap(),
            ),
            at,
        )
        .unwrap();
        support::operator(
            &deployment,
            cs_mail_operations::OperatorCommand::PublishProtocolPolicy(policy()),
            at,
        )
        .unwrap();
        enroll(
            &deployment,
            1,
            ALICE,
            ActorRef::Sender(ALICE),
            ALICE_KEY,
            at,
        );
        enroll(&deployment, 2, BOB, ActorRef::Recipient(BOB), BOB_KEY, at);
        for (n, key) in [(1, ALICE_KEY), (2, BOB_KEY)] {
            support::purchase_service(&deployment, scope(), UNIT, BillingAccountId(n), key, at)
                .unwrap();
        }
        let gateway = gateway(&url, &clock);
        let bob = CommandSigner::from_secret_bytes(ActorRef::Recipient(BOB), BOB_KEY, &[21; 32]);
        gateway
            .publish_classes(
                &bob.sign_request_classes(
                    gateway.scope(),
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
        Self {
            url,
            clock,
            gateway,
            processor: CountingProcessor {
                inner: SimulatedProcessor::new([7; 32]),
                captures: BTreeSet::new(),
            },
            alice: CommandSigner::from_secret_bytes(ActorRef::Sender(ALICE), ALICE_KEY, &[11; 32]),
            bob,
        }
    }
    fn advance(&self, millis: u64) {
        self.clock.0.fetch_add(millis, Ordering::SeqCst);
    }
    fn sign(
        &self,
        signer: &CommandSigner,
        key: u128,
        command: ProtocolCommand,
    ) -> SignedCommandBytes {
        let handle = self.gateway.address(ALICE, BOB).unwrap().public();
        signer
            .sign(
                SigningScope {
                    deployment_domain: [7; 32],
                    intended_provider: ProviderRef(30),
                    relationship: handle.relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(key),
                command,
            )
            .unwrap()
    }
    fn send(&self, signed: &SignedCommandBytes) -> Result<ServiceOutcome, ServiceError> {
        self.advance(1);
        self.gateway.submit(ALICE, BOB, signed)
    }
    fn snapshot(&self) -> SettlementSnapshot {
        let aggregate = self.gateway.address(ALICE, BOB).unwrap();
        self.gateway
            .deployment()
            .existing_relationship(aggregate.aggregate_key())
            .unwrap()
            .unwrap()
            .snapshot()
            .unwrap()
    }
    fn work(&mut self) {
        self.advance(1);
        self.gateway
            .run_relationship_work(&mut self.processor, Duration(30_000), 50)
            .unwrap();
    }
    fn terms(&self, key: u128) -> Box<cs_mail_protocol::RequestTerms> {
        let outcome = self
            .send(&self.sign(
                &self.alice,
                key,
                ProtocolCommand::IssueRequestTerms {
                    class_id: RequestClassId(1),
                    quote_id: QuoteId(key),
                    declaration_digest: Some(message_declaration_digest(&declarations()).unwrap()),
                },
            ))
            .unwrap();
        let ReceivedOutcome::Protocol(outcome) = outcome.outcome else {
            panic!("terms refused: {:?}", outcome.outcome)
        };
        let Some(TermsOutcome::ChargeRequired(terms)) = outcome.transition.terms_outcome else {
            panic!("expected a charge")
        };
        terms
    }
    fn create(
        &self,
        key: u128,
        terms: &cs_mail_protocol::RequestTerms,
        method: u8,
    ) -> SignedCommandBytes {
        self.sign(
            &self.alice,
            key,
            ProtocolCommand::CreateRequest {
                payment_method: [method; 32],
                request_id: RequestId(1),
                message_id: MessageId(1),
                terms: Box::new(terms.clone()),
            },
        )
    }
    /// Uploads the initial message's ciphertext, as Alice's client does.
    fn upload(&self, content: u128) -> MessageValidityUntil {
        let handle = self.gateway.address(ALICE, BOB).unwrap().public();
        let now = self.clock.now();
        let valid_until = MessageValidityUntil(CanonicalTime(now.0 + 3_600_000));
        let (alice_content, alice_public) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(12), &[12; 32]).unwrap();
        let (_, bob_public) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(22), &[22; 32]).unwrap();
        let certificate = self
            .alice
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: ProtocolVersion(2),
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: handle.relationship,
                owner: ALICE,
                operational_key: ALICE_KEY,
                content_key_version: ContentKeyVersion(1),
                key: alice_public,
                valid_from: now,
                valid_until: valid_until.0,
            })
            .unwrap();
        let document = cs_mail_client::correspondence::compose(vec![
            cs_mail_correspondence::DocumentPart::Text("Hello Bob".into()),
        ])
        .map(|doc| {
            doc.with_subject(
                cs_mail_correspondence::ConversationSubject::new(
                    "Fixture conversation",
                    cs_mail_primitives::CanonicalTime(0),
                    0,
                    [19; 32],
                )
                .unwrap(),
            )
        })
        .unwrap();
        let package = cs_mail_client::correspondence::prepare(
            &cs_mail_client::correspondence::EndpointKeys {
                owner: ALICE,
                secret: &alice_content,
                public: alice_public,
            },
            &document,
            cs_mail_correspondence::ConversationId::new(content + 1000).unwrap(),
            bob_public,
            self.gateway.custody_key().unwrap(),
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(content),
                message_id: MessageId(1),
                sender: ALICE,
                recipient: BOB,
                protocol_version: ProtocolVersion(2),
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations: declarations(),
                message_valid_until: valid_until,
                capability: None,
            },
            (now, valid_until.0),
        )
        .unwrap();
        let document = cs_mail_application::requests::documents::RequestDocument::new(
            RequestId(1),
            ContentRef(content),
            package,
            cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
        )
        .unwrap();
        let signed = cs_mail_application::requests::documents::SignedRequestDocument {
            scope: SigningScope {
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: handle.relationship,
            },
            key: ALICE_KEY,
            document,
            certificate,
            signature: Vec::new(),
        }
        .sign(&[11; 32])
        .unwrap();
        self.gateway
            .upload_request_document(ALICE, BOB, &signed)
            .unwrap();
        valid_until
    }
    fn submit(
        &self,
        key: u128,
        content: u128,
        valid_until: MessageValidityUntil,
    ) -> SignedCommandBytes {
        let version = self.snapshot().state.requests[&RequestId(1)].version;
        self.sign(
            &self.alice,
            key,
            ProtocolCommand::SubmitRequestToRecipient {
                request_id: RequestId(1),
                expected_request_version: version.into(),
                content_ref: ContentRef(content),
                delivery_intent_ref: DeliveryIntentRef(1),
                declaration_digest: message_declaration_digest(&declarations()).unwrap(),
                message_valid_until: valid_until,
            },
        )
    }
    /// Issue, create, fund and submit one request.
    fn submitted(&mut self) {
        let terms = self.terms(1);
        self.send(&self.create(2, &terms, 41)).unwrap();
        self.work();
        let valid_until = self.upload(1);
        let outcome = self.send(&self.submit(3, 1, valid_until)).unwrap();
        assert!(matches!(outcome.outcome, ReceivedOutcome::Protocol(_)));
        assert!(matches!(
            self.snapshot().state.requests[&RequestId(1)].lifecycle,
            RequestLifecycle::AwaitingRecipientDecision(_)
        ));
    }
    fn accept(&self, key: u128) -> SignedCommandBytes {
        let version = self.snapshot().state.relationship.version;
        self.sign(
            &self.bob,
            key,
            ProtocolCommand::AcceptRelationship {
                expected_version: version.into(),
            },
        )
    }
    fn lane(&self, lifetime: u64) -> SignedLaneGrant {
        let handle = self.gateway.address(ALICE, BOB).unwrap().public();
        let now = self.clock.now();
        let grant = SenderScopedGrant {
            id: LaneId(90),
            subject: LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                *handle.relationship.as_bytes(),
            )),
            sender: ALICE,
            recipient: BOB,
            protocol_version: ProtocolVersion(2),
            deployment_domain: [7; 32],
            intended_provider: ProviderRef(30),
            recipient_operational_key: BOB_KEY,
            purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
            origin: Some(OriginMode::HumanInitiated),
            declaration_authority: DeclarationAuthorityConstraint::NativeSender,
            lifetime: Duration(lifetime),
            rate_limit: RateLimit {
                max_messages: 5,
                interval: Duration(60_000),
            },
            mode: LaneMode::Expiring,
            issued_at: now,
            not_before: now,
            not_after: CanonicalTime(now.0 + lifetime),
            version: Version(1),
        };
        SignedLaneGrant {
            signature: ed25519_dalek::Signer::sign(
                &ed25519_dalek::SigningKey::from_bytes(&[21; 32]),
                &grant.signing_bytes().unwrap(),
            )
            .to_bytes(),
            grant,
        }
    }
    fn sql(&self, statements: &str) {
        postgres::Client::connect(&self.url, postgres::NoTls)
            .unwrap()
            .batch_execute(statements)
            .unwrap();
    }
    /// Refund work recorded for request 1.
    fn refund_work(&self) -> i64 {
        let snapshot = self.snapshot();
        let refund = snapshot.payments[&RequestId(1)]
            .refund()
            .operation()
            .map(|o| o.id.0.to_string());
        let Some(refund) = refund else { return 0 };
        postgres::Client::connect(&self.url, postgres::NoTls)
            .unwrap()
            .query_one(
                "SELECT count(*) FROM cs_work WHERE kind='request-payment' \
                 AND payload#>>'{Effect,ExecutePayment,operation_id}'=$1",
                &[&refund],
            )
            .unwrap()
            .get(0)
    }
}

fn declarations() -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(ALICE_KEY),
        },
        payload_schema: None,
    }
}
fn refused(outcome: &ServiceOutcome) -> bool {
    matches!(outcome.outcome, ReceivedOutcome::Refused(_))
}
fn conflict(result: &Result<ServiceOutcome, ServiceError>) -> bool {
    matches!(
        result,
        Err(ServiceError::Storage(StorageError::DuplicateConflict))
    )
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn lost_response_retries_cannot_create_a_second_request_or_charge() {
    let mut f = Loop::new();
    let terms = f.terms(1);
    let create = f.create(2, &terms, 41);
    // The response to the first attempt is lost; the client sends the same bytes again,
    // once alone and twice concurrently through independent service connections.
    let first = f.send(&create).unwrap();
    let retry = f.send(&create).unwrap();
    assert_eq!(first.receipt, retry.receipt);
    let barrier = Arc::new(Barrier::new(2));
    let concurrent: Vec<_> = (0..2)
        .map(|_| {
            let (url, clock, create, barrier) = (
                f.url.clone(),
                f.clock.clone(),
                create.clone(),
                barrier.clone(),
            );
            std::thread::spawn(move || {
                let gateway = gateway(&url, &clock);
                barrier.wait();
                gateway.submit(ALICE, BOB, &create).unwrap()
            })
        })
        .collect();
    for handle in concurrent {
        assert_eq!(handle.join().unwrap().receipt, first.receipt);
    }
    // A client that lost its journal signs the same request under a new identity.
    let resigned = f.send(&f.create(3, &terms, 41)).unwrap();
    assert!(refused(&resigned));
    f.work();
    f.work();
    let snapshot = f.snapshot();
    assert_eq!(snapshot.state.requests.len(), 1);
    assert_eq!(snapshot.payments.len(), 1);
    assert_eq!(f.processor.captures.len(), 1);
    assert_eq!(
        snapshot.ledger.balance(Account::RequestEscrow(
            snapshot.payments[&RequestId(1)].capture().id
        )),
        Money::from_minor_units(support::FIXTURE_PROCESSING_CHARGE + support::FIXTURE_COLLATERAL)
    );

    // The same holds for submission: one submission and one initial message.
    let valid_until = f.upload(1);
    let submit = f.submit(4, 1, valid_until);
    let first = f.send(&submit).unwrap();
    assert_eq!(f.send(&submit).unwrap().receipt, first.receipt);
    let snapshot = f.snapshot();
    let RequestLifecycle::AwaitingRecipientDecision(submission) =
        snapshot.state.requests[&RequestId(1)].lifecycle
    else {
        panic!("expected a submitted request")
    };
    assert_eq!(snapshot.messages.len(), 1);
    f.advance(10);
    assert_eq!(f.send(&submit).unwrap().receipt, first.receipt);
    assert_eq!(
        f.snapshot().state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::AwaitingRecipientDecision(submission)
    );
    assert_eq!(f.processor.captures.len(), 1);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)] // Three injected failures, each observed on both sides.
fn acceptance_commits_permission_and_the_full_refund_together_or_not_at_all() {
    let charge =
        Money::from_minor_units(support::FIXTURE_PROCESSING_CHARGE + support::FIXTURE_COLLATERAL);
    let unaccepted = |f: &Loop| {
        let snapshot = f.snapshot();
        assert!(matches!(
            snapshot.state.relationship.state,
            RelationshipState::Unknown
        ));
        assert!(snapshot.lane.is_none());
        assert!(matches!(
            snapshot.state.requests[&RequestId(1)].lifecycle,
            RequestLifecycle::AwaitingRecipientDecision(_)
        ));
        assert!(
            snapshot.payments[&RequestId(1)]
                .refund()
                .operation()
                .is_none()
        );
        assert_eq!(f.refund_work(), 0);
        assert!(
            snapshot
                .ledger
                .balances()
                .iter()
                .all(|(account, _)| !matches!(account, Account::RefundPayable(_)))
        );
    };
    let accepted = |f: &Loop, lane: bool| {
        let snapshot = f.snapshot();
        assert_eq!(
            matches!(
                snapshot.state.relationship.state,
                RelationshipState::ExpressLane(_)
            ),
            lane
        );
        assert_eq!(
            matches!(
                snapshot.state.relationship.state,
                RelationshipState::Accepted
            ),
            !lane
        );
        assert_eq!(snapshot.lane.is_some(), lane);
        challenge_navigation_ownership(f);
        let refund = snapshot.payments[&RequestId(1)]
            .refund()
            .operation()
            .unwrap()
            .clone();
        assert_eq!(refund.amount, charge);
        assert_eq!(f.refund_work(), 1);
        // The worker may already have paid the refund; until then the ledger owes it.
        match snapshot.payments[&RequestId(1)].view().refund {
            RefundView::Owed(amount) => {
                assert_eq!(amount, charge);
                assert_eq!(
                    snapshot.ledger.balance(Account::RefundPayable(refund.id)),
                    charge
                );
            }
            RefundView::Confirmed(amount) => assert_eq!(amount, charge),
            RefundView::None => panic!("acceptance without its refund obligation"),
        }
    };
    let fail = |f: &Loop, table: &str, event: &str, condition: &str| {
        f.sql(&format!(
            "CREATE FUNCTION cs_injected_failure() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; \
             CREATE TRIGGER cs_injected_failure BEFORE {event} ON {table} FOR EACH ROW \
             WHEN ({condition}) EXECUTE FUNCTION cs_injected_failure();"
        ));
    };
    let heal = |f: &Loop, table: &str| {
        f.sql(&format!(
            "DROP TRIGGER cs_injected_failure ON {table}; DROP FUNCTION cs_injected_failure();"
        ));
    };

    // Standing permission, failing when the refund payment work is recorded, and then
    // when the relationship's new state is written.
    for (table, event, condition) in [
        ("cs_work", "INSERT", "NEW.kind = 'request-payment'"),
        (
            "cs_relationship_aggregates",
            "UPDATE",
            "NEW.relationship_state IS DISTINCT FROM OLD.relationship_state",
        ),
    ] {
        let mut f = Loop::new();
        f.submitted();
        fail(&f, table, event, condition);
        assert!(f.send(&f.accept(10)).is_err());
        unaccepted(&f);
        heal(&f, table);
        f.work();
        accepted(&f, false);
    }

    // An express lane, failing when the refund payment work is recorded.
    let mut f = Loop::new();
    f.submitted();
    fail(&f, "cs_work", "INSERT", "NEW.kind = 'request-payment'");
    f.advance(1);
    assert!(
        f.gateway
            .grant_lane(&f.lane(86_400_000), IdempotencyKey(20))
            .is_err()
    );
    unaccepted(&f);
    heal(&f, "cs_work");
    f.work();
    accepted(&f, true);
}

// The navigation read must expose each direction only to its owner, without
// treating the recipient's view of a request as a request they sent.
fn challenge_navigation_ownership(f: &Loop) {
    use cs_mail_application::members::MemberAnswer;
    use cs_mail_security::{MemberRead, SignedMemberRead};
    let read = |persona, key, seed| {
        SignedMemberRead::sign(
            f.gateway.scope(),
            persona,
            key,
            f.clock.now(),
            MemberRead::Outbox,
            &seed,
        )
        .unwrap()
    };
    let MemberAnswer::Outbox(alice) = f.gateway.read(&read(ALICE, ALICE_KEY, [11; 32])).unwrap()
    else {
        panic!()
    };
    assert!(
        alice
            .iter()
            .all(|view| view.relationship.handle.sender == ALICE)
    );
    assert!(alice.iter().any(|view| {
        view.relationship.handle.recipient == BOB
            && view
                .relationship
                .requests
                .iter()
                .any(|request| request.id == RequestId(1))
    }));
    let MemberAnswer::Outbox(bob) = f.gateway.read(&read(BOB, BOB_KEY, [21; 32])).unwrap() else {
        panic!()
    };
    assert!(bob.iter().all(
        |view| view.relationship.handle.sender == BOB && view.relationship.requests.is_empty()
    ));
    assert!(f.gateway.read(&read(BOB, ALICE_KEY, [11; 32])).is_err());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn altered_retries_under_an_existing_operation_identity_are_rejected() {
    let mut f = Loop::new();
    let terms = f.terms(1);
    f.send(&f.create(2, &terms, 41)).unwrap();
    let before = f.snapshot();
    // Another payment method under the creation's identity.
    assert!(conflict(&f.send(&f.create(2, &terms, 42))));
    // Other terms under the creation's identity.
    let mut altered = (*terms).clone();
    altered.collateral = Money::from_minor_units(support::FIXTURE_COLLATERAL + 1);
    assert!(conflict(&f.send(&f.create(2, &altered, 41))));
    assert_eq!(f.snapshot(), before);

    f.work();
    let valid_until = f.upload(1);
    f.send(&f.submit(3, 1, valid_until)).unwrap();
    let before = f.snapshot();
    // Other content under the submission's identity.
    assert!(conflict(&f.send(&f.submit(3, 2, valid_until))));
    assert_eq!(f.snapshot(), before);

    // A lane grant, then a changed grant under the same identity.
    f.advance(1);
    f.gateway
        .grant_lane(&f.lane(86_400_000), IdempotencyKey(20))
        .unwrap();
    let before = f.snapshot();
    f.advance(1);
    assert!(matches!(
        f.gateway.grant_lane(&f.lane(3_600_000), IdempotencyKey(20)),
        Err(ServiceError::Storage(StorageError::DuplicateConflict))
    ));
    assert_eq!(f.snapshot(), before);
}

#[path = "core_loop/external_flow.rs"]
mod external_flow;
#[path = "core_loop/native.rs"]
mod native;
#[path = "core_loop/paired.rs"]
mod paired;

// Claim: reopening ingress preserves a published admission policy, and a previously
// opened gateway uses subsequent updates. Reverting policy or accepting a now-forbidden
// critical extension falsifies it and makes operator policy ineffective.
#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)] // Keep staging, policy replacement and historical retry together.
fn admission_policy_survives_reopening_and_is_checked_at_content_commit() {
    use cs_mail_primitives::{ExtensionCriticality, NamespacedIdentifier, PayloadSchema};
    use cs_mail_protocol::admission::{AdmissionFailure, AdmissionPolicy};
    let f = Loop::new();
    let terms = f.terms(1);
    f.send(&f.create(2, &terms, 41)).unwrap();
    let address = f.gateway.address(ALICE, BOB).unwrap();
    let engine = f
        .gateway
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap();
    challenge_temporary_verification(&f, address.aggregate_key());
    let schema = NamespacedIdentifier::new("com.example", "checked", 1).unwrap();
    engine
        .configure_admission_policy(&AdmissionPolicy {
            version: Version(2),
            supported_critical_schemas: [schema.clone()].into(),
        })
        .unwrap();
    let reopened = gateway(&f.url, &f.clock);
    let make = |content: u128| {
        let now = f.clock.now();
        let valid = MessageValidityUntil(CanonicalTime(now.0 + 10000));
        let (secret, public) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(12), &[12; 32]).unwrap();
        let recipient = EndpointSecretKey::from_secret_bytes(ContentKeyRef(22), &[22; 32])
            .unwrap()
            .1;
        let certificate = f
            .alice
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: ProtocolVersion(2),
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: address.public().relationship,
                owner: ALICE,
                operational_key: ALICE_KEY,
                content_key_version: ContentKeyVersion(1),
                key: public,
                valid_from: now,
                valid_until: valid.0,
            })
            .unwrap();
        let mut declarations = declarations();
        declarations.payload_schema = Some(PayloadSchema {
            id: schema.clone(),
            criticality: ExtensionCriticality::Critical,
        });
        let doc = cs_mail_client::correspondence::compose(vec![
            cs_mail_correspondence::DocumentPart::Text("checked policy".into()),
        ])
        .map(|doc| {
            doc.with_subject(
                cs_mail_correspondence::ConversationSubject::new(
                    "Fixture conversation",
                    cs_mail_primitives::CanonicalTime(0),
                    0,
                    [19; 32],
                )
                .unwrap(),
            )
        })
        .unwrap();
        let package = cs_mail_client::correspondence::prepare(
            &cs_mail_client::correspondence::EndpointKeys {
                owner: ALICE,
                secret: &secret,
                public,
            },
            &doc,
            cs_mail_correspondence::ConversationId::new(content + 1000).unwrap(),
            recipient,
            reopened.custody_key().unwrap(),
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(content),
                message_id: MessageId(1),
                sender: ALICE,
                recipient: BOB,
                protocol_version: ProtocolVersion(2),
                relationship: address.public().relationship,
                content_scope: address.public().content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations,
                message_valid_until: valid,
                capability: None,
            },
            (now, valid.0),
        )
        .unwrap();
        let document = cs_mail_application::requests::documents::RequestDocument::new(
            RequestId(1),
            ContentRef(content),
            package,
            cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
        )
        .unwrap();
        cs_mail_application::requests::documents::SignedRequestDocument {
            scope: SigningScope {
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: address.public().relationship,
            },
            key: ALICE_KEY,
            document,
            certificate,
            signature: Vec::new(),
        }
        .sign(&[11; 32])
        .unwrap()
    };
    let original = make(56);
    reopened
        .upload_request_document(ALICE, BOB, &original)
        .unwrap();
    engine
        .configure_admission_policy(&AdmissionPolicy {
            version: Version(3),
            supported_critical_schemas: BTreeSet::new(),
        })
        .unwrap();
    // Historical acknowledgement does not republish ciphertext under a new policy.
    reopened
        .upload_request_document(ALICE, BOB, &original)
        .unwrap();
    assert!(matches!(
        reopened.upload_request_document(ALICE, BOB, &make(57)),
        Err(ServiceError::Storage(StorageError::Correspondence(
            cs_mail_correspondence::CorrespondenceError::AdmissionRefused(
                AdmissionFailure::UnsupportedCriticalExtension
            )
        )))
    ));
    let mut db = postgres::Client::connect(&f.url, postgres::NoTls).unwrap();
    assert!(
        db.query_opt(
            "SELECT 1 FROM cs_request_documents WHERE content_ref='57'",
            &[]
        )
        .unwrap()
        .is_none()
    );
}

fn challenge_temporary_verification(f: &Loop, key: &str) {
    // Conjecture: temporary evidence acquisition failure is retryable and cannot admit mail.
    // A permanent denial or a canonical write would falsify it and lose valid delivery.
    use cs_mail_adapters::{
        DmarcError, DmarcErrorKind, DmarcVerificationFuture, DmarcVerifier, DomainIdentity,
        SmtpAuthenticationRequest,
    };
    struct Temporary;
    impl DmarcVerifier for Temporary {
        fn verify<'a>(&'a self, _: SmtpAuthenticationRequest<'a>) -> DmarcVerificationFuture<'a> {
            Box::pin(async {
                Err(DmarcError::new(
                    DmarcErrorKind::Temporary,
                    "fixture lookup unavailable",
                ))
            })
        }
    }
    let domain = DomainIdentity::parse_ascii("example.test").unwrap();
    let request = cs_mail_capabilities::LegacyBondFreeAdmission {
        sender: domain.synthetic_protocol_identity(&[7; 32], 1),
        recipient: BOB,
        message_id: MessageId(900),
        content_ref: ContentRef(900),
        delivery_intent_ref: DeliveryIntentRef(900),
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        payload_schema: None,
        message_valid_until: MessageValidityUntil(CanonicalTime(f.clock.now().0 + 1000)),
        capability: None,
        idempotency_key: IdempotencyKey(900),
        protocol_version: ProtocolVersion(2),
        deployment_domain: [7; 32],
        intended_provider: ProviderRef(30),
        sender_domain: domain,
    };
    let ingress = f.gateway.existing(key).unwrap().unwrap();
    let engine = f
        .gateway
        .deployment()
        .existing_relationship(key)
        .unwrap()
        .unwrap();
    let before = engine.snapshot().unwrap();
    let authentication = SmtpAuthenticationRequest {
        raw_message: b"From: sender@example.test\r\n\r\nfixture",
        remote_ip: "192.0.2.1".parse().unwrap(),
        helo_identity: "example.test",
        mail_from: Some("sender@example.test"),
        receiver_hostname: "mail.example.test",
        received_at: f.clock.now(),
    };
    let mut future = Box::pin(ingress.admit_legacy_bond_free(&Temporary, authentication, &request));
    let result = std::future::Future::poll(
        future.as_mut(),
        &mut std::task::Context::from_waker(std::task::Waker::noop()),
    );
    assert!(
        matches!(result, std::task::Poll::Ready(Err(ServiceError::Dmarc(e))) if e.is_temporary())
    );
    assert_eq!(engine.snapshot().unwrap(), before);
}
