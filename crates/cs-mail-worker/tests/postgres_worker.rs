use cs_mail_test_support as support;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use cs_mail_content::{
    ContentBinding, ContentCertificateDigest, EncryptedContentRecord, EndpointSecretKey, encrypt,
    message_declaration_digest,
};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentRef, ContentScopeRef, DeclarationAuthority,
    DeclaredPurpose, DeliveryIntentRef, Duration, IdempotencyKey, KnownPurpose,
    MessageDeclarations, MessageId, MessageValidityUntil, Money, OperationalKeyRef,
    OriginDeclaration, OriginMode, PolicyVersion, PrivacyProfileVersion, ProtocolIdentity,
    ProtocolVersion, ProviderRef, QuoteId, RelationshipRef, RequestHistoryRef, RequestId,
    RetentionPolicyVersion, SettlementUnit, Version, WireVersion,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot, ProtocolCommand, ProtocolState, TermsOutcome};
use cs_mail_security::{
    CommandSigner, KeyRegistry, ProviderSigner, SignedCommandBytes, SigningScope,
};
use cs_mail_storage_postgres::{
    DurableExecutionOutcome, PostgresDeployment, PostgresEngine, StorageError, WorkItem,
};
use cs_mail_worker::{DeliverySink, deliver_batch, run_schedule_batch};

const SENDER: ProtocolIdentity = ProtocolIdentity(102);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(103);
const PROVIDER: ProviderRef = ProviderRef(104);
const UNIT: SettlementUnit = SettlementUnit(1);
const DEPLOYMENT_DOMAIN: [u8; 32] = [7; 32];
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn database_url() -> String {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_test_{}_{}_{}",
        std::process::id(),
        NEXT_KEY.fetch_add(1, Ordering::Relaxed),
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

fn engine(url: &str, label: &str) -> PostgresEngine {
    let attempt_seed = if label == "delivery" { 1_001 } else { 1_002 };
    PostgresDeployment::connect(url)
        .unwrap()
        .relationship(
            format!(
                "worker-{label}-{}-{}",
                std::process::id(),
                NEXT_KEY.fetch_add(1, Ordering::Relaxed)
            ),
            &ProtocolState::initial_scoped(
                RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                RequestHistoryRef::from_u128_for_test(attempt_seed),
                SENDER,
                RECIPIENT,
                test_time(0),
            ),
            UNIT,
        )
        .unwrap()
}

fn policy() -> PolicySnapshot {
    PolicySnapshot {
        protocol_version: ProtocolVersion(2),
        policy_version: PolicyVersion(1),
        privacy_profile_version: PrivacyProfileVersion(1),
        retention_policy_version: RetentionPolicyVersion(1),
        recipient_provider: PROVIDER,
        unit: UNIT,
        submission_window: Duration(10),
        decision_window: Duration(50),
        quote_lifetime: Duration(20),
        backoff: vec![Duration(0), Duration(5)],
        financial: cs_mail_finance::FinancialTerms {
            scope: cs_mail_finance::FinancialScope::new(
                [7; 32],
                PROVIDER,
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            policy_version: PolicyVersion(1),
            corporate_basis_points: 300,
            maturity_delay: Duration(10),
        },
        payment_provider_key: cs_mail_finance::SimulatedProcessor::new([7; 32]).verifying_key(),
        expiry_cooldown: Duration(30),
        rejection_cooldown: Duration(90),
    }
}

fn sender_signer() -> CommandSigner {
    CommandSigner::from_secret_bytes(ActorRef::Sender(SENDER), OperationalKeyRef(1), &[1; 32])
}

fn provider_signer() -> ProviderSigner {
    ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(2), &[2; 32])
}

fn signing_scope() -> SigningScope {
    SigningScope {
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
    }
}

fn registry() -> KeyRegistry {
    let mut registry = KeyRegistry::default();
    registry
        .register(
            OperationalKeyRef(1),
            ActorRef::Sender(SENDER),
            sender_signer().verifying_key_bytes(),
            test_time(0),
        )
        .unwrap();
    registry
        .register(
            OperationalKeyRef(2),
            ActorRef::Provider(PROVIDER),
            provider_signer().verifying_key_bytes(),
            test_time(0),
        )
        .unwrap();
    registry
        .register(
            OperationalKeyRef(99),
            ActorRef::Scheduler(PROVIDER),
            CommandSigner::from_secret_bytes(
                ActorRef::Scheduler(PROVIDER),
                OperationalKeyRef(99),
                &[99; 32],
            )
            .verifying_key_bytes(),
            test_time(0),
        )
        .unwrap();
    registry
}

fn sender(command: ProtocolCommand, idempotency: u128) -> SignedCommandBytes {
    sender_signer()
        .sign(
            signing_scope(),
            ProtocolVersion(2),
            IdempotencyKey(idempotency),
            command,
        )
        .unwrap()
}

trait TestExecute {
    fn execute(
        &self,
        command: SignedCommandBytes,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError>;
}

impl TestExecute for PostgresEngine {
    fn execute(
        &self,
        command: SignedCommandBytes,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError> {
        support::provision(self, &policy, registry(), SENDER, test_time(0))?;
        self.configure_ingress(DEPLOYMENT_DOMAIN, &policy)?;
        let handle = self.receive_signed(&command, DEPLOYMENT_DOMAIN, || now, policy)?;
        let outcome = self.process_received(&handle)?;
        self.sign_artifacts_batch(&provider_signer(), test_time(0), Duration(30_000), 100)?;
        match outcome {
            cs_mail_storage_postgres::ReceivedOutcome::Protocol(o) => Ok(*o),
            cs_mail_storage_postgres::ReceivedOutcome::Refused(e) => Err(e.into_error()),
            _ => unreachable!(),
        }
    }
}

fn declarations() -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
        },
        payload_schema: None,
    }
}

