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
use cs_mail_storage_postgres::{DurableExecutionOutcome, PostgresEngine, StorageError, WorkItem};
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
    PostgresEngine::connect(
        url,
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
            CanonicalTime(0),
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
        processing_charge: Money::from_minor_units(2),
        collateral: Money::from_minor_units(8),
        admission_window: Duration(10),
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
        payment_provider_key: cs_mail_finance::SimulatedProvider::new([7; 32]).verifying_key(),
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
            CanonicalTime(0),
        )
        .unwrap();
    registry
        .register(
            OperationalKeyRef(2),
            ActorRef::Provider(PROVIDER),
            provider_signer().verifying_key_bytes(),
            CanonicalTime(0),
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
            CanonicalTime(0),
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
        self.initialize_key_registry(&registry(), CanonicalTime(0))?;
        self.configure_ingress(DEPLOYMENT_DOMAIN, &policy)?;
        let handle = self.receive_signed(&command, DEPLOYMENT_DOMAIN, || now, policy)?;
        let outcome = self.process_received(&handle)?;
        self.sign_artifacts_batch(&provider_signer(), CanonicalTime(0), Duration(30_000), 100)?;
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
                    quote_id: QuoteId(id),
                    declaration_digest: None,
                },
                id * 10,
            ),
            CanonicalTime(1),
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
            CanonicalTime(2),
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

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn delivery_failure_is_retried_after_lease_expiry() {
    let url = database_url();
    let engine = engine(&url, "delivery");
    reserve(&engine, 1);
    let mut provider = cs_mail_finance::SimulatedProvider::new([7; 32]);
    cs_mail_worker::run_payment_batch(
        &engine,
        &mut provider,
        CanonicalTime(2),
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
                    message_valid_until: MessageValidityUntil(CanonicalTime(9)),
                    capability: None,
                },
                b"private",
                CanonicalTime(2),
                CanonicalTime(10),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    engine
        .execute(
            sender(
                ProtocolCommand::AdmitRequest {
                    request_id: RequestId(1),
                    expected_request_version: Version(0),
                    content_ref: ContentRef(1),
                    delivery_intent_ref: DeliveryIntentRef(1),
                    declaration_digest: message_declaration_digest(&declarations()).unwrap(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(9)),
                },
                12,
            ),
            CanonicalTime(3),
            policy(),
        )
        .unwrap();

    let mut sink = TestSink {
        fail_once: true,
        ..TestSink::default()
    };
    let first = deliver_batch(&engine, &mut sink, CanonicalTime(4), Duration(10), 10).unwrap();
    assert_eq!(first.retried, 1);
    assert_eq!(first.completed, 1);
    assert_eq!(
        deliver_batch(&engine, &mut sink, CanonicalTime(5), Duration(10), 10)
            .unwrap()
            .claimed,
        0
    );
    assert_eq!(
        engine
            .run_retention(CanonicalTime(10), 100)
            .unwrap()
            .deleted,
        0
    );
    let report = deliver_batch(&engine, &mut sink, CanonicalTime(2004), Duration(10), 10).unwrap();
    assert_eq!(report.completed, 1);
    assert_eq!(
        engine
            .run_retention(CanonicalTime(2004), 100)
            .unwrap()
            .deleted,
        1
    );
    assert!(engine.content(ContentRef(1)).unwrap().is_none());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn due_admission_timeout_is_materialized_once() {
    let url = database_url();
    let engine = engine(&url, "schedule");
    reserve(&engine, 2);
    let report = run_schedule_batch(
        &engine,
        CanonicalTime(12),
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
    use cs_mail_finance::SimulatedProvider;
    use cs_mail_ledger::Account;
    use cs_mail_protocol::CancellationReason;
    use cs_mail_worker::run_payment_batch;
    let url = database_url();
    let engine = engine(&url, "payments");
    let mut provider = SimulatedProvider::new([7; 32]);
    reserve(&engine, 3);
    provider.lose_next_response();
    assert_eq!(
        run_payment_batch(
            &engine,
            &mut provider,
            CanonicalTime(3),
            Duration(1),
            10,
            &policy()
        )
        .unwrap()
        .retried,
        1
    );
    assert_eq!(provider.operation_count(), 1);
    assert!(!engine.snapshot().unwrap().payments[&RequestId(3)].capture_confirmed());
    let report = run_payment_batch(
        &engine,
        &mut provider,
        CanonicalTime(2004),
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
                ProtocolCommand::CancelPreparingRequest {
                    request_id: RequestId(3),
                    expected_request_version: Version(0),
                    reason: CancellationReason::SenderRequested,
                },
                32,
            ),
            CanonicalTime(2005),
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
            CanonicalTime(2006),
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
        CanonicalTime(4006),
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
            CanonicalTime(4010),
            Duration(1),
            10,
            &policy()
        )
        .unwrap()
        .claimed,
        0
    );
}