fn reserve(engine: &PostgresEngine, id: u128) {
    let terms = match engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    class_id: cs_mail_primitives::RequestClassId(1),
                    quote_id: QuoteId(id),
                    declaration_digest: None,
                },
                id * 10,
            ),
            test_time(1),
            policy(),
        )
        .unwrap()
        .transition
        .terms_outcome
        .unwrap()
    {
        TermsOutcome::ChargeRequired(terms) => terms,
        TermsOutcome::NoChargeRequired | TermsOutcome::ExistingRequest(_) => {
            panic!("expected bond")
        }
    };
    engine
        .execute(
            sender(
                ProtocolCommand::CreateRequest {
                    request_id: RequestId(id),

                    message_id: MessageId(id),
                    terms,
                    payment_method: [9; 32],
                },
                id * 10 + 1,
            ),
            test_time(2),
            policy(),
        )
        .unwrap();
}

#[derive(Default)]
struct TestSink {
    fail_once: bool,
    published: BTreeSet<i64>,
}

impl DeliverySink for TestSink {
    type Error = &'static str;

    fn publish(
        &mut self,
        item: &WorkItem,
        _content: Option<&EncryptedContentRecord>,
    ) -> Result<(), Self::Error> {
        if self.fail_once
            && matches!(
                item.payload,
                cs_mail_storage_postgres::WorkPayload::Effect(
                    cs_mail_protocol::EffectIntent::DeliverMessage { .. }
                )
            )
        {
            self.fail_once = false;
            return Err("transient");
        }
        self.published.insert(item.id);
        Ok(())
    }
}

fn submit_request_to_recipient(engine: &PostgresEngine) {
    reserve(engine, 1);
    let mut provider = cs_mail_finance::SimulatedProcessor::new([7; 32]);
    cs_mail_worker::run_payment_batch(
        engine,
        &mut provider,
        test_time(2),
        Duration(10),
        10,
        &policy(),
    )
    .unwrap();
    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(2));
    let (_, public) = EndpointSecretKey::generate(ContentKeyRef(1));
    engine
        .store_trusted_content(
            &encrypt(
                &sender_content_key,
                public,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(2),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(1),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: declarations(),
                    message_valid_until: MessageValidityUntil(test_time(9)),
                    capability: None,
                },
                b"private",
                test_time(2),
                test_time(10),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    engine
        .execute(
            sender(
                ProtocolCommand::SubmitRequestToRecipient {
                    request_id: RequestId(1),
                    expected_request_version: Version(0),
                    content_ref: ContentRef(1),
                    delivery_intent_ref: DeliveryIntentRef(1),
                    declaration_digest: message_declaration_digest(&declarations()).unwrap(),
                    message_valid_until: MessageValidityUntil(test_time(9)),
                },
                12,
            ),
            test_time(3),
            policy(),
        )
        .unwrap();
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn delivery_failure_is_retried_after_lease_expiry() {
    let url = database_url();
    let engine = engine(&url, "delivery");
    submit_request_to_recipient(&engine);

    let mut sink = TestSink {
        fail_once: true,
        ..TestSink::default()
    };
    let first = deliver_batch(&engine, &mut sink, test_time(4), Duration(10), 10).unwrap();
    assert_eq!(first.retried, 1);
    assert_eq!(first.completed, 1);
    assert_eq!(
        deliver_batch(&engine, &mut sink, test_time(5), Duration(10), 10)
            .unwrap()
            .claimed,
        0
    );
    assert_eq!(engine.run_retention(test_time(10), 100).unwrap().deleted, 0);
    let report = deliver_batch(&engine, &mut sink, test_time(2004), Duration(10), 10).unwrap();
    assert_eq!(report.completed, 1);
    assert_eq!(
        engine.run_retention(test_time(2004), 100).unwrap().deleted,
        1
    );
    assert!(engine.content(ContentRef(1)).unwrap().is_none());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn due_submission_timeout_is_materialized_once() {
    let url = database_url();
    let engine = engine(&url, "schedule");
    reserve(&engine, 2);
    let report = run_schedule_batch(
        &engine,
        test_time(12),
        Duration(10),
        10,
        PROVIDER,
        OperationalKeyRef(99),
        &policy(),
    )
    .unwrap();
    assert_eq!(report.completed, 1);
    assert!(matches!(
        engine.snapshot().unwrap().state.requests[&RequestId(2)].lifecycle,
        cs_mail_protocol::RequestLifecycle::Cancelled { .. }
    ));
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)] // One end-to-end recovery sequence.
fn uncertain_capture_and_failed_refund_keep_one_operation_and_obligation() {
    use cs_mail_finance::SimulatedProcessor;
    use cs_mail_ledger::Account;
    use cs_mail_protocol::CancellationReason;
    use cs_mail_worker::run_payment_batch;
    let url = database_url();
    let engine = engine(&url, "payments");
    let mut provider = SimulatedProcessor::new([7; 32]);
    reserve(&engine, 3);
    provider.lose_next_response();
    assert_eq!(
        run_payment_batch(
            &engine,
            &mut provider,
            test_time(3),
            Duration(1),
            10,
            &policy()
        )
        .unwrap()
        .retried,
        1
    );
    assert_eq!(provider.operation_count(), 1);
    assert!(!engine.snapshot().unwrap().payments[&RequestId(3)].funding_finalized());
    let report = run_payment_batch(
        &engine,
        &mut provider,
        test_time(2004),
        Duration(1),
        10,
        &policy(),
    )
    .unwrap();
    assert_eq!(report.completed, 1);
    assert_eq!(provider.operation_count(), 1);
    engine
        .execute(
            sender(
                ProtocolCommand::CancelRequestSubmission {
                    request_id: RequestId(3),
                    expected_request_version: Version(0),
                    reason: CancellationReason::SenderRequested,
                },
                32,
            ),
            test_time(2005),
            policy(),
        )
        .unwrap();
    let request = engine.snapshot().unwrap().payments[&RequestId(3)].clone();
    let obligation = Account::RefundPayable(request.refund().operation().unwrap().id);
    assert_eq!(
        engine.snapshot().unwrap().ledger.balance(obligation),
        Money::from_minor_units(10)
    );
    provider.fail_next_submission();
    assert_eq!(
        run_payment_batch(
            &engine,
            &mut provider,
            test_time(2006),
            Duration(1),
            10,
            &policy()
        )
        .unwrap()
        .retried,
        1
    );
    assert_eq!(
        engine.snapshot().unwrap().ledger.balance(obligation),
        Money::from_minor_units(10)
    );
    run_payment_batch(
        &engine,
        &mut provider,
        test_time(4006),
        Duration(1),
        10,
        &policy(),
    )
    .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().ledger.balance(obligation),
        Money::ZERO
    );
    assert!(matches!(
        engine.snapshot().unwrap().payments[&RequestId(3)].refund(),
        cs_mail_finance::RefundStatus::Confirmed(_)
    ));
    assert_eq!(provider.operation_count(), 2);
    assert_eq!(
        run_payment_batch(
            &engine,
            &mut provider,
            test_time(4010),
            Duration(1),
            10,
            &policy()
        )
        .unwrap()
        .claimed,
        0
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn request_expiry_preserves_the_deadline_and_settles_once_afterward() {
    let url = database_url();
    let engine = engine(&url, "expiry");
    submit_request_to_recipient(&engine);
    let run = |at| {
        run_schedule_batch(
            &engine,
            test_time(at),
            Duration(10),
            10,
            PROVIDER,
            OperationalKeyRef(99),
            &policy(),
        )
        .unwrap()
    };
    assert_eq!(run(53).claimed, 0);
    assert!(
        engine.snapshot().unwrap().state.requests[&RequestId(1)]
            .lifecycle
            .is_awaiting_recipient_decision()
    );
    assert_eq!(run(54).completed, 1);
    let settled = engine.snapshot().unwrap();
    assert!(matches!(
        settled.state.requests[&RequestId(1)].lifecycle,
        cs_mail_protocol::RequestLifecycle::Expired { .. }
    ));
    let refund = settled.payments[&RequestId(1)]
        .refund()
        .operation()
        .unwrap();
    assert_eq!(refund.amount, Money::from_minor_units(8));
    assert_eq!(
        settled
            .ledger
            .balance(cs_mail_ledger::Account::RefundPayable(refund.id)),
        Money::from_minor_units(8)
    );
    assert_eq!(run(65).claimed, 0);
    assert_eq!(engine.snapshot().unwrap(), settled);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn request_expiry_recovers_an_old_refusal_without_rewriting_its_receipt() {
    let url = database_url();
    let engine = engine(&url, "old-expiry");
    submit_request_to_recipient(&engine);
    // Frozen v1 schedule identity for RequestExpiry(RequestId(1)).
    let legacy_key = IdempotencyKey(270_857_372_444_572_738_917_188_274_515_517_531_426);
    let command = CommandSigner::from_secret_bytes(
        ActorRef::Scheduler(PROVIDER),
        OperationalKeyRef(99),
        &[99; 32],
    )
    .sign(
        signing_scope(),
        ProtocolVersion(2),
        legacy_key,
        ProtocolCommand::ExpireRequest {
            request_id: RequestId(1),
            expected_request_version: Version(1),
        },
    )
    .unwrap();
    let handle = engine
        .receive_signed(&command, DEPLOYMENT_DOMAIN, || test_time(53), policy())
        .unwrap();
    let refused = engine.process_received(&handle).unwrap();
    assert!(matches!(
        refused,
        cs_mail_storage_postgres::ReceivedOutcome::Refused(
            cs_mail_storage_postgres::Refusal::Protocol(
                cs_mail_protocol::ProtocolError::DecisionWindowClosed
            )
        )
    ));
    engine
        .sign_artifacts_batch(&provider_signer(), test_time(53), Duration(10), 100)
        .unwrap();
    let original_receipt = engine.receipt(&handle).unwrap().unwrap();
    assert_eq!(
        run_schedule_batch(
            &engine,
            test_time(64),
            Duration(10),
            10,
            PROVIDER,
            OperationalKeyRef(99),
            &policy()
        )
        .unwrap()
        .completed,
        1
    );
    assert!(matches!(
        engine.snapshot().unwrap().state.requests[&RequestId(1)].lifecycle,
        cs_mail_protocol::RequestLifecycle::Expired { .. }
    ));
    assert_eq!(engine.process_received(&handle).unwrap(), refused);
    assert_eq!(engine.receipt(&handle).unwrap(), Some(original_receipt));
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn blocked_inbox_still_signs_its_quote_and_recovers_across_relationship_workers() {
    use cs_mail_storage_postgres::ReceivedOutcome;
    use cs_mail_worker::{WorkerError, run_received_batch};
    let url = database_url();
    let engine = engine(&url, "unsigned-quote");
    support::provision(&engine, &policy(), registry(), SENDER, test_time(0)).unwrap();
    engine
        .configure_ingress(DEPLOYMENT_DOMAIN, &policy())
        .unwrap();
    let issue = sender(
        ProtocolCommand::IssueRequestTerms {
            class_id: cs_mail_primitives::RequestClassId(1),
            quote_id: QuoteId(9),
            declaration_digest: None,
        },
        90,
    );
    let issued = engine
        .receive_signed(&issue, DEPLOYMENT_DOMAIN, || test_time(1), policy())
        .unwrap();
    let ReceivedOutcome::Protocol(outcome) = engine.process_received(&issued).unwrap() else {
        panic!("expected terms")
    };
    let Some(TermsOutcome::ChargeRequired(terms)) = outcome.transition.terms_outcome else {
        panic!("expected quote")
    };
    let create = sender(
        ProtocolCommand::CreateRequest {
            request_id: RequestId(9),
            message_id: MessageId(9),
            terms,
            payment_method: [9; 32],
        },
        91,
    );
    let pending = engine
        .receive_signed(&create, DEPLOYMENT_DOMAIN, || test_time(2), policy())
        .unwrap();
    let other = PostgresDeployment::connect(&url)
        .unwrap()
        .relationship(
            "other-worker",
            &ProtocolState::initial_scoped(
                RelationshipRef::from_u128_for_test(999),
                RequestHistoryRef::from_u128_for_test(999),
                SENDER,
                ProtocolIdentity(999),
                test_time(0),
            ),
            UNIT,
        )
        .unwrap();
    for host in [&other, &engine] {
        assert!(matches!(
            run_received_batch(host, &provider_signer(), test_time(3), Duration(10), 100),
            Err(WorkerError::Storage(StorageError::QuoteNotSigned))
        ));
    }
    // The owning worker reached signing despite the processing error.
    engine.signed_quote(QuoteId(9)).unwrap();
    run_received_batch(&other, &provider_signer(), test_time(4), Duration(10), 100).unwrap();
    run_received_batch(&engine, &provider_signer(), test_time(4), Duration(10), 100).unwrap();
    assert!(matches!(
        engine.process_received(&pending).unwrap(),
        ReceivedOutcome::Protocol(_)
    ));
    assert!(engine.receipt(&pending).unwrap().is_some());
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.state.requests.len(), 1);
    assert_eq!(snapshot.payments.len(), 1);
    run_received_batch(&engine, &provider_signer(), test_time(5), Duration(10), 100).unwrap();
    assert_eq!(engine.snapshot().unwrap(), snapshot);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn published_annual_period_is_finalized_by_durable_work() {
    use cs_mail_finance::*;
    use ed25519_dalek::SigningKey;
    let url = database_url();
    let engine = engine(&url, "annual");
    let secret = [42; 32];
    let provider = SimulatedProcessor::new([7; 32]);
    support::arrangement(&engine, &policy()).unwrap();
    engine
        .deployment()
        .configure_financial_program(
            policy().financial.scope,
            UNIT,
            SigningKey::from_bytes(&secret).verifying_key().to_bytes(),
            provider.verifying_key(),
        )
        .unwrap();
    let schedule = AnnualDistributionSchedule::utc(
        1971,
        EligibilityPolicy {
            version: PolicyVersion(1),
            minimum_tenure: Duration(0),
            minimum_active_days: 1,
        },
        cs_mail_finance::DistributionTerms::new(
            cs_mail_primitives::PolicyVersion(1),
            cs_mail_primitives::Money::from_minor_units(12000),
            cs_mail_primitives::calendar_date((1971) + 1, 1, 1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let command = SignedProgramCommand::sign(
        policy().financial.scope,
        UNIT,
        IdempotencyKey(1),
        0,
        ProgramCommand::PublishAnnualDistribution(schedule.clone()),
        &secret,
    )
    .unwrap();
    engine
        .deployment()
        .execute_financial_command(&command, test_time(0))
        .unwrap();
    assert_eq!(
        cs_mail_worker::run_annual_distribution_batch(
            engine.deployment(),
            UNIT,
            &|| test_time(schedule.cutoff.0 - 1),
            Duration(30_000),
            10
        )
        .unwrap()
        .claimed,
        0
    );
    assert_eq!(
        cs_mail_worker::run_annual_distribution_batch(
            engine.deployment(),
            UNIT,
            &|| schedule.cutoff,
            Duration(30_000),
            10
        )
        .unwrap()
        .completed,
        1
    );
    let finalized = engine.deployment().financial_program(UNIT).unwrap();
    assert!(finalized.distribution(schedule.id).is_some());
    assert_eq!(
        cs_mail_worker::run_annual_distribution_batch(
            engine.deployment(),
            UNIT,
            &|| schedule.cutoff,
            Duration(30_000),
            10
        )
        .unwrap()
        .claimed,
        0
    );
    assert_eq!(
        finalized,
        engine.deployment().financial_program(UNIT).unwrap()
    );
}

fn test_time(value: u64) -> CanonicalTime {
    const START: u64 = 31_536_000_000;
    CanonicalTime(if value < START { START + value } else { value })
}
